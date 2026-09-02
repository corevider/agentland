use std::collections::BTreeMap;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

/// What a person wants doing in a project, kept where the app can read it.
///
/// A goal used to be a sentence typed into a pane. That works until the pane
/// is not there any more: measured, a commander was handed a goal, its pane
/// filled while it read, the app traded it for a fresh session, and the goal
/// went with the conversation — thirty-three minutes at an empty prompt, no
/// cards, no plan. Nothing could hand it over again because nothing had it.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Goal {
    pub repository_id: String,
    pub text: String,
    /// Who asked for it. A person, by name or by "a person".
    pub set_by: String,
    pub at: u64,
}

#[derive(Default, Deserialize, Serialize)]
struct State {
    #[serde(default)]
    by_project: BTreeMap<String, Goal>,
}

pub struct Goals {
    state: Mutex<State>,
    data_dir: std::path::PathBuf,
}

/// A goal is a paragraph, not an essay: it is typed into a pane every time a
/// commander comes back, and a wall of text costs a turn each time.
const MOST: usize = 2_000;

impl Goals {
    pub fn new(data_dir: std::path::PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&data_dir);
        let state = crate::db::load_state(&data_dir, "goals");

        Self {
            state: Mutex::new(state),
            data_dir,
        }
    }

    /// Write down what a project is for. Replaces whatever stood before it:
    /// one project, one thing being asked for at a time.
    pub fn set(&self, repository_id: &str, text: &str, set_by: &str, at: u64) -> Option<Goal> {
        let trimmed = text.trim();
        if trimmed.is_empty() || trimmed.len() > MOST {
            return None;
        }

        let goal = Goal {
            repository_id: repository_id.to_owned(),
            text: trimmed.to_owned(),
            set_by: set_by.to_owned(),
            at,
        };

        let mut state = self.state.lock();
        state.by_project.insert(repository_id.to_owned(), goal.clone());
        let snapshot = State {
            by_project: state.by_project.clone(),
        };
        drop(state);

        crate::db::save_state(&self.data_dir, "goals", &snapshot);
        Some(goal)
    }

    pub fn for_project(&self, repository_id: &str) -> Option<Goal> {
        self.state.lock().by_project.get(repository_id).cloned()
    }

    pub fn everything(&self) -> Vec<Goal> {
        self.state.lock().by_project.values().cloned().collect()
    }

    /// Say it is done, or that it was never the thing. Returns whether there
    /// was one to clear.
    pub fn clear(&self, repository_id: &str) -> bool {
        let mut state = self.state.lock();
        if state.by_project.remove(repository_id).is_none() {
            return false;
        }

        let snapshot = State {
            by_project: state.by_project.clone(),
        };
        drop(state);

        crate::db::save_state(&self.data_dir, "goals", &snapshot);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(name: &str) -> Goals {
        let dir = std::env::temp_dir().join(format!("agentland-goals-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        Goals::new(dir)
    }

    #[test]
    fn a_goal_outlives_the_pane_it_was_said_to() {
        let dir = std::env::temp_dir().join("agentland-goals-restart");
        let _ = std::fs::remove_dir_all(&dir);

        {
            let held = Goals::new(dir.clone());
            held.set("svc-demo", "count requests that matched no route", "a person", 10);
        }

        let reopened = Goals::new(dir);
        let goal = reopened.for_project("svc-demo").expect("it was written down");

        assert_eq!(goal.text, "count requests that matched no route");
        assert_eq!(goal.set_by, "a person");
    }

    #[test]
    fn one_project_holds_one_goal_and_the_newer_replaces_it() {
        let held = store("replace");

        held.set("svc-demo", "the first thing", "a person", 10);
        held.set("svc-demo", "no, this thing", "a person", 20);

        let goal = held.for_project("svc-demo").expect("a goal");
        assert_eq!(goal.text, "no, this thing");
        assert_eq!(held.everything().len(), 1);
    }

    #[test]
    fn projects_do_not_share_a_goal() {
        let held = store("apart");

        held.set("svc-demo", "one thing", "a person", 10);
        held.set("ccdo", "another", "a person", 10);

        assert_eq!(held.for_project("svc-demo").unwrap().text, "one thing");
        assert_eq!(held.for_project("ccdo").unwrap().text, "another");
    }

    #[test]
    fn nothing_and_an_essay_are_both_refused() {
        let held = store("refused");

        assert!(held.set("svc-demo", "   ", "a person", 10).is_none());
        assert!(held.set("svc-demo", &"x".repeat(MOST + 1), "a person", 10).is_none());
        assert!(held.for_project("svc-demo").is_none());
    }

    #[test]
    fn clearing_says_whether_there_was_anything_to_clear() {
        let held = store("clear");

        held.set("svc-demo", "a thing", "a person", 10);

        assert!(held.clear("svc-demo"));
        assert!(!held.clear("svc-demo"), "twice is not an error worth reporting");
        assert!(held.for_project("svc-demo").is_none());
    }
}
