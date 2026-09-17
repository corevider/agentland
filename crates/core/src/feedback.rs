//! Durable repair feedback. An event is acknowledged only after delivery, so
//! restarting the app does not silently lose a failed check or review request.
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Message {
    pub id: String,
    pub task: String,
    pub agent: String,
    pub text: String,
    pub delivered: bool,
}
#[derive(Default, Deserialize, Serialize)]
struct State {
    messages: Vec<Message>,
    attempts: BTreeMap<String, usize>,
    held: BTreeSet<String>,
    seen: BTreeSet<String>,
}
pub struct Feedback {
    path: PathBuf,
    state: Mutex<State>,
}
#[derive(Debug, PartialEq)]
pub enum Queued {
    New,
    Duplicate,
    Escalate,
    Held,
}

impl Feedback {
    pub fn new(root: PathBuf) -> Self {
        let path = root.join("repair-feedback.json");
        let state = std::fs::read(&path)
            .ok()
            .and_then(|raw| serde_json::from_slice(&raw).ok())
            .unwrap_or_default();
        Self {
            path,
            state: Mutex::new(state),
        }
    }
    fn save(&self, state: &State) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = self.path.with_extension("json.part");
        std::fs::write(&tmp, serde_json::to_vec(state)?)?;
        std::fs::rename(tmp, &self.path)?;
        Ok(())
    }
    pub fn enqueue(
        &self,
        task: &str,
        agent: &str,
        signature: &str,
        text: &str,
    ) -> anyhow::Result<Queued> {
        let mut state = self.state.lock();
        let key = format!("{task}:{signature}");
        if state.seen.contains(&key) {
            return Ok(Queued::Duplicate);
        }
        if state.held.contains(task) {
            return Ok(Queued::Held);
        }
        let attempts = state.attempts.get(task).copied().unwrap_or(0);
        state.seen.insert(key);
        if attempts >= 3 {
            state.held.insert(task.into());
            // Feedback already waiting must not start more repairs after escalation.
            for message in &mut state.messages {
                if message.task == task {
                    message.delivered = true;
                }
            }
            self.save(&state)?;
            return Ok(Queued::Escalate);
        }
        state.attempts.insert(task.into(), attempts + 1);
        state.messages.push(Message {
            id: crate::generate_token(),
            task: task.into(),
            agent: agent.into(),
            text: text.into(),
            delivered: false,
        });
        self.save(&state)?;
        Ok(Queued::New)
    }
    pub fn notify(&self, task: &str, agent: &str, text: &str) -> anyhow::Result<()> {
        let mut state = self.state.lock();
        state.messages.push(Message {
            id: crate::generate_token(),
            task: task.into(),
            agent: agent.into(),
            text: text.into(),
            delivered: false,
        });
        self.save(&state)
    }
    pub fn notify_once(&self, task: &str, agent: &str, text: &str) -> anyhow::Result<()> {
        let mut state = self.state.lock();
        if state
            .messages
            .iter()
            .any(|m| m.task == task && m.agent == agent && m.text == text)
        {
            return Ok(());
        }
        state.messages.push(Message {
            id: crate::generate_token(),
            task: task.into(),
            agent: agent.into(),
            text: text.into(),
            delivered: false,
        });
        self.save(&state)
    }
    pub fn pending(&self) -> Vec<Message> {
        self.state
            .lock()
            .messages
            .iter()
            .filter(|m| !m.delivered)
            .cloned()
            .collect()
    }
    pub fn acknowledge(&self, ids: &[String]) -> anyhow::Result<()> {
        let mut state = self.state.lock();
        for message in &mut state.messages {
            if ids.contains(&message.id) {
                message.delivered = true;
            }
        }
        self.save(&state)
    }
    pub fn reset(&self, task: &str) -> anyhow::Result<()> {
        let mut state = self.state.lock();
        state.held.remove(task);
        state.attempts.remove(task);
        state
            .seen
            .retain(|key| !key.starts_with(&format!("{task}:")));
        self.save(&state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn feedback_survives_restart_deduplicates_and_stops_after_three_repairs() {
        let root = std::env::temp_dir().join(format!("feedback-{}", crate::generate_token()));
        let store = Feedback::new(root.clone());
        assert_eq!(
            store.enqueue("t1", "ada", "sha1:ci", "fix CI").unwrap(),
            Queued::New
        );
        let store = Feedback::new(root.clone());
        assert_eq!(store.pending().len(), 1);
        assert_eq!(
            store.enqueue("t1", "ada", "sha1:ci", "fix CI").unwrap(),
            Queued::Duplicate
        );
        let id = store.pending()[0].id.clone();
        store.acknowledge(&[id]).unwrap();
        assert!(Feedback::new(root.clone()).pending().is_empty());
        assert_eq!(
            store.enqueue("t1", "ada", "sha2:ci", "fix CI").unwrap(),
            Queued::New
        );
        assert_eq!(
            store.enqueue("t1", "ada", "sha3:ci", "fix CI").unwrap(),
            Queued::New
        );
        assert_eq!(
            store.enqueue("t1", "ada", "sha4:ci", "fix CI").unwrap(),
            Queued::Escalate
        );
        assert!(store.pending().is_empty());
        assert_eq!(
            Feedback::new(root.clone())
                .enqueue("t1", "ada", "sha5:ci", "fix CI")
                .unwrap(),
            Queued::Held
        );
        store.reset("t1").unwrap();
        assert_eq!(
            store.enqueue("t1", "ada", "sha4:ci", "fix CI").unwrap(),
            Queued::New
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
