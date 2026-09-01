use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use std::sync::Arc;

use anyhow::{bail, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::vault::{Scope, Vault};

/// A memory as the rest of the app sees it.
///
/// It is a note in the vault, and this is the shape of the fields that matter
/// when deciding what to tell an agent. `id` is the note's slug, so anything
/// holding a memory can open the file it came from.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Memory {
    pub id: String,
    pub text: String,
    pub scope: String,
    pub proposed_by: String,
    /// When it was written down, so a list of them can be read in order.
    #[serde(default)]
    pub written_at: u64,
    /// The memory it replaces, if it says.
    #[serde(default)]
    pub supersedes: Option<String>,
    /// A person approved this once and then took it back out. It is kept, and
    /// it is not told to anyone.
    #[serde(default)]
    pub retired: bool,
    #[serde(default)]
    pub approved: bool,
    #[serde(default)]
    pub masked: bool,
}

impl Memory {
    fn from_note(note: &crate::vault::Note) -> Self {
        Self {
            id: note.slug.clone(),
            text: note.body.clone(),
            scope: note
                .slug
                .rsplit_once("memory/")
                .map(|(folder, _)| folder.trim_end_matches('/').to_owned())
                .unwrap_or_default(),
            proposed_by: note.written_by.clone(),
            written_at: note.written_at,
            supersedes: note.supersedes.clone(),
            retired: note.retired,
            approved: note.approved.unwrap_or(false),
            masked: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct ProposeMemory {
    pub text: String,
    /// The memory this one replaces, by slug, when it replaces one.
    #[serde(default)]
    pub supersedes: Option<String>,
    /// Where this belongs, in the vault's own words: "shared",
    /// "workspace:atolye", "project:atolye/svc-demo".
    #[serde(default)]
    pub scope: String,
    #[serde(default = "unknown_author")]
    pub proposed_by: String,
}

fn unknown_author() -> String {
    "unknown".to_owned()
}

const REDACTED: &str = "[redacted]";

fn is_secretish(token: &str) -> bool {
    let trimmed = token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_');

    let prefixes = [
        "sk-", "sk_", "pk_", "ghp_", "gho_", "ghu_", "ghs_", "ghr_", "github_pat_", "xoxb-",
        "xoxp-", "AKIA", "ASIA", "AIza", "glpat-", "npm_", "dop_v1_", "hf_",
    ];

    if prefixes.iter().any(|prefix| trimmed.starts_with(prefix)) && trimmed.len() >= 12 {
        return true;
    }

    if trimmed.starts_with("eyJ") && trimmed.len() >= 24 {
        return true;
    }

    let long_and_mixed = trimmed.len() >= 32
        && trimmed.chars().any(|c| c.is_ascii_digit())
        && trimmed.chars().any(|c| c.is_ascii_lowercase())
        && trimmed.chars().any(|c| c.is_ascii_uppercase())
        && trimmed.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');

    long_and_mixed
}

pub fn mask_secrets(text: &str) -> (String, bool) {
    let mut masked = false;

    let cleaned: Vec<String> = text
        .split_whitespace()
        .map(|token| {
            if let Some((key, value)) = token.split_once('=') {
                if is_secretish(value) {
                    masked = true;
                    return format!("{key}={REDACTED}");
                }
            }

            if is_secretish(token) {
                masked = true;
                return REDACTED.to_owned();
            }

            token.to_owned()
        })
        .collect();

    (cleaned.join(" "), masked)
}

/// Only what markdown should not carry.
///
/// A memory's words live in the vault, where a person can read and edit them.
/// Its embedding is a few hundred floats that mean nothing to a reader and would
/// make the file unopenable, so the vectors stay here, keyed by the note's slug.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct State {
    #[serde(default)]
    vectors: BTreeMap<String, Vec<f32>>,
}

pub struct MemoryStore {
    vault: Arc<Vault>,
    state: Mutex<State>,
    data_dir: PathBuf,
}

impl MemoryStore {
    pub fn new(vault: Arc<Vault>, data_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&data_dir);
        let data_dir = fs::canonicalize(&data_dir).unwrap_or(data_dir);
        let state = crate::db::load_state(&data_dir, "memory-vectors");

        Self {
            vault,
            state: Mutex::new(state),
            data_dir,
        }
    }

    fn persist(&self, state: &State) {
        crate::db::save_state(&self.data_dir, "memory-vectors", state);
    }

    /// Move memories written before they lived in the vault.
    ///
    /// They were rows in a database nobody can open with a text editor. Each one
    /// becomes a note, keeping who proposed it and whether it was approved. The
    /// old rows are left where they are rather than deleted — a migration that
    /// eats the only copy is a migration nobody can check — and marked done so
    /// it runs once.
    pub fn take_in_what_was_kept_before(&self, workspace_of: &BTreeMap<String, String>) {
        #[derive(Default, Deserialize, Serialize)]
        struct WasKept {
            #[serde(default)]
            memories: BTreeMap<String, WasAMemory>,
            #[serde(default)]
            moved_into_the_vault: bool,
        }

        #[derive(Default, Deserialize, Serialize)]
        struct WasAMemory {
            #[serde(default)]
            text: String,
            #[serde(default)]
            scope: String,
            #[serde(default)]
            scope_id: String,
            #[serde(default)]
            proposed_by: String,
            #[serde(default)]
            approved: bool,
        }

        let mut kept: WasKept = crate::db::load_state(&self.data_dir, "memory");
        if kept.moved_into_the_vault || kept.memories.is_empty() {
            return;
        }

        let mut moved = 0;
        for memory in kept.memories.values() {
            // A memory kept against a repository belongs to that repository's
            // project folder — which means knowing whose workspace it is in.
            // When nobody claims it, the crew's own shelf is the honest place
            // for it rather than a folder named after a guess.
            let scope = match memory.scope.as_str() {
                "repository" if !memory.scope_id.is_empty() => {
                    match workspace_of.get(&memory.scope_id) {
                        Some(workspace) => Scope::Project {
                            workspace: workspace.clone(),
                            project: memory.scope_id.clone(),
                        },
                        None => Scope::Shared,
                    }
                }
                "workspace" if !memory.scope_id.is_empty() => {
                    Scope::Workspace(memory.scope_id.clone())
                }
                _ => Scope::Shared,
            };

            let by = if memory.proposed_by.is_empty() {
                "someone"
            } else {
                &memory.proposed_by
            };

            if let Ok(note) = self.vault.write_memory(&scope, &memory.text, by, None, now_secs()) {
                if memory.approved {
                    let _ = self.vault.set_approved(&note.slug, true);
                }
                moved += 1;
            }
        }

        kept.moved_into_the_vault = true;
        crate::db::save_state(&self.data_dir, "memory", &kept);
        tracing::info!(moved, "moved what was remembered into the vault");
    }

    pub fn list(&self) -> Vec<Memory> {
        self.vault
            .memories()
            .iter()
            .map(Memory::from_note)
            .collect()
    }

    /// What may be told to an agent working in this scope: what belongs to it,
    /// and what belongs to everything above it.
    pub fn approved_for(&self, scope: &Scope) -> Vec<Memory> {
        let folders = reachable_folders(scope);

        self.vault
            .memories()
            .iter()
            .filter(|note| note.approved == Some(true))
            .filter(|note| folders.iter().any(|folder| in_folder(&note.slug, folder)))
            .map(Memory::from_note)
            .collect()
    }

    pub fn remember_vector(&self, id: &str, vector: Vec<f32>) {
        let mut state = self.state.lock();
        state.vectors.insert(id.to_owned(), vector);
        self.persist(&state);
    }

    pub fn without_vectors(&self) -> Vec<Memory> {
        let state = self.state.lock();
        self.vault
            .memories()
            .iter()
            .filter(|note| note.approved == Some(true))
            .filter(|note| !state.vectors.contains_key(&note.slug))
            .map(Memory::from_note)
            .collect()
    }

    pub fn recall(
        &self,
        scope: &Scope,
        query: &str,
        query_vector: Option<&[f32]>,
        floor: f32,
        limit: usize,
    ) -> Vec<Recalled> {
        let state = self.state.lock();
        let words = tokens(query);

        let mut scored: Vec<Recalled> = self
            .approved_for(scope)
            .into_iter()
            .map(|memory| {
                let lexical = lexical_score(&words, &memory.text);
                let semantic = match (query_vector, state.vectors.get(&memory.id)) {
                    (Some(left), Some(right)) => crate::embed::cosine(left, right).max(0.0),
                    _ => 0.0,
                };

                Recalled {
                    memory,
                    lexical,
                    semantic,
                    score: if semantic >= floor {
                        0.65 * lexical + 0.35 * semantic
                    } else {
                        lexical
                    },
                }
            })
            .collect();

        if words.is_empty() && query_vector.is_none() {
            scored.truncate(limit);
            return scored;
        }

        scored.retain(|entry| entry.lexical > 0.0 || entry.semantic >= floor);
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.memory.id.cmp(&b.memory.id))
        });
        scored.truncate(limit);
        scored
    }

    /// Write down something an agent thinks the crew should be told. It is not
    /// told to anyone until a person approves it.
    pub fn propose(&self, request: ProposeMemory, scope: &Scope, now: u64) -> Result<Memory> {
        if request.text.trim().is_empty() {
            bail!("a memory needs text");
        }

        let (text, masked) = mask_secrets(&request.text);
        let note = self
            .vault
            .write_memory(scope, &text, &request.proposed_by, request.supersedes.as_deref(), now)?;

        let mut memory = Memory::from_note(&note);
        memory.masked = masked;
        Ok(memory)
    }

    /// Say yes or no to a memory.
    ///
    /// Approving one that replaces another takes the other out of the crew's
    /// brief in the same breath. Two memories that contradict each other are
    /// worse than either alone, and leaving that to a second click meant the
    /// old one stayed in force while a person hunted for it in a list with no
    /// dates on it. The replaced memory is only unapproved, never deleted.
    pub fn approve(&self, id: &str, approved: bool) -> Result<Approved> {
        let note = self.vault.set_approved(id, approved)?;
        let memory = Memory::from_note(&note);

        let replaced = match (approved, memory.supersedes.as_deref()) {
            (true, Some(older)) => self
                .vault
                .get(older)
                .filter(|held| held.approved == Some(true))
                .and_then(|_| self.vault.set_approved(older, false).ok())
                .map(|held| held.slug),
            _ => None,
        };

        Ok(Approved { memory, replaced })
    }

    pub fn forget(&self, id: &str) -> Result<()> {
        self.vault.forget(id)?;
        let mut state = self.state.lock();
        state.vectors.remove(id);
        self.persist(&state);
        Ok(())
    }
}

/// The folders a scope may be told from: its own, and everything above it.
///
/// What the crew as a whole has learned reaches every project; what a workspace
/// learned reaches the projects in it; what a project learned stays there. The
/// walk is upward only, so one project is never told another's business.
fn reachable_folders(scope: &Scope) -> Vec<String> {
    let shared = Scope::Shared.folder();

    match scope {
        Scope::Shared => vec![shared],
        Scope::Workspace(workspace) => vec![shared, workspace.clone()],
        Scope::Project { workspace, project } => vec![
            shared,
            workspace.clone(),
            format!("{workspace}/{project}"),
        ],
    }
}

/// Whether a memory written in `folder` is the one this slug belongs to.
fn in_folder(slug: &str, folder: &str) -> bool {
    slug.starts_with(&format!("{folder}/memory/"))
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or_default()
}

/// What answering a memory did: the memory itself, and the one it replaced.
#[derive(Clone, Debug, Serialize)]
pub struct Approved {
    #[serde(flatten)]
    pub memory: Memory,
    pub replaced: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Recalled {
    pub memory: Memory,
    pub score: f32,
    pub lexical: f32,
    pub semantic: f32,
}

fn tokens(text: &str) -> Vec<String> {
    text.split(|character: char| !character.is_alphanumeric() && character != '_')
        .filter(|word| word.len() > 1)
        .map(|word| word.to_lowercase())
        .collect()
}

fn exact_looking(word: &str) -> bool {
    word.contains('_') || word.chars().any(|character| character.is_ascii_digit())
}

fn lexical_score(words: &[String], text: &str) -> f32 {
    if words.is_empty() {
        return 0.0;
    }

    let held = tokens(text);
    if held.is_empty() {
        return 0.0;
    }

    let mut hits = 0.0;
    let mut weight = 0.0;

    for word in words {
        let value = if exact_looking(word) { 2.0 } else { 1.0 };
        weight += value;
        if held.iter().any(|entry| entry == word) {
            hits += value;
        }
    }

    if weight == 0.0 {
        0.0
    } else {
        hits / weight
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_the_credential_formats_that_actually_leak() {
        let cases = [
            "sk-ant-EXAMPLE-NOT-REAL",
            "ghp_EXAMPLE_NOT_REAL",
            "github_pat_EXAMPLE_NOT_REAL",
            "AKIA-EXAMPLE-NOT-REAL",
            "xoxb-EXAMPLE-NOT-REAL",
            "glpat-EXAMPLE-NOT-REAL",
            "AIza-EXAMPLE-NOT-REAL",
        ];

        for secret in cases {
            let (masked, changed) = mask_secrets(&format!("the key is {secret} keep it"));
            assert!(changed, "{secret} should be masked");
            assert!(!masked.contains(secret), "{secret} survived: {masked}");
            assert!(masked.contains("[redacted]"));
        }
    }

    #[test]
    fn masks_assignments_but_keeps_the_name() {
        let (masked, changed) =
            mask_secrets("run with ANTHROPIC_API_KEY=sk-ant-EXAMPLE-NOT-REAL");
        assert!(changed);
        assert!(masked.contains("ANTHROPIC_API_KEY=[redacted]"), "{masked}");
    }

    #[test]
    fn leaves_ordinary_prose_alone() {
        let text = "The auth service reads its config from config/auth.toml and retries twice.";
        let (masked, changed) = mask_secrets(text);
        assert!(!changed);
        assert_eq!(masked, text);
    }

    #[test]
    fn a_proposed_memory_is_not_usable_until_it_is_approved() {
        let store = super::store_at("approval");
        let here = Scope::parse("project:atolye/demo", None);
        let elsewhere = Scope::parse("project:atolye/other", None);

        let memory = store
            .propose(
                ProposeMemory {
                    text: "The database migrations live in db/migrations.".to_owned(),
                    scope: "project:atolye/demo".to_owned(),
                    supersedes: None,
                    proposed_by: "ada".to_owned(),
                },
                &here,
                0,
            )
            .expect("propose");

        assert!(!memory.approved);
        assert!(store.approved_for(&here).is_empty());

        store.approve(&memory.id, true).expect("approve");
        assert_eq!(store.approved_for(&here).len(), 1);
        assert!(store.approved_for(&elsewhere).is_empty(), "another project is not told");
    }

    #[test]
    fn what_was_kept_before_the_vault_moves_in_once() {
        use std::collections::BTreeMap;

        let dir = std::env::temp_dir().join("agentland-memory-migration");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        #[derive(serde::Serialize)]
        struct WasAMemory {
            text: String,
            scope: String,
            scope_id: String,
            proposed_by: String,
            approved: bool,
        }

        #[derive(serde::Serialize)]
        struct WasKept {
            memories: BTreeMap<String, WasAMemory>,
        }

        let mut memories = BTreeMap::new();
        memories.insert(
            "m1".to_owned(),
            WasAMemory {
                text: "the dev server reads PORT from the env".to_owned(),
                scope: "repository".to_owned(),
                scope_id: "svc-demo".to_owned(),
                proposed_by: "ada".to_owned(),
                approved: true,
            },
        );
        memories.insert(
            "m2".to_owned(),
            WasAMemory {
                text: "nobody claims this repository".to_owned(),
                scope: "repository".to_owned(),
                scope_id: "orphan".to_owned(),
                proposed_by: "ada".to_owned(),
                approved: false,
            },
        );
        crate::db::save_state(&dir, "memory", &WasKept { memories });

        let vault = Arc::new(Vault::open_at(dir.join("vault")).expect("a vault"));
        let store = MemoryStore::new(vault, dir.clone());

        let mut owners = BTreeMap::new();
        owners.insert("svc-demo".to_owned(), "atolye".to_owned());
        store.take_in_what_was_kept_before(&owners);

        let held = store.list();
        assert_eq!(held.len(), 2);
        assert!(
            held.iter().any(|memory| memory.id == "atolye/svc-demo/memory/the-dev-server-reads-port-from-the-env"
                && memory.approved),
            "filed under the project whose workspace claims it: {held:?}",
        );
        assert!(
            held.iter().any(|memory| memory.id.starts_with("shared/memory/") && !memory.approved),
            "an unclaimed repository's memory goes on the crew's own shelf: {held:?}",
        );

        store.take_in_what_was_kept_before(&owners);
        assert_eq!(store.list().len(), 2, "moving twice would write them twice");
    }

    #[test]
    fn approving_a_replacement_takes_the_old_one_out_of_the_brief() {
        let store = super::store_at("supersede");
        let older = super::keep_at(&store, "project:atolye/demo", "adding an endpoint is two changes");

        let newer = store
            .propose(
                ProposeMemory {
                    text: "adding an endpoint is three changes".to_owned(),
                    scope: "project:atolye/demo".to_owned(),
                    supersedes: Some(older.id.clone()),
                    proposed_by: "x".to_owned(),
                },
                &Scope::parse("project:atolye/demo", None),
                0,
            )
            .expect("propose");

        let answered = store.approve(&newer.id, true).expect("approve");

        assert_eq!(answered.replaced.as_deref(), Some(older.id.as_str()));

        let held = store.list();
        let old_now = held.iter().find(|m| m.id == older.id).expect("the old one is still there");
        assert!(!old_now.approved, "it is out of the brief");
        assert!(held.iter().find(|m| m.id == newer.id).unwrap().approved);
    }

    #[test]
    fn a_memory_taken_back_is_retired_rather_than_waiting_again() {
        let store = super::store_at("retire");
        let memory = super::keep_at(&store, "shared", "the reviewer prefers small commits");

        let taken_back = store.approve(&memory.id, false).expect("revoke").memory;

        assert!(!taken_back.approved);
        assert!(taken_back.retired, "it is a decision, not a fresh question");

        let back_in_force = store.approve(&memory.id, true).expect("approve").memory;
        assert!(back_in_force.approved);
        assert!(!back_in_force.retired, "it is in force again");
    }

    #[test]
    fn a_memory_nobody_ever_approved_is_not_retired() {
        let store = super::store_at("never-approved");
        let memory = store
            .propose(
                ProposeMemory {
                    text: "something nobody said yes to".to_owned(),
                    scope: "shared".to_owned(),
                    supersedes: None,
                    proposed_by: "ada".to_owned(),
                },
                &Scope::Shared,
                0,
            )
            .expect("propose");

        let refused = store.approve(&memory.id, false).expect("say no").memory;

        assert!(!refused.retired, "it was never in force, so it is still a question");
    }

    #[test]
    fn a_memory_that_replaces_nothing_replaces_nothing() {
        let store = super::store_at("supersede-none");
        let memory = store
            .propose(
                ProposeMemory {
                    text: "the reviewer prefers small commits".to_owned(),
                    scope: "shared".to_owned(),
                    supersedes: None,
                    proposed_by: "ada".to_owned(),
                },
                &Scope::Shared,
                0,
            )
            .expect("propose");

        assert_eq!(store.approve(&memory.id, true).unwrap().replaced, None);
    }

    #[test]
    fn a_memory_is_a_file_a_person_can_open() {
        let store = super::store_at("on-disk");

        let memory = super::keep_at(&store, "project:atolye/demo", "the dev server reads PORT from the env");

        assert_eq!(memory.id, "atolye/demo/memory/the-dev-server-reads-port-from-the-env");
        assert_eq!(memory.scope, "atolye/demo");
        assert!(memory.approved);
    }

    #[test]
    fn what_the_crew_shares_reaches_every_project() {
        let store = super::store_at("reach");
        super::keep_at(&store, "shared", "the reviewer prefers small commits");
        super::keep_at(&store, "workspace:atolye", "worktrees each get their own listener");
        super::keep_at(&store, "project:atolye/demo", "demo serves /health");

        let in_demo = store.approved_for(&Scope::parse("project:atolye/demo", None));
        let in_other = store.approved_for(&Scope::parse("project:atolye/other", None));
        let shared_only = store.approved_for(&Scope::Shared);

        assert_eq!(in_demo.len(), 3, "its own, its workspace's, and the crew's");
        assert_eq!(in_other.len(), 2, "not another project's");
        assert_eq!(shared_only.len(), 1, "the crew's alone");
    }
}

#[cfg(test)]
mod recall_tests {
    use super::*;

    fn store(name: &str) -> MemoryStore {
        super::store_at(&format!("recall-{name}"))
    }

    fn keep(store: &MemoryStore, text: &str) -> Memory {
        super::keep_at(store, "workspace:atolye", text)
    }

    #[test]
    fn an_exact_identifier_outranks_a_paraphrase() {
        let store = store("exact");
        let paraphrase = keep(&store, "the development server reads its port from the environment");
        let exact = keep(&store, "svc_demo reads PORT_4103 from the env");

        let found = store.recall(
            &Scope::parse("workspace:atolye", None),
            "PORT_4103", None, 0.5, 5);
        assert_eq!(found[0].memory.id, exact.id, "{found:?}");
        assert!(found.iter().all(|entry| entry.memory.id != paraphrase.id));
    }

    #[test]
    fn a_memory_that_shares_no_word_is_left_out_rather_than_ranked_last() {
        let store = store("miss");
        keep(&store, "the reviewer prefers small commits");

        assert!(store.recall(
            &Scope::parse("workspace:atolye", None),
            "port allocation", None, 0.5, 5).is_empty());
    }

    #[test]
    fn the_vector_only_breaks_ties_it_does_not_overturn_the_words() {
        let store = store("hybrid");
        let wordy = keep(&store, "the port probe scans 4100 upwards");
        let vectorish = keep(&store, "ports are chosen at worktree creation");

        store.remember_vector(&vectorish.id, vec![1.0, 0.0]);
        store.remember_vector(&wordy.id, vec![0.0, 1.0]);

        let query = [1.0, 0.0];
        let found = store.recall(
            &Scope::parse("workspace:atolye", None),
            "port probe", Some(&query), 0.5, 5);

        assert_eq!(found[0].memory.id, wordy.id, "words win: {found:?}");
        assert!(found[0].semantic == 0.0 || found[0].semantic < found[0].lexical);
    }

    #[test]
    fn a_vector_lifts_a_memory_the_words_would_have_dropped() {
        let store = store("semantic");
        let sibling = keep(&store, "worktrees each get their own listener");
        store.remember_vector(&sibling.id, vec![1.0, 0.0]);

        let words_only = store.recall(
            &Scope::parse("workspace:atolye", None),
            "port", None, 0.5, 5);
        assert!(words_only.is_empty(), "{words_only:?}");

        let query = [1.0, 0.0];
        let with_vector = store.recall(
            &Scope::parse("workspace:atolye", None),
            "port", Some(&query), 0.5, 5);
        assert_eq!(with_vector.len(), 1);
        assert!(with_vector[0].semantic > 0.9);
    }

    #[test]
    fn recall_never_reaches_outside_its_scope() {
        let store = store("scope");
        super::keep_at(&store, "project:atolye/api", "the api repo pins node 20");

        let api = Scope::parse("project:atolye/api", None);
        let web = Scope::parse("project:atolye/web", None);

        assert_eq!(store.recall(&api, "node", None, 0.5, 5).len(), 1);
        assert!(store.recall(&web, "node", None, 0.5, 5).is_empty());
    }

    #[test]
    fn an_unapproved_memory_is_never_recalled() {
        let store = store("gate");
        store
            .propose(
                ProposeMemory {
                    text: "the deploy key lives in the vault".to_owned(),
                    scope: "workspace:atolye".to_owned(),
                    supersedes: None,
                    proposed_by: "ada".to_owned(),
                },
                &Scope::parse("workspace:atolye", None),
                0,
            )
            .expect("propose");

        assert!(store.recall(
            &Scope::parse("workspace:atolye", None),
            "deploy key", None, 0.5, 5).is_empty());
    }

    #[test]
    fn the_store_can_say_which_memories_still_need_a_vector() {
        let store = store("pending");
        let first = keep(&store, "one");
        let second = keep(&store, "two");
        store.remember_vector(&first.id, vec![0.1, 0.2]);

        let pending: Vec<String> = store.without_vectors().into_iter().map(|m| m.id).collect();
        assert_eq!(pending, vec![second.id]);
    }

    #[test]
    fn an_unapproved_memory_is_never_sent_to_the_embedder() {
        let store = store("gate-embed");
        store
            .propose(
                ProposeMemory {
                    text: "the deploy key lives in the vault".to_owned(),
                    scope: "workspace:atolye".to_owned(),
                    supersedes: None,
                    proposed_by: "ada".to_owned(),
                },
                &Scope::parse("workspace:atolye", None),
                0,
            )
            .expect("propose");

        assert!(
            store.without_vectors().is_empty(),
            "the approval gate holds for the embedder too"
        );
    }
}

#[cfg(test)]
mod floor_tests {
    use super::*;

    fn store(name: &str) -> MemoryStore {
        super::store_at(&format!("floor-{name}"))
    }

    fn keep(store: &MemoryStore, text: &str) -> Memory {
        super::keep_at(store, "workspace:atolye", text)
    }

    #[test]
    fn a_faint_vector_does_not_earn_a_place_in_the_brief() {
        let store = store("faint");
        let unrelated = keep(&store, "the reviewer prefers small commits");
        store.remember_vector(&unrelated.id, vec![0.2, 0.98]);

        let query = [1.0, 0.0];
        let strict = store.recall(
            &Scope::parse("workspace:atolye", None),
            "port", Some(&query), 0.5, 5);
        assert!(strict.is_empty(), "a weak neighbour is not a memory: {strict:?}");

        let loose = store.recall(
            &Scope::parse("workspace:atolye", None),
            "port", Some(&query), 0.1, 5);
        assert_eq!(loose.len(), 1, "lowering the floor lets it back in");
    }

    #[test]
    fn a_word_match_never_needs_to_clear_the_floor() {
        let store = store("words");
        let named = keep(&store, "the port probe scans upwards from 4100");
        store.remember_vector(&named.id, vec![0.0, 1.0]);

        let query = [1.0, 0.0];
        let found = store.recall(
            &Scope::parse("workspace:atolye", None),
            "port probe", Some(&query), 0.9, 5);
        assert_eq!(found.len(), 1, "the words are enough on their own");
        assert_eq!(found[0].memory.id, named.id);
        assert_eq!(found[0].score, found[0].lexical, "a vector below the floor is ignored");
    }
}

#[cfg(test)]
fn store_at(name: &str) -> MemoryStore {
    let dir = std::env::temp_dir().join(format!("agentland-memory-{name}"));
    let _ = fs::remove_dir_all(&dir);
    let vault = Arc::new(Vault::open_at(dir.join("vault")).expect("a vault"));
    MemoryStore::new(vault, dir)
}

#[cfg(test)]
fn keep_at(store: &MemoryStore, scope: &str, text: &str) -> Memory {
    let memory = store
        .propose(
            ProposeMemory {
                text: text.to_owned(),
                scope: scope.to_owned(),
                supersedes: None,
                proposed_by: "ada".to_owned(),
            },
            &Scope::parse(scope, Some("atolye")),
            0,
        )
        .expect("propose");
    store.approve(&memory.id, true).expect("approve").memory
}
