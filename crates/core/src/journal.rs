use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

/// One thing the app decided or did.
///
/// `tracing` says these to a terminal nobody is reading and forgets them when
/// the app closes. The board carries what happened to a card and the notices
/// carry what a person should look at, but neither answers *who woke the
/// commander, how often, and why* — which is the question a bill asks.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Entry {
    pub at: u64,
    /// What kind of thing happened, in a dotted name: `card.assigned`,
    /// `commander.woken`, `budget.tightened`.
    pub kind: String,
    /// Who did it — an agent id, `the supervisor`, `the dispatcher`, a person.
    pub actor: String,
    /// What it was done to: a card, an agent, a project. Empty when it was
    /// about nothing in particular.
    #[serde(default)]
    pub subject: String,
    /// Why, in the words that would be said to a person.
    #[serde(default)]
    pub detail: String,
}

/// What a reader is asking for.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Ask {
    /// A dotted prefix: `card` matches `card.assigned` and `card.queued`.
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub actor: Option<String>,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub since: Option<u64>,
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Whether an entry is one this reader asked for.
///
/// The kind is matched by prefix on whole segments, so `card` finds
/// `card.assigned` and `card.moved` but not `cardboard.folded`. Everything else
/// is an exact match: an actor is a name, not a search.
pub fn wanted(entry: &Entry, ask: &Ask) -> bool {
    if let Some(kind) = &ask.kind {
        let matches = entry.kind == *kind
            || entry
                .kind
                .strip_prefix(kind.as_str())
                .is_some_and(|rest| rest.starts_with('.'));
        if !matches {
            return false;
        }
    }

    if ask.actor.as_ref().is_some_and(|who| *who != entry.actor) {
        return false;
    }

    if ask.subject.as_ref().is_some_and(|what| *what != entry.subject) {
        return false;
    }

    if ask.since.is_some_and(|floor| entry.at < floor) {
        return false;
    }

    true
}

/// How many entries are kept. Old enough to answer "what happened last week",
/// small enough that reading it is not a job.
const KEEP: usize = 20_000;

/// How far past the cap it is allowed to drift before being trimmed. Rewriting
/// the file on every write would make the journal the most expensive thing in
/// the app.
const SLACK: usize = 2_000;

pub struct Journal {
    path: PathBuf,
    /// Entries written since the last trim, so the trim is not a line count of
    /// the whole file on every append.
    since_trim: Mutex<usize>,
}

impl Journal {
    pub fn new(data_dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&data_dir);

        Self {
            path: data_dir.join("journal.jsonl"),
            since_trim: Mutex::new(0),
        }
    }

    /// Write one thing down.
    ///
    /// Failing to write a journal entry is never a reason to fail the thing
    /// being journalled, so this returns nothing and complains to the log.
    pub fn write(&self, kind: &str, actor: &str, subject: &str, detail: &str, at: u64) {
        let entry = Entry {
            at,
            kind: kind.to_owned(),
            actor: actor.to_owned(),
            subject: subject.to_owned(),
            detail: detail.to_owned(),
        };

        let Ok(line) = serde_json::to_string(&entry) else {
            return;
        };

        let appended = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .and_then(|mut file| writeln!(file, "{line}"));

        if let Err(error) = appended {
            tracing::warn!(%error, "the journal could not be written to");
            return;
        }

        let mut since = self.since_trim.lock();
        *since += 1;
        if *since >= SLACK {
            *since = 0;
            drop(since);
            self.trim();
        }
    }

    /// Read what was asked for, newest first.
    pub fn read(&self, ask: &Ask) -> Vec<Entry> {
        let Ok(file) = File::open(&self.path) else {
            return Vec::new();
        };

        let mut found: Vec<Entry> = BufReader::new(file)
            .lines()
            .map_while(Result::ok)
            .filter_map(|line| serde_json::from_str::<Entry>(&line).ok())
            .filter(|entry| wanted(entry, ask))
            .collect();

        found.reverse();
        found.truncate(ask.limit.unwrap_or(200));
        found
    }

    /// How many entries the journal holds.
    pub fn len(&self) -> usize {
        File::open(&self.path)
            .map(|file| BufReader::new(file).lines().count())
            .unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn trim(&self) {
        let Ok(file) = File::open(&self.path) else {
            return;
        };

        let lines: Vec<String> = BufReader::new(file).lines().map_while(Result::ok).collect();
        if lines.len() <= KEEP {
            return;
        }

        let kept = &lines[lines.len() - KEEP..];
        let written = std::fs::write(&self.path, format!("{}\n", kept.join("\n")));

        if let Err(error) = written {
            tracing::warn!(%error, "the journal could not be trimmed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn journal(name: &str) -> Journal {
        let dir = std::env::temp_dir().join(format!("agentland-journal-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        Journal::new(dir)
    }

    fn entry(kind: &str, actor: &str, subject: &str, at: u64) -> Entry {
        Entry {
            at,
            kind: kind.to_owned(),
            actor: actor.to_owned(),
            subject: subject.to_owned(),
            detail: String::new(),
        }
    }

    #[test]
    fn what_is_written_can_be_read_back_newest_first() {
        let held = journal("roundtrip");
        held.write("card.assigned", "the dispatcher", "t12", "ada was free", 100);
        held.write("commander.woken", "the supervisor", "x", "a step settled", 200);

        let all = held.read(&Ask::default());
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].kind, "commander.woken", "newest first");
        assert_eq!(all[1].detail, "ada was free");
    }

    #[test]
    fn a_kind_is_matched_on_whole_segments() {
        let assigned = entry("card.assigned", "the dispatcher", "t12", 1);

        let ask = |kind: &str| Ask { kind: Some(kind.to_owned()), ..Ask::default() };
        assert!(wanted(&assigned, &ask("card")), "a family is a prefix");
        assert!(wanted(&assigned, &ask("card.assigned")), "and so is the whole name");

        assert!(!wanted(&assigned, &ask("car")), "half a word is not a family");
        assert!(!wanted(&entry("cardboard.folded", "x", "", 1), &ask("card")));
    }

    #[test]
    fn everything_else_is_an_exact_match() {
        let woken = entry("commander.woken", "the supervisor", "x", 100);

        assert!(wanted(&woken, &Ask { actor: Some("the supervisor".into()), ..Ask::default() }));
        assert!(!wanted(&woken, &Ask { actor: Some("the".into()), ..Ask::default() }),
            "an actor is a name, not a search");
        assert!(wanted(&woken, &Ask { subject: Some("x".into()), ..Ask::default() }));
        assert!(!wanted(&woken, &Ask { subject: Some("x-desk".into()), ..Ask::default() }));
    }

    #[test]
    fn since_keeps_what_came_after_it() {
        let woken = entry("commander.woken", "the supervisor", "x", 100);

        assert!(wanted(&woken, &Ask { since: Some(100), ..Ask::default() }), "its own second counts");
        assert!(wanted(&woken, &Ask { since: Some(99), ..Ask::default() }));
        assert!(!wanted(&woken, &Ask { since: Some(101), ..Ask::default() }));
    }

    #[test]
    fn a_reader_asking_for_one_family_gets_only_that_family() {
        let held = journal("filtered");
        held.write("card.assigned", "the dispatcher", "t1", "", 10);
        held.write("card.queued", "the dispatcher", "t2", "no room", 20);
        held.write("commander.woken", "the supervisor", "x", "", 30);

        let cards = held.read(&Ask { kind: Some("card".into()), ..Ask::default() });
        assert_eq!(cards.len(), 2);
        assert!(cards.iter().all(|held| held.kind.starts_with("card.")));
    }

    #[test]
    fn a_limit_takes_the_newest_rather_than_the_first() {
        let held = journal("limited");
        for i in 0..10 {
            held.write("tick", "clock", "", "", i);
        }

        let some = held.read(&Ask { limit: Some(3), ..Ask::default() });
        assert_eq!(some.len(), 3);
        assert_eq!(some[0].at, 9, "the newest is the one worth having");
        assert_eq!(some[2].at, 7);
    }

    #[test]
    fn a_journal_nobody_has_written_to_reads_as_empty() {
        let held = journal("fresh");

        assert!(held.is_empty());
        assert_eq!(held.read(&Ask::default()).len(), 0);
    }

    #[test]
    fn a_line_that_is_not_an_entry_does_not_stop_the_rest() {
        let held = journal("corrupt");
        held.write("one", "someone", "", "", 1);

        // A half-written line from a process that died mid-append.
        let mut file = OpenOptions::new().append(true).open(&held.path).unwrap();
        writeln!(file, "{{\"at\": 2, \"kind\": ").unwrap();
        drop(file);

        held.write("two", "someone", "", "", 3);

        let all = held.read(&Ask::default());
        assert_eq!(all.len(), 2, "the readable entries survive the unreadable one");
    }
}
