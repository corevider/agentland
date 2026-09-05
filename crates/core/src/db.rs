use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use anyhow::{Context, Result};
use parking_lot::Mutex;
use rusqlite::Connection;
use serde::de::DeserializeOwned;
use serde::Serialize;

pub struct Database {
    connection: Mutex<Connection>,
    path: PathBuf,
}

impl Database {
    pub fn open(data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir)?;
        let path = data_dir.join("agentland.db");

        let connection = Connection::open(&path)
            .with_context(|| format!("cannot open {}", path.display()))?;

        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;

        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS documents (
                collection TEXT NOT NULL,
                id         TEXT NOT NULL,
                body       TEXT NOT NULL,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY (collection, id)
            );
            CREATE INDEX IF NOT EXISTS documents_by_collection
                ON documents (collection, updated_at);",
        )?;

        Ok(Self {
            connection: Mutex::new(connection),
            path,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load<T: DeserializeOwned>(&self, collection: &str) -> Result<Vec<(String, T)>> {
        let connection = self.connection.lock();
        let mut statement =
            connection.prepare("SELECT id, body FROM documents WHERE collection = ?1 ORDER BY id")?;

        let rows = statement.query_map([collection], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        let mut items = Vec::new();
        for row in rows {
            let (id, body) = row?;
            match serde_json::from_str::<T>(&body) {
                Ok(value) => items.push((id, value)),
                Err(error) => tracing::warn!(%collection, %id, %error, "skipping unreadable row"),
            }
        }

        Ok(items)
    }

    pub fn load_one<T: DeserializeOwned>(&self, collection: &str, id: &str) -> Result<Option<T>> {
        let connection = self.connection.lock();
        let mut statement =
            connection.prepare("SELECT body FROM documents WHERE collection = ?1 AND id = ?2")?;

        let mut rows = statement.query([collection, id])?;
        match rows.next()? {
            Some(row) => {
                let body: String = row.get(0)?;
                Ok(Some(serde_json::from_str(&body)?))
            }
            None => Ok(None),
        }
    }

    pub fn put<T: Serialize>(&self, collection: &str, id: &str, value: &T) -> Result<()> {
        let body = serde_json::to_string(value)?;
        let connection = self.connection.lock();

        connection.execute(
            "INSERT INTO documents (collection, id, body, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (collection, id) DO UPDATE SET body = ?3, updated_at = ?4",
            rusqlite::params![collection, id, body, now_secs()],
        )?;

        Ok(())
    }

    pub fn put_all<T: Serialize>(&self, collection: &str, items: &[(String, T)]) -> Result<()> {
        let mut connection = self.connection.lock();
        let transaction = connection.transaction()?;
        let stamp = now_secs();

        transaction.execute("DELETE FROM documents WHERE collection = ?1", [collection])?;

        {
            let mut statement = transaction.prepare(
                "INSERT INTO documents (collection, id, body, updated_at) VALUES (?1, ?2, ?3, ?4)",
            )?;

            for (id, value) in items {
                statement.execute(rusqlite::params![
                    collection,
                    id,
                    serde_json::to_string(value)?,
                    stamp
                ])?;
            }
        }

        transaction.commit()?;
        Ok(())
    }

    pub fn delete(&self, collection: &str, id: &str) -> Result<()> {
        let connection = self.connection.lock();
        connection.execute(
            "DELETE FROM documents WHERE collection = ?1 AND id = ?2",
            [collection, id],
        )?;
        Ok(())
    }

    pub fn count(&self, collection: &str) -> Result<usize> {
        let connection = self.connection.lock();
        let count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM documents WHERE collection = ?1",
            [collection],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs() as i64)
        .unwrap_or_default()
}

static SHARED: OnceLock<Mutex<HashMap<PathBuf, Arc<Database>>>> = OnceLock::new();

impl Database {
    pub fn shared(data_dir: &Path) -> Arc<Database> {
        let key = crate::exec::settled(data_dir);
        let registry = SHARED.get_or_init(|| Mutex::new(HashMap::new()));

        let mut open = registry.lock();
        if let Some(existing) = open.get(&key) {
            return Arc::clone(existing);
        }

        let database = Arc::new(
            Database::open(&key).unwrap_or_else(|error| panic!("cannot open the state database at {}: {error}", key.display())),
        );
        open.insert(key, Arc::clone(&database));
        database
    }
}

pub fn load_state<T: DeserializeOwned + Default + Serialize>(data_dir: &Path, name: &str) -> T {
    let database = Database::shared(data_dir);

    match database.load_one::<T>(STATE, name) {
        Ok(Some(state)) => return state,
        Ok(None) => {}
        Err(error) => tracing::warn!(%name, %error, "cannot read state, starting empty"),
    }

    match import_legacy_json::<T>(data_dir, name) {
        Some(state) => {
            if let Err(error) = database.put(STATE, name, &state) {
                tracing::warn!(%name, %error, "imported state could not be stored");
            }
            state
        }
        None => T::default(),
    }
}

pub fn save_state<T: Serialize>(data_dir: &Path, name: &str, state: &T) {
    if let Err(error) = Database::shared(data_dir).put(STATE, name, state) {
        tracing::error!(%name, %error, "cannot save state");
    }
}

const STATE: &str = "state";

fn import_legacy_json<T: DeserializeOwned>(data_dir: &Path, name: &str) -> Option<T> {
    let path = data_dir.join(format!("{name}.json"));
    let raw = std::fs::read_to_string(&path).ok()?;

    match serde_json::from_str::<T>(&raw) {
        Ok(state) => {
            let kept = path.with_extension("json.imported");
            match std::fs::rename(&path, &kept) {
                Ok(()) => tracing::info!(%name, kept = %kept.display(), "imported state into the database"),
                Err(error) => tracing::warn!(%name, %error, "imported state but could not rename the file"),
            }
            Some(state)
        }
        Err(error) => {
            tracing::error!(%name, %error, path = %path.display(), "leaving unreadable state file in place");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq, Serialize)]
    struct Note {
        text: String,
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("agentland-db-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_document_survives_reopening() {
        let dir = scratch("reopen");
        {
            let database = Database::open(&dir).expect("open");
            database
                .put("notes", "n1", &Note { text: "keep me".into() })
                .expect("put");
        }

        let database = Database::open(&dir).expect("reopen");
        let loaded: Option<Note> = database.load_one("notes", "n1").expect("load");
        assert_eq!(loaded, Some(Note { text: "keep me".into() }));
    }

    #[test]
    fn writing_a_collection_replaces_it_atomically() {
        let dir = scratch("replace");
        let database = Database::open(&dir).expect("open");

        database
            .put_all(
                "notes",
                &[
                    ("a".to_owned(), Note { text: "one".into() }),
                    ("b".to_owned(), Note { text: "two".into() }),
                ],
            )
            .expect("put_all");
        assert_eq!(database.count("notes").expect("count"), 2);

        database
            .put_all("notes", &[("c".to_owned(), Note { text: "three".into() })])
            .expect("replace");

        let rows: Vec<(String, Note)> = database.load("notes").expect("load");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "c");
    }

    #[test]
    fn a_deleted_document_is_gone() {
        let dir = scratch("delete");
        let database = Database::open(&dir).expect("open");

        database.put("notes", "n1", &Note { text: "bye".into() }).expect("put");
        database.delete("notes", "n1").expect("delete");

        let loaded: Option<Note> = database.load_one("notes", "n1").expect("load");
        assert_eq!(loaded, None);
    }

    #[test]
    fn unreadable_rows_are_skipped_rather_than_failing_the_load() {
        let dir = scratch("corrupt");
        let database = Database::open(&dir).expect("open");

        database.put("notes", "good", &Note { text: "fine".into() }).expect("put");
        {
            let connection = database.connection.lock();
            connection
                .execute(
                    "INSERT INTO documents (collection, id, body, updated_at) VALUES ('notes', 'bad', '{oops', 0)",
                    [],
                )
                .expect("insert junk");
        }

        let rows: Vec<(String, Note)> = database.load("notes").expect("load");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "good");
    }

    #[derive(Debug, Default, Deserialize, PartialEq, Serialize)]
    struct Roster {
        #[serde(default)]
        names: Vec<String>,
    }

    #[test]
    fn a_legacy_json_file_is_imported_once_and_set_aside() {
        let dir = scratch("import");
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(
            dir.join("crew.json"),
            r#"{"names":["Ada","Nova","Rex"]}"#,
        )
        .expect("seed legacy file");

        let imported: Roster = load_state(&dir, "crew");
        assert_eq!(imported.names, vec!["Ada", "Nova", "Rex"]);
        assert!(!dir.join("crew.json").exists(), "the legacy file is set aside");
        assert!(dir.join("crew.json.imported").exists(), "and kept as a backup");

        let again: Roster = load_state(&dir, "crew");
        assert_eq!(again.names, vec!["Ada", "Nova", "Rex"], "reads from the database now");
    }

    #[test]
    fn an_unreadable_legacy_file_is_left_in_place_rather_than_destroyed() {
        let dir = scratch("import-broken");
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(dir.join("board.json"), "{ truncated").expect("seed");

        let state: Roster = load_state(&dir, "board");
        assert_eq!(state, Roster::default());
        assert!(dir.join("board.json").exists(), "the operator can still recover it");
    }

    #[test]
    fn saved_state_is_read_back_by_a_second_store_on_the_same_directory() {
        let dir = scratch("shared");
        save_state(&dir, "mail", &Roster { names: vec!["X".into()] });

        let read_back: Roster = load_state(&dir, "mail");
        assert_eq!(read_back.names, vec!["X"]);
    }
}
