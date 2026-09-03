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

/// What a machine that has never been told anything starts with.
///
/// A blank box on a first run means every agent works to whatever its engine
/// happens to prefer. This is the least somebody would have written themselves,
/// short enough to read in a sitting and meant to be edited: it is a starting
/// point, not a position.
pub const STARTING_POINT: &str = "\
# House rules

These hold for every agent, in every project, on every turn.

## Work

- Prefer the simple thing. Complexity has to earn its place.
- Follow the conventions already in the project when they differ from these.
- Keep a function, a file and a change about one thing.
- Optimise when something is measured to be slow, not when it looks slow.

## Names and shape

- Name things for what they are, in words a newcomer would use.
- snake_case for variables and functions where the language allows it;
  PascalCase for types.
- Four spaces, unless the project or the language says otherwise.

## Saying why

- Write code that does not need a comment to be understood.
- Where a comment is worth having, say why rather than what.
- English in code, in commits, in errors and in tests.

## Proving it

- Change behaviour, change a test. A behaviour nobody tested is a behaviour
  nobody promised.
- Cover what would actually break, including the awkward edges.
- Do not delete a test to make a change pass.

## Commits

- `<type>(<scope>): <subject>`, in the imperative, no full stop.
- One change per commit; unrelated changes go in their own.
- A commit that is not obvious gets a body saying what and why.

## Care

- Never a secret, a token or a key in the code, in a log or in an error.
- Check what comes from outside before believing it.
- Fail in a way that says what to do next.
";

impl Standards {
    pub fn new(data_dir: PathBuf) -> Self {
        let held: Held = crate::db::load_state(&data_dir, "standards");
        let text = held.text.unwrap_or_else(|| STARTING_POINT.to_owned());

        let standing = Self {
            text: Mutex::new(text.trim().to_owned()),
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
                text: Some(trimmed.to_owned()),
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

/// `None` means nobody has said anything yet, which is not the same as
/// somebody clearing the rules on purpose — that is `Some("")`, and it sticks.
#[derive(Default, serde::Deserialize, serde::Serialize)]
struct Held {
    #[serde(default)]
    text: Option<String>,
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
    fn a_machine_that_has_never_been_told_anything_starts_with_something() {
        let dir = scratch("first-run");
        let held = Standards::new(dir);

        assert_eq!(held.read(), STARTING_POINT.trim());
        assert!(held.file().is_some(), "an agent starting today is held to them");
    }

    #[test]
    fn clearing_them_on_purpose_sticks_across_a_restart() {
        let dir = scratch("cleared-sticks");

        {
            let held = Standards::new(dir.clone());
            held.set("").unwrap();
        }

        let reopened = Standards::new(dir);

        assert_eq!(reopened.read(), "", "somebody said no rules, and meant it");
        assert_eq!(reopened.file(), None);
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

        held.set("").unwrap();
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
        held.set("Four spaces.").unwrap();

        assert!(held.set(&"x".repeat(MOST + 1)).is_err());
        assert_eq!(held.read(), "Four spaces.", "and what stood before it still stands");
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
