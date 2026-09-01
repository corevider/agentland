use std::collections::VecDeque;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

/// What the crew wants the human to know, and where it came from.
///
/// A notice is not a log line: it is addressed to a person who is looking at
/// something else. So it says which workspace and which agent it came from, and
/// carries the id of the thing to open — a person reading "Ada finished" wants
/// to be taken to Ada, not to be told a fact.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Notice {
    pub id: u64,
    pub kind: Kind,
    pub text: String,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub repository_id: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    /// Where the human lands when they click it: a panel, and what to focus.
    #[serde(default)]
    pub opens: Option<String>,
    pub at: u64,
    #[serde(default)]
    pub seen: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Something needs the human before work can go on.
    Waiting,
    /// Something finished.
    Finished,
    /// Something went wrong.
    Trouble,
    /// Worth knowing, nothing to do.
    Word,
}

impl Kind {
    /// Whether this is the kind that should light the bell rather than sit in
    /// the list. Only the two that change what the human would do next.
    pub fn is_loud(self) -> bool {
        matches!(self, Kind::Waiting | Kind::Trouble)
    }
}

#[derive(Debug, Default)]
struct State {
    notices: VecDeque<Notice>,
    next_id: u64,
}

/// The last few hundred notices, newest first. Deliberately not persisted: a
/// notice is about now, and a list of yesterday's interruptions is noise.
pub struct Notices {
    state: Mutex<State>,
    keep: usize,
}

impl Default for Notices {
    fn default() -> Self {
        Self::new(200)
    }
}

impl Notices {
    pub fn new(keep: usize) -> Self {
        Self {
            state: Mutex::new(State::default()),
            keep,
        }
    }

    pub fn push(&self, notice: NewNotice, now: u64) -> Notice {
        let mut state = self.state.lock();
        state.next_id += 1;

        let notice = Notice {
            id: state.next_id,
            kind: notice.kind,
            text: notice.text,
            workspace_id: notice.workspace_id,
            repository_id: notice.repository_id,
            agent_id: notice.agent_id,
            opens: notice.opens,
            at: now,
            seen: false,
        };

        state.notices.push_front(notice.clone());
        while state.notices.len() > self.keep {
            state.notices.pop_back();
        }

        notice
    }

    pub fn list(&self, limit: usize) -> Vec<Notice> {
        self.state.lock().notices.iter().take(limit).cloned().collect()
    }

    /// How many the human has not looked at, and whether any of them is the kind
    /// that should interrupt.
    pub fn unseen(&self) -> (usize, bool) {
        let state = self.state.lock();
        let unseen: Vec<&Notice> = state.notices.iter().filter(|notice| !notice.seen).collect();
        (unseen.len(), unseen.iter().any(|notice| notice.kind.is_loud()))
    }

    pub fn mark_seen(&self, ids: &[u64]) {
        let mut state = self.state.lock();
        for notice in state.notices.iter_mut() {
            if ids.is_empty() || ids.contains(&notice.id) {
                notice.seen = true;
            }
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct NewNotice {
    pub kind: Kind,
    pub text: String,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub repository_id: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub opens: Option<String>,
}

impl Default for Kind {
    fn default() -> Self {
        Kind::Word
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(text: &str) -> NewNotice {
        NewNotice {
            kind: Kind::Word,
            text: text.to_owned(),
            ..NewNotice::default()
        }
    }

    #[test]
    fn the_newest_thing_is_the_first_thing_read() {
        let notices = Notices::default();
        notices.push(word("first"), 10);
        notices.push(word("second"), 20);

        let held = notices.list(10);
        assert_eq!(held[0].text, "second");
        assert_eq!(held[1].text, "first");
    }

    #[test]
    fn only_what_changes_what_the_human_would_do_next_is_loud() {
        assert!(Kind::Waiting.is_loud());
        assert!(Kind::Trouble.is_loud());
        assert!(!Kind::Finished.is_loud());
        assert!(!Kind::Word.is_loud());
    }

    #[test]
    fn the_bell_says_how_many_and_whether_any_of_them_can_wait() {
        let notices = Notices::default();
        notices.push(word("a finished step"), 10);
        assert_eq!(notices.unseen(), (1, false), "finished work is not an interruption");

        notices.push(
            NewNotice {
                kind: Kind::Waiting,
                text: "Ada is asking".into(),
                ..NewNotice::default()
            },
            20,
        );
        assert_eq!(notices.unseen(), (2, true));
    }

    #[test]
    fn looking_at_them_is_what_clears_them() {
        let notices = Notices::default();
        let one = notices.push(word("one"), 10);
        notices.push(word("two"), 20);

        notices.mark_seen(&[one.id]);
        assert_eq!(notices.unseen().0, 1);

        notices.mark_seen(&[]);
        assert_eq!(notices.unseen().0, 0, "an empty list means all of them");
    }

    #[test]
    fn a_notice_remembers_where_it_came_from_so_it_can_be_opened() {
        let notices = Notices::default();
        notices.push(
            NewNotice {
                kind: Kind::Finished,
                text: "Ada finished the health endpoint".into(),
                workspace_id: Some("w1".into()),
                repository_id: Some("agentland-svc-demo".into()),
                agent_id: Some("ada".into()),
                opens: Some("agent:ada".into()),
            },
            30,
        );

        let held = notices.list(1).remove(0);
        assert_eq!(held.agent_id.as_deref(), Some("ada"));
        assert_eq!(held.workspace_id.as_deref(), Some("w1"));
        assert_eq!(held.opens.as_deref(), Some("agent:ada"));
    }

    #[test]
    fn the_list_does_not_grow_forever() {
        let notices = Notices::new(3);
        for step in 0..6 {
            notices.push(word(&format!("notice {step}")), step);
        }

        let held = notices.list(10);
        assert_eq!(held.len(), 3);
        assert_eq!(held[0].text, "notice 5", "the newest survive");
    }
}
