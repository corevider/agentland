use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, bail, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Column {
    Backlog,
    Assigned,
    Working,
    Review,
    /// Reviewed, green and mergeable — waiting on nothing but a person saying
    /// yes. The difference between this and `Review` is whose turn it is.
    Ready,
    Done,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Evidence {
    Commit { sha: String, subject: String },
    Diff { files: usize, insertions: u32, deletions: u32 },
    PullRequest { url: String },
    Note { text: String },
    /// What somebody who did not write the work made of it.
    Reviewed { verdict: String, summary: String },
    /// What an agent says it did when its turn on this card ended, and what the
    /// worktree looked like when it stopped. The one entry on a card that is a
    /// report rather than a remark.
    Finished {
        summary: String,
        #[serde(default)]
        files: usize,
        #[serde(default)]
        insertions: u32,
        #[serde(default)]
        deletions: u32,
    },
}

impl Evidence {
    /// Whether this is a record of work rather than a remark about it.
    ///
    /// A card carrying a record cannot be discarded by an agent. The two used to
    /// be the same thing, and a routing note — *"Nova is the free agent with the
    /// closest role"* — was enough to make a duplicate card undeletable, which
    /// is a guard protecting nothing at the cost of a job nobody could finish.
    pub fn is_a_record(&self) -> bool {
        !matches!(self, Evidence::Note { .. })
    }
}

/// A piece of evidence, and who put it there.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Entry {
    pub what: Evidence,
    /// An agent id, `the supervisor`, or whoever else wrote it down.
    pub by: String,
    /// Seconds. Zero for entries from before anyone recorded it.
    #[serde(default)]
    pub at: u64,
}

impl Entry {
    pub fn new(what: Evidence, by: &str, at: u64) -> Self {
        Self {
            what,
            by: by.to_owned(),
            at,
        }
    }
}

/// Entries were once bare evidence with nobody's name on them, and a board that
/// came back empty because the shape changed would be worse than one that never
/// learned who did what. Both shapes are read; only the new one is written.
impl<'de> Deserialize<'de> for Entry {
    fn deserialize<D: serde::Deserializer<'de>>(reader: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Written {
            what: Evidence,
            #[serde(default)]
            by: String,
            #[serde(default)]
            at: u64,
        }

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Either {
            Attributed(Written),
            Bare(Evidence),
        }

        Ok(match Either::deserialize(reader)? {
            Either::Attributed(held) => Entry {
                what: held.what,
                by: if held.by.is_empty() {
                    "someone".to_owned()
                } else {
                    held.by
                },
                at: held.at,
            },
            Either::Bare(what) => Entry {
                what,
                by: "someone".to_owned(),
                at: 0,
            },
        })
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub body: String,
    pub column: Column,
    pub repository_id: String,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub worktree: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub evidence: Vec<Entry>,
    /// When it was written down. Zero for cards from before anyone thought to
    /// record it — shown as "no date" rather than as the epoch.
    #[serde(default)]
    pub at: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CreateTask {
    pub title: String,
    #[serde(default)]
    pub body: String,
    pub repository_id: String,
    /// The worktree this work belongs in, when it belongs in one. A step that
    /// commits to a branch can only be done where that branch is checked out.
    #[serde(default)]
    pub worktree: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MoveTask {
    pub column: Column,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct State {
    #[serde(default)]
    tasks: BTreeMap<String, Task>,
    #[serde(default)]
    next_number: u32,
}

pub struct Board {
    state: Mutex<State>,
    data_dir: PathBuf,
}

impl Board {
    pub fn new(data_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&data_dir);
        let data_dir = fs::canonicalize(&data_dir).unwrap_or(data_dir);
        let state = crate::db::load_state(&data_dir, "board");

        Self {
            state: Mutex::new(state),
            data_dir,
        }
    }

    fn persist(&self, state: &State) {
        crate::db::save_state(&self.data_dir, "board", state);
    }

    pub fn list(&self) -> Vec<Task> {
        self.state.lock().tasks.values().cloned().collect()
    }

    pub fn get(&self, id: &str) -> Option<Task> {
        self.state.lock().tasks.get(id).cloned()
    }

    pub fn create(&self, request: CreateTask) -> Result<Task> {
        if request.title.trim().is_empty() {
            bail!("a task needs a title");
        }

        let mut state = self.state.lock();
        state.next_number += 1;
        let id = format!("t{}", state.next_number);

        let task = Task {
            id: id.clone(),
            title: request.title,
            body: request.body,
            column: Column::Backlog,
            repository_id: request.repository_id,
            assignee: None,
            worktree: request.worktree,
            branch: None,
            evidence: Vec::new(),
            at: now_secs(),
        };

        state.tasks.insert(id, task.clone());
        self.persist(&state);
        Ok(task)
    }

    pub fn move_to(&self, id: &str, column: Column) -> Result<Task> {
        let mut state = self.state.lock();
        let task = state
            .tasks
            .get_mut(id)
            .ok_or_else(|| anyhow!("unknown task: {id}"))?;
        task.column = column;
        let updated = task.clone();
        self.persist(&state);
        Ok(updated)
    }

    pub fn attach(&self, id: &str, evidence: Evidence, by: &str, at: u64) -> Result<Task> {
        let mut state = self.state.lock();
        let task = state
            .tasks
            .get_mut(id)
            .ok_or_else(|| anyhow!("unknown task: {id}"))?;
        task.evidence.push(Entry::new(evidence, by, at));
        let updated = task.clone();
        self.persist(&state);
        Ok(updated)
    }

    pub fn record_assignment(
        &self,
        id: &str,
        assignee: &str,
        worktree: &str,
        branch: &str,
    ) -> Result<Task> {
        let mut state = self.state.lock();
        let task = state
            .tasks
            .get_mut(id)
            .ok_or_else(|| anyhow!("unknown task: {id}"))?;

        task.assignee = Some(assignee.to_owned());
        // A card bound to a worktree keeps that binding: it says where the work
        // belongs, which outlives whoever is holding the card.
        task.worktree
            .get_or_insert_with(|| worktree.to_owned());
        task.branch = Some(branch.to_owned());
        task.column = Column::Working;
        let updated = task.clone();
        self.persist(&state);
        Ok(updated)
    }

    /// Take a card back from whoever holds it.
    ///
    /// A card handed to the wrong agent used to be dead: dispatch refused it
    /// because it already belonged to someone, and the holder could not be told
    /// to let go, so the only way on was a fresh card and a lost history. The
    /// worktree it belongs in survives the release — that was a property of the
    /// work, not of the agent that was holding it.
    pub fn release(&self, id: &str) -> Result<Task> {
        let mut state = self.state.lock();
        let task = state
            .tasks
            .get_mut(id)
            .ok_or_else(|| anyhow!("unknown task: {id}"))?;

        let held_by = task.assignee.take();
        task.branch = None;
        task.column = Column::Backlog;

        if let Some(who) = held_by {
            task.evidence.push(Entry::new(
                Evidence::Note {
                    text: format!("taken back from {who}"),
                },
                "the board",
                now_secs(),
            ));
        }

        let updated = task.clone();
        self.persist(&state);
        Ok(updated)
    }

    /// Say which branch a card's work is on.
    pub fn record_branch(&self, id: &str, branch: &str) -> Result<Task> {
        let mut state = self.state.lock();
        let task = state
            .tasks
            .get_mut(id)
            .ok_or_else(|| anyhow!("unknown task: {id}"))?;

        task.branch = Some(branch.to_owned());
        let updated = task.clone();
        self.persist(&state);
        Ok(updated)
    }

    /// Say which worktree a card belongs in, before anyone is handed it.
    pub fn bind_to_worktree(&self, id: &str, worktree: Option<&str>) -> Result<Task> {
        let mut state = self.state.lock();
        let task = state
            .tasks
            .get_mut(id)
            .ok_or_else(|| anyhow!("unknown task: {id}"))?;

        task.worktree = worktree.map(str::to_owned);
        let updated = task.clone();
        self.persist(&state);
        Ok(updated)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let mut state = self.state.lock();
        state
            .tasks
            .remove(id)
            .ok_or_else(|| anyhow!("unknown task: {id}"))?;
        self.persist(&state);
        Ok(())
    }

    /// Throw away a card that never became anything.
    ///
    /// The crew clears its own board — leftovers from a routine that ran while
    /// nobody was watching are noise, and a commander that cannot remove them
    /// starts mislabelling them "done" instead, which is worse than clutter. But
    /// a card carrying evidence is a record of work that happened, and the crew
    /// does not get to delete those: the human does, in the app.
    pub fn discard(&self, id: &str) -> Result<Task> {
        let mut state = self.state.lock();
        let task = state
            .tasks
            .get(id)
            .ok_or_else(|| anyhow!("unknown task: {id}"))?;

        let records = task
            .evidence
            .iter()
            .filter(|entry| entry.what.is_a_record())
            .count();

        if records > 0 {
            bail!(
                "{id} carries {records} record(s) of work — only a person can remove that"
            );
        }

        let discarded = task.clone();
        state.tasks.remove(id);
        self.persist(&state);
        Ok(discarded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_board_written_before_anyone_signed_their_work_still_opens() {
        let bare: Vec<Entry> =
            serde_json::from_str(r#"[{"kind":"note","text":"taken back from ada"}]"#)
                .expect("the old shape is still read");

        assert_eq!(bare.len(), 1);
        assert_eq!(bare[0].by, "someone", "nobody is invented as the author");
        assert_eq!(bare[0].at, 0);

        let signed: Vec<Entry> = serde_json::from_str(
            r#"[{"what":{"kind":"commit","sha":"abc","subject":"feat: it"},"by":"ada","at":42}]"#,
        )
        .expect("the new shape too");

        assert_eq!(signed[0].by, "ada");
        assert_eq!(signed[0].at, 42);
    }

    #[test]
    fn a_remark_is_not_a_record_of_work() {
        let routing = Evidence::Note {
            text: "X: Nova is the free agent with the closest role".to_owned(),
        };
        assert!(!routing.is_a_record(), "a routing note records nothing");

        for held in [
            Evidence::Commit { sha: "abc".into(), subject: "feat: it".into() },
            Evidence::Diff { files: 1, insertions: 2, deletions: 0 },
            Evidence::PullRequest { url: "https://example.com/1".into() },
            Evidence::Finished { summary: "done".into(), files: 1, insertions: 2, deletions: 0 },
        ] {
            assert!(held.is_a_record(), "{held:?} is work somebody did");
        }
    }

    #[test]
    fn a_card_carrying_only_remarks_can_still_be_discarded() {
        let board = board("remarks");
        let card = board
            .create(CreateTask {
                title: "a duplicate".into(),
                body: String::new(),
                repository_id: "svc-demo".into(),
                worktree: None,
            })
            .expect("create");

        board
            .attach(
                &card.id,
                Evidence::Note { text: "X: Nova is free".into() },
                "the dispatcher",
                7,
            )
            .expect("a routing note");

        board.discard(&card.id).expect("a remark is not a record");
    }

    #[test]
    fn a_card_that_records_work_is_a_persons_to_remove() {
        let board = board("records");
        let card = board
            .create(CreateTask {
                title: "real work".into(),
                body: String::new(),
                repository_id: "svc-demo".into(),
                worktree: None,
            })
            .expect("create");

        board
            .attach(
                &card.id,
                Evidence::Finished { summary: "it works".into(), files: 2, insertions: 9, deletions: 1 },
                "ada",
                7,
            )
            .expect("a finish report");

        let refused = board.discard(&card.id).expect_err("only a person removes that");
        assert!(refused.to_string().contains("1 record"), "{refused}");
    }

    fn board(name: &str) -> Board {
        let dir = std::env::temp_dir().join(format!("agentland-board-{name}"));
        let _ = fs::remove_dir_all(&dir);
        Board::new(dir)
    }

    fn a_card(board: &Board, worktree: Option<&str>) -> Task {
        board
            .create(CreateTask {
                title: "document the endpoint".to_owned(),
                body: String::new(),
                repository_id: "demo".to_owned(),
                worktree: worktree.map(str::to_owned),
            })
            .unwrap()
    }

    #[test]
    fn a_card_someone_is_holding_can_be_found_again_after_a_restart() {
        let board = board("still-holding");
        let mine = a_card(&board, Some("ada-tree"));
        let theirs = a_card(&board, Some("ada-tree"));
        board.record_assignment(&mine.id, "ada", "ada-tree", "agent/ada-tree").unwrap();
        board.record_assignment(&theirs.id, "zen", "ada-tree", "agent/ada-tree").unwrap();
        board.move_to(&theirs.id, Column::Done).unwrap();

        let ada_is_holding: Vec<_> = board
            .list()
            .into_iter()
            .filter(|task| task.assignee.as_deref() == Some("ada"))
            .filter(|task| matches!(task.column, Column::Working))
            .collect();

        assert_eq!(ada_is_holding.len(), 1, "the one still in flight, not the finished one");
        assert_eq!(ada_is_holding[0].id, mine.id);
    }

    #[test]
    fn a_card_that_became_nothing_can_be_thrown_away() {
        let board = board("discard");
        let card = a_card(&board, None);

        let gone = board.discard(&card.id).unwrap();

        assert_eq!(gone.id, card.id);
        assert!(board.get(&card.id).is_none());
        assert!(board.discard(&card.id).is_err(), "and it is gone for good");
    }

    #[test]
    fn a_card_that_records_work_is_a_record_the_crew_may_not_delete() {
        let board = board("discard-evidence");
        let card = a_card(&board, None);
        board
            .attach(
                &card.id,
                Evidence::Commit { sha: "abc1234".into(), subject: "feat: it".into() },
                "ada",
                7,
            )
            .unwrap();

        let refused = board.discard(&card.id).unwrap_err().to_string();

        assert!(refused.contains("record"), "it says why: {refused}");
        assert!(board.get(&card.id).is_some(), "and the card is still there");
    }

    #[test]
    fn a_card_can_be_born_belonging_to_a_worktree() {
        let board = board("bound");

        let card = a_card(&board, Some("ada-tree"));

        assert_eq!(card.worktree.as_deref(), Some("ada-tree"));
        assert_eq!(card.assignee, None);
    }

    #[test]
    fn taking_a_card_back_frees_it_and_says_who_held_it() {
        let board = board("release");
        let card = a_card(&board, Some("ada-tree"));
        board
            .record_assignment(&card.id, "nova", "ada-tree", "agent/ada-tree")
            .unwrap();

        let freed = board.release(&card.id).unwrap();

        assert_eq!(freed.assignee, None, "nobody holds it now");
        assert_eq!(freed.branch, None, "the branch was the holder's, not the card's");
        assert!(matches!(freed.column, Column::Backlog));
        assert_eq!(
            freed.worktree.as_deref(),
            Some("ada-tree"),
            "where the work belongs is a property of the work",
        );
        assert!(
            freed
                .evidence
                .iter()
                .any(|held| matches!(&held.what, Evidence::Note { text } if text.contains("nova"))),
            "the card remembers who had it",
        );
    }

    #[test]
    fn a_card_nobody_held_can_still_be_taken_back() {
        let board = board("release-free");
        let card = a_card(&board, None);

        let freed = board.release(&card.id).unwrap();

        assert_eq!(freed.assignee, None);
        assert!(freed.evidence.is_empty(), "nothing to say about nobody");
    }

    #[test]
    fn where_a_card_belongs_can_be_decided_after_it_is_written() {
        let board = board("bind");
        let card = a_card(&board, None);

        let bound = board.bind_to_worktree(&card.id, Some("ada-tree")).unwrap();
        assert_eq!(bound.worktree.as_deref(), Some("ada-tree"));

        let loosened = board.bind_to_worktree(&card.id, None).unwrap();
        assert_eq!(loosened.worktree, None);
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or_default()
}
