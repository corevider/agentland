use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{bail, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Message {
    pub id: String,
    pub from: String,
    pub to: String,
    pub text: String,
    #[serde(default)]
    pub delivered: bool,
    /// When it was sent. Zero for messages from before this was recorded.
    #[serde(default)]
    pub at: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SendMessage {
    pub from: String,
    pub to: String,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MailPolicy {
    #[serde(default)]
    pub paused: bool,
    #[serde(default = "allow")]
    pub allow_unlisted: bool,
    #[serde(default)]
    pub grants: BTreeMap<String, Vec<String>>,
}

fn allow() -> bool {
    true
}

impl Default for MailPolicy {
    fn default() -> Self {
        Self {
            paused: false,
            allow_unlisted: true,
            grants: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct State {
    #[serde(default)]
    policy: MailPolicy,
    #[serde(default)]
    messages: Vec<Message>,
    #[serde(default)]
    next_number: u32,
}

pub struct Mailbox {
    state: Mutex<State>,
    data_dir: PathBuf,
}

pub fn permits(policy: &MailPolicy, from: &str, to: &str) -> Result<()> {
    if policy.paused {
        bail!("agent-to-agent messaging is paused");
    }

    if from == to {
        bail!("an agent cannot message itself");
    }

    match policy.grants.get(from) {
        Some(allowed) => {
            if allowed.iter().any(|entry| entry == "*" || entry == to) {
                Ok(())
            } else {
                bail!("{from} is not granted to message {to}")
            }
        }
        None => {
            if policy.allow_unlisted {
                Ok(())
            } else {
                bail!("{from} has no messaging grants")
            }
        }
    }
}

impl Mailbox {
    pub fn new(data_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&data_dir);
        let data_dir = crate::exec::settled(&data_dir);
        let state = crate::db::load_state(&data_dir, "mail");

        Self {
            state: Mutex::new(state),
            data_dir,
        }
    }

    fn persist(&self, state: &State) {
        crate::db::save_state(&self.data_dir, "mail", state);
    }

    pub fn policy(&self) -> MailPolicy {
        self.state.lock().policy.clone()
    }

    pub fn set_policy(&self, policy: MailPolicy) -> MailPolicy {
        let mut state = self.state.lock();
        state.policy = policy;
        let updated = state.policy.clone();
        self.persist(&state);
        updated
    }

    pub fn messages(&self) -> Vec<Message> {
        self.state.lock().messages.clone()
    }

    pub fn send(&self, request: SendMessage) -> Result<Message> {
        if request.text.trim().is_empty() {
            bail!("a message needs text");
        }

        let mut state = self.state.lock();
        permits(&state.policy, &request.from, &request.to)?;

        state.next_number += 1;
        let message = Message {
            id: format!("msg{}", state.next_number),
            from: request.from,
            to: request.to,
            text: request.text,
            delivered: false,
            at: now_secs(),
        };

        state.messages.push(message.clone());
        self.persist(&state);
        Ok(message)
    }

    pub fn take_inbox(&self, agent_id: &str) -> Vec<Message> {
        let mut state = self.state.lock();
        let mut taken = Vec::new();

        for message in state.messages.iter_mut() {
            if message.to == agent_id && !message.delivered {
                message.delivered = true;
                taken.push(message.clone());
            }
        }

        if !taken.is_empty() {
            self.persist(&state);
        }

        taken
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pause_stops_every_handoff() {
        let policy = MailPolicy {
            paused: true,
            ..MailPolicy::default()
        };
        assert!(permits(&policy, "ada", "rex").is_err());
    }

    #[test]
    fn an_explicit_grant_restricts_who_can_be_reached() {
        let mut policy = MailPolicy::default();
        policy.grants.insert("ada".to_owned(), vec!["rex".to_owned()]);

        assert!(permits(&policy, "ada", "rex").is_ok());
        assert!(permits(&policy, "ada", "worker2").is_err());
        assert!(permits(&policy, "rex", "worker2").is_ok());
    }

    #[test]
    fn unlisted_senders_can_be_denied_wholesale() {
        let policy = MailPolicy {
            allow_unlisted: false,
            ..MailPolicy::default()
        };
        assert!(permits(&policy, "ada", "rex").is_err());
    }

    #[test]
    fn an_inbox_is_delivered_once() {
        let dir = std::env::temp_dir().join("agentland-mail-test");
        let _ = fs::remove_dir_all(&dir);
        let mailbox = Mailbox::new(dir);

        mailbox
            .send(SendMessage {
                from: "ada".to_owned(),
                to: "rex".to_owned(),
                text: "the auth branch is ready for review".to_owned(),
            })
            .expect("send");

        assert_eq!(mailbox.take_inbox("rex").len(), 1);
        assert!(mailbox.take_inbox("rex").is_empty());
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or_default()
}
