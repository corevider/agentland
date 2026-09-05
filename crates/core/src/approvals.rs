use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, bail, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Pending,
    Approved,
    Rejected,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Approval {
    pub id: String,
    pub summary: String,
    #[serde(default)]
    pub detail: String,
    pub requested_by: String,
    pub verdict: Verdict,
    #[serde(default)]
    pub answered_note: Option<String>,
    /// What saying yes actually does, when the answer is not just a word. Only
    /// the core sets this — an agent asking for approval cannot describe an act
    /// the core will then carry out on its behalf.
    #[serde(default)]
    pub grants: Option<Grant>,
    /// What saying yes lets a project run from now on. Only the core sets this.
    #[serde(default)]
    pub allows: Option<AllowCommand>,
    /// When it was asked, and when it was answered.
    #[serde(default)]
    pub at: u64,
    #[serde(default)]
    pub answered_at: u64,
}

/// A command a project may run without asking, once somebody says so.
///
/// Different from a raise: a raise widens what an agent may do everywhere, and
/// this widens one project by one command. It is the smaller answer to "must I
/// sit here saying yes to `npm test`", and the smaller answer is the right one.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AllowCommand {
    pub repository_id: String,
    /// The rule as the engine would read it, e.g. `Bash(bash tests/run.sh:*)`.
    pub rule: String,
    /// Who was stopped by it, so they can be told the answer.
    pub agent_id: String,
}

/// A permission raise waiting on a human.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Grant {
    pub agent_id: String,
    pub from: String,
    pub to: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RequestApproval {
    pub summary: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default = "unknown")]
    pub requested_by: String,
}

fn unknown() -> String {
    "unknown".to_owned()
}

#[derive(Clone, Debug, Deserialize)]
pub struct AnswerApproval {
    pub approved: bool,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct State {
    #[serde(default)]
    approvals: BTreeMap<String, Approval>,
    #[serde(default)]
    next_number: u32,
}

pub struct Approvals {
    state: Mutex<State>,
    data_dir: PathBuf,
}

impl Approvals {
    pub fn new(data_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&data_dir);
        let data_dir = crate::exec::settled(&data_dir);
        let state = crate::db::load_state(&data_dir, "approvals");

        Self {
            state: Mutex::new(state),
            data_dir,
        }
    }

    fn persist(&self, state: &State) {
        crate::db::save_state(&self.data_dir, "approvals", state);
    }

    pub fn list(&self) -> Vec<Approval> {
        self.state.lock().approvals.values().cloned().collect()
    }

    pub fn get(&self, id: &str) -> Option<Approval> {
        self.state.lock().approvals.get(id).cloned()
    }

    pub fn request(&self, request: RequestApproval) -> Result<Approval> {
        if request.summary.trim().is_empty() {
            bail!("an approval needs a summary a human can read on a phone");
        }

        let mut state = self.state.lock();
        state.next_number += 1;
        let id = format!("a{}", state.next_number);

        let approval = Approval {
            id: id.clone(),
            summary: request.summary,
            detail: request.detail,
            requested_by: request.requested_by,
            verdict: Verdict::Pending,
            answered_note: None,
            grants: None,
            allows: None,
            at: now_secs(),
            answered_at: 0,
        };

        state.approvals.insert(id, approval.clone());
        self.persist(&state);
        Ok(approval)
    }

    /// An approval that, said yes to, lets one project run one command.
    pub fn request_allow(&self, summary: String, detail: String, allow: AllowCommand) -> Result<Approval> {
        let approval = self.request(RequestApproval {
            summary,
            detail,
            requested_by: allow.agent_id.clone(),
        })?;

        let mut state = self.state.lock();
        if let Some(stored) = state.approvals.get_mut(&approval.id) {
            stored.allows = Some(allow);
            let carried = stored.clone();
            self.persist(&state);
            return Ok(carried);
        }

        Ok(approval)
    }

    /// Whether somebody is already being asked this exact thing.
    ///
    /// A pane holds its question until it is answered, and the supervisor looks
    /// every ten seconds — without this it would ask a hundred times about one
    /// command.
    pub fn already_asking(&self, repository_id: &str, rule: &str) -> bool {
        self.state.lock().approvals.values().any(|held| {
            held.verdict == Verdict::Pending
                && held
                    .allows
                    .as_ref()
                    .is_some_and(|allow| allow.repository_id == repository_id && allow.rule == rule)
        })
    }

    /// An approval the core raised itself, carrying what saying yes will do.
    pub fn request_grant(&self, summary: String, detail: String, by: &str, grant: Grant) -> Result<Approval> {
        let approval = self.request(RequestApproval {
            summary,
            detail,
            requested_by: by.to_owned(),
        })?;

        let mut state = self.state.lock();
        if let Some(stored) = state.approvals.get_mut(&approval.id) {
            stored.grants = Some(grant);
            let carried = stored.clone();
            self.persist(&state);
            return Ok(carried);
        }

        Ok(approval)
    }

    pub fn answer(&self, id: &str, answer: AnswerApproval) -> Result<Approval> {
        let mut state = self.state.lock();
        let approval = state
            .approvals
            .get_mut(id)
            .ok_or_else(|| anyhow!("unknown approval: {id}"))?;

        if approval.verdict != Verdict::Pending {
            bail!("{id} was already answered");
        }

        approval.verdict = if answer.approved {
            Verdict::Approved
        } else {
            Verdict::Rejected
        };
        approval.answered_note = answer.note;
        approval.answered_at = now_secs();

        let updated = approval.clone();
        self.persist(&state);
        Ok(updated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(name: &str) -> Approvals {
        let dir = std::env::temp_dir().join(format!("agentland-approvals-{name}"));
        let _ = fs::remove_dir_all(&dir);
        Approvals::new(dir)
    }

    #[test]
    fn an_approval_starts_pending_and_is_answered_once() {
        let approvals = store("once");
        let request = approvals
            .request(RequestApproval {
                summary: "Send the outreach batch".to_owned(),
                detail: "ten drafts".to_owned(),
                requested_by: "ada".to_owned(),
            })
            .expect("request");

        assert_eq!(request.verdict, Verdict::Pending);

        let answered = approvals
            .answer(
                &request.id,
                AnswerApproval {
                    approved: true,
                    note: Some("looks right".to_owned()),
                },
            )
            .expect("answer");
        assert_eq!(answered.verdict, Verdict::Approved);

        let again = approvals.answer(
            &request.id,
            AnswerApproval {
                approved: false,
                note: None,
            },
        );
        assert!(again.is_err(), "an answered approval must not flip later");
    }

    #[test]
    fn a_rejection_is_recorded_as_such() {
        let approvals = store("reject");
        let request = approvals
            .request(RequestApproval {
                summary: "Force push to main".to_owned(),
                detail: String::new(),
                requested_by: "rex".to_owned(),
            })
            .expect("request");

        let answered = approvals
            .answer(
                &request.id,
                AnswerApproval {
                    approved: false,
                    note: Some("never".to_owned()),
                },
            )
            .expect("answer");

        assert_eq!(answered.verdict, Verdict::Rejected);
        assert_eq!(answered.answered_note.as_deref(), Some("never"));
    }

    #[test]
    fn an_approval_without_a_summary_is_refused() {
        let approvals = store("blank");
        assert!(approvals
            .request(RequestApproval {
                summary: "   ".to_owned(),
                detail: String::new(),
                requested_by: "ada".to_owned(),
            })
            .is_err());
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or_default()
}
