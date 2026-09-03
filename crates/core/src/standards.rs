use std::path::{Path, PathBuf};

use parking_lot::Mutex;

/// How the house works, handed to every agent that starts.
///
/// Not a brief and not a memory: a brief is one piece of work and a memory is
/// something learned. This is the standing instruction — how to name things,
/// what a commit message looks like, what never goes in a log — and it has to
/// hold for every agent, in every project, without being said again each time.
pub struct Standards {
    text: Mutex<String>,
    data_dir: PathBuf,
}

/// Long enough for a page of house rules, short enough that it cannot quietly
/// become the whole prompt.
pub const MOST: usize = 16_000;

impl Standards {
    pub fn new(data_dir: PathBuf) -> Self {
        let held: Held = crate::db::load_state(&data_dir, "standards");
        let text = held.text;

        let standing = Self {
            text: Mutex::new(text),
            data_dir,
        };
        standing.write_out();
        standing
    }

    pub fn read(&self) -> String {
        self.text.lock().clone()
    }

    /// Refuses an essay. Everything else, including nothing at all, is somebody
    /// deciding what the house rules are.
    pub fn set(&self, text: &str) -> anyhow::Result<()> {
        let trimmed = text.trim();
        if trimmed.len() > MOST {
            anyhow::bail!("house rules are a page, not a book: {MOST} characters at most");
        }

        *self.text.lock() = trimmed.to_owned();
        crate::db::save_state(
            &self.data_dir,
            "standards",
            &Held {
                text: trimmed.to_owned(),
            },
        );
        self.write_out();
        Ok(())
    }

    /// Where the rules are on disk for an engine to read, or nothing when there
    /// are none. Handed to the engine as a file rather than as an argument: a
    /// page of rules on a command line is a page of rules in every process
    /// listing.
    pub fn file(&self) -> Option<PathBuf> {
        let file = self.path();
        file.is_file().then_some(file)
    }

    fn path(&self) -> PathBuf {
        self.data_dir.join("standards").join("CLAUDE.md")
    }

    fn write_out(&self) {
        let text = self.text.lock().clone();
        let file = self.path();

        if text.is_empty() {
            let _ = std::fs::remove_file(&file);
            return;
        }

        if let Some(folder) = file.parent() {
            let _ = std::fs::create_dir_all(folder);
        }

        let _ = std::fs::write(&file, text);
    }
}

#[derive(Default, serde::Deserialize, serde::Serialize)]
struct Held {
    #[serde(default)]
    text: String,
}

/// What an engine that cannot be handed a file is told instead.
///
/// The rules go at the top of the brief, once, above the work: an engine that
/// takes no standing instruction still has to know how the house works.
pub fn spoken(text: &str, brief: &str) -> String {
    if text.trim().is_empty() {
        return brief.to_owned();
    }

    format!("How this house works, and it holds for everything below:\n\n{text}\n\n---\n\n{brief}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("agentland-standards-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn what_is_set_is_read_back_and_survives_a_restart() {
        let dir = scratch("round-trip");

        {
            let held = Standards::new(dir.clone());
            held.set("Use four spaces. Say why, not what.").unwrap();
        }

        let reopened = Standards::new(dir);
        assert_eq!(reopened.read(), "Use four spaces. Say why, not what.");
    }

    #[test]
    fn the_rules_are_on_disk_for_an_engine_to_read() {
        let dir = scratch("file");
        let held = Standards::new(dir);

        assert_eq!(held.file(), None, "nothing set is nothing to hand over");

        held.set("Four spaces.").unwrap();
        let file = held.file().expect("a file once there are rules");

        assert_eq!(std::fs::read_to_string(file).unwrap(), "Four spaces.");
    }

    #[test]
    fn clearing_them_takes_the_file_away_too() {
        let dir = scratch("cleared");
        let held = Standards::new(dir);

        held.set("Four spaces.").unwrap();
        held.set("   ").unwrap();

        assert_eq!(held.read(), "");
        assert_eq!(held.file(), None, "an engine must not read rules nobody holds");
    }

    #[test]
    fn a_book_is_refused_because_it_would_become_the_whole_prompt() {
        let dir = scratch("book");
        let held = Standards::new(dir);

        assert!(held.set(&"x".repeat(MOST + 1)).is_err());
        assert_eq!(held.read(), "");
    }

    #[test]
    fn an_engine_that_takes_no_file_is_told_at_the_top_of_the_brief() {
        let said = spoken("Four spaces.", "Serve /metrics from server.js");

        assert!(said.starts_with("How this house works"));
        assert!(said.contains("Four spaces."));
        assert!(said.ends_with("Serve /metrics from server.js"));
    }

    #[test]
    fn no_rules_leave_the_brief_exactly_as_it_was() {
        assert_eq!(spoken("  ", "Do the thing"), "Do the thing");
    }
}
