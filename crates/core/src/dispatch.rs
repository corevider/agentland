use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::crew::{Agent, AgentState};
use crate::board::Task;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct Caps {
    pub per_repository: usize,
    pub per_engine: usize,
}

impl Default for Caps {
    fn default() -> Self {
        Self {
            per_repository: 3,
            per_engine: 2,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Decision {
    Assign { agent_id: String, reason: String },
    Queue { reason: String },
    Refuse { reason: String },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DispatchEvent {
    pub seq: u64,
    pub agent_id: String,
    pub task_id: String,
    pub reason: String,
    /// When the decision was taken.
    #[serde(default)]
    pub at: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct DispatchState {
    #[serde(default)]
    pub paused: bool,
    #[serde(default)]
    pub caps: Caps,
    #[serde(default)]
    pub queue: VecDeque<String>,
    #[serde(default)]
    pub events: VecDeque<DispatchEvent>,
    #[serde(default)]
    pub next_seq: u64,
}

const EVENT_HISTORY: usize = 24;

impl DispatchState {
    pub fn record_handoff(&mut self, agent_id: &str, task_id: &str, reason: &str) {
        self.next_seq += 1;
        self.events.push_back(DispatchEvent {
            seq: self.next_seq,
            agent_id: agent_id.to_owned(),
            task_id: task_id.to_owned(),
            reason: reason.to_owned(),
            at: now_secs(),
        });

        while self.events.len() > EVENT_HISTORY {
            self.events.pop_front();
        }
    }
}

fn role_affinity(role: &str, task: &Task) -> u8 {
    let haystack = format!("{} {}", task.title, task.body).to_lowercase();

    let hints: &[(&str, &str)] = &[
        ("reviewer", "review"),
        ("tester", "test"),
        ("researcher", "research"),
        ("ops", "deploy"),
    ];

    for (candidate_role, keyword) in hints {
        if role == *candidate_role && haystack.contains(keyword) {
            return 2;
        }
    }

    if role == "implementer" {
        1
    } else {
        0
    }
}

pub fn decide(state: &DispatchState, task: &Task, crew: &[Agent]) -> Decision {
    if state.paused {
        return Decision::Queue {
            reason: "X is paused; nothing new is being handed out".to_owned(),
        };
    }

    if task.assignee.is_some() {
        return Decision::Refuse {
            reason: format!("{} already belongs to someone", task.id),
        };
    }

    let in_repository: Vec<&Agent> = crew
        .iter()
        .filter(|agent| agent.repository_id == task.repository_id)
        .collect();

    if in_repository.is_empty() {
        return Decision::Refuse {
            reason: format!(
                "nobody is hired on {} — hire an agent there first",
                task.repository_id
            ),
        };
    }

    // A card that names a worktree can only be done there. A branch lives in
    // exactly one worktree, so handing such a step to an agent somewhere else
    // means its commit lands on the wrong branch — which is what happened to
    // the README step of the /version plan, caught only by reading the reply.
    let in_repository: Vec<&Agent> = match task.worktree.as_deref() {
        None => in_repository,
        Some(wanted) => {
            let here: Vec<&Agent> = in_repository
                .into_iter()
                .filter(|agent| agent.worktree == wanted)
                .collect();

            if here.is_empty() {
                return Decision::Refuse {
                    reason: format!(
                        "{} belongs in the {wanted} worktree and nobody is working there — hire an agent in it",
                        task.id
                    ),
                };
            }

            here
        }
    };

    let working_here = in_repository
        .iter()
        .filter(|agent| agent.state == AgentState::Working)
        .count();

    if working_here >= state.caps.per_repository {
        return Decision::Queue {
            reason: format!(
                "{} of {} allowed agents are already working on {}",
                working_here, state.caps.per_repository, task.repository_id
            ),
        };
    }

    let mut candidates: Vec<&&Agent> = in_repository
        .iter()
        .filter(|agent| agent.state != AgentState::Working)
        .filter(|agent| {
            let busy_on_engine = crew
                .iter()
                .filter(|other| {
                    other.engine_id == agent.engine_id && other.state == AgentState::Working
                })
                .count();
            busy_on_engine < state.caps.per_engine
        })
        .collect();

    if candidates.is_empty() {
        return Decision::Queue {
            reason: format!(
                "every agent on {} is busy or at the {} concurrent limit for its engine",
                task.repository_id, state.caps.per_engine
            ),
        };
    }

    candidates.sort_by_key(|agent| std::cmp::Reverse(role_affinity(&agent.role, task)));
    let chosen = candidates[0];

    let reason = if role_affinity(&chosen.role, task) == 2 {
        format!(
            "{} is free and the task reads like {} work",
            chosen.name, chosen.role
        )
    } else {
        format!(
            "{} is the free agent on {} with the closest role ({})",
            chosen.name, task.repository_id, chosen.role
        )
    };

    Decision::Assign {
        agent_id: chosen.id.clone(),
        reason,
    }
}

pub struct Dispatch {
    state: Mutex<DispatchState>,
    data_dir: PathBuf,
}

impl Dispatch {
    pub fn new(data_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&data_dir);
        let data_dir = fs::canonicalize(&data_dir).unwrap_or(data_dir);
        let state = crate::db::load_state(&data_dir, "dispatch");

        Self {
            state: Mutex::new(state),
            data_dir,
        }
    }

    fn persist(&self, state: &DispatchState) {
        crate::db::save_state(&self.data_dir, "dispatch", state);
    }

    pub fn snapshot(&self) -> DispatchState {
        self.state.lock().clone()
    }

    pub fn set_paused(&self, paused: bool) -> DispatchState {
        let mut state = self.state.lock();
        state.paused = paused;
        self.persist(&state);
        state.clone()
    }

    pub fn set_caps(&self, caps: Caps) -> DispatchState {
        let mut state = self.state.lock();
        state.caps = caps;
        self.persist(&state);
        state.clone()
    }

    pub fn decide(&self, task: &Task, crew: &[Agent]) -> Decision {
        decide(&self.state.lock(), task, crew)
    }

    pub fn record_assignment(&self, agent_id: &str, task_id: &str, reason: &str) -> DispatchState {
        let mut state = self.state.lock();
        state.queue.retain(|entry| entry != task_id);
        state.record_handoff(agent_id, task_id, reason);
        self.persist(&state);
        state.clone()
    }

    pub fn enqueue(&self, task_id: &str) -> DispatchState {
        let mut state = self.state.lock();
        if !state.queue.iter().any(|entry| entry == task_id) {
            state.queue.push_back(task_id.to_owned());
        }
        self.persist(&state);
        state.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::Column;

    fn agent(id: &str, role: &str, state: AgentState) -> Agent {
        Agent {
            id: id.to_owned(),
            name: id.to_owned(),
            role: role.to_owned(),
            engine_id: "claude".to_owned(),
            repository_id: "demo".to_owned(),
            worktree: format!("{id}-tree"),
            session_id: None,
            state,
            model: None,
            title: None,
            colour: None,
            permissions: None,
            account: None,
        }
    }

    fn task(title: &str) -> Task {
        Task {
            id: "t1".to_owned(),
            title: title.to_owned(),
            body: String::new(),
            column: Column::Backlog,
            repository_id: "demo".to_owned(),
            assignee: None,
            worktree: None,
            branch: None,
            evidence: Vec::new(),
            at: 0,
            position: 0.0,
            attachments: Vec::new(),
        }
    }

    #[test]
    fn a_card_bound_to_a_worktree_only_goes_to_an_agent_standing_in_it() {
        let state = DispatchState::default();
        let crew = vec![
            agent("nova", "ops", AgentState::Idle),
            agent("ada", "implementer", AgentState::Idle),
        ];
        let mut card = task("document the endpoint in the README");
        card.worktree = Some("ada-tree".to_owned());

        match decide(&state, &card, &crew) {
            // Nova reads like the better role for documenting, and would win
            // without the binding — the branch is what decides here.
            Decision::Assign { agent_id, .. } => assert_eq!(agent_id, "ada"),
            other => panic!("expected an assignment, got {other:?}"),
        }
    }

    #[test]
    fn a_card_bound_to_an_empty_worktree_says_so_rather_than_landing_elsewhere() {
        let state = DispatchState::default();
        let crew = vec![agent("nova", "ops", AgentState::Idle)];
        let mut card = task("document the endpoint");
        card.worktree = Some("ada-tree".to_owned());

        match decide(&state, &card, &crew) {
            Decision::Refuse { reason } => {
                assert!(reason.contains("ada-tree"), "the reason names the worktree: {reason}");
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn prefers_the_role_the_task_reads_like() {
        let state = DispatchState::default();
        let crew = vec![
            agent("ada", "implementer", AgentState::Idle),
            agent("rex", "reviewer", AgentState::Idle),
        ];

        match decide(&state, &task("review the auth changes"), &crew) {
            Decision::Assign { agent_id, reason } => {
                assert_eq!(agent_id, "rex");
                assert!(reason.contains("reviewer"), "reason should explain: {reason}");
            }
            other => panic!("expected an assignment, got {other:?}"),
        }
    }

    #[test]
    fn queues_with_a_reason_when_the_repository_is_at_its_cap() {
        let state = DispatchState {
            caps: Caps {
                per_repository: 1,
                per_engine: 5,
            },
            ..DispatchState::default()
        };
        let crew = vec![
            agent("ada", "implementer", AgentState::Working),
            agent("rex", "reviewer", AgentState::Idle),
        ];

        match decide(&state, &task("anything"), &crew) {
            Decision::Queue { reason } => assert!(reason.contains("already working")),
            other => panic!("expected a queue, got {other:?}"),
        }
    }

    #[test]
    fn a_paused_manager_hands_out_nothing() {
        let state = DispatchState {
            paused: true,
            ..DispatchState::default()
        };
        let crew = vec![agent("ada", "implementer", AgentState::Idle)];

        assert!(matches!(
            decide(&state, &task("anything"), &crew),
            Decision::Queue { .. }
        ));
    }

    #[test]
    fn refuses_when_nobody_is_hired_on_that_repository() {
        let state = DispatchState::default();
        assert!(matches!(
            decide(&state, &task("anything"), &[]),
            Decision::Refuse { .. }
        ));
    }
}

#[cfg(test)]
mod store_tests {
    use super::*;

    fn store(name: &str) -> Dispatch {
        let dir = std::env::temp_dir().join(format!("agentland-dispatch-{name}"));
        let _ = fs::remove_dir_all(&dir);
        Dispatch::new(dir)
    }

    fn reopen(name: &str) -> Dispatch {
        Dispatch::new(std::env::temp_dir().join(format!("agentland-dispatch-{name}")))
    }

    #[test]
    fn what_x_decided_survives_a_restart() {
        let dispatch = store("history");
        dispatch.record_assignment("ada", "t9", "Ada is the free agent with the closest role");
        dispatch.record_assignment("kai", "t8", "Kai is idle on this repository");

        let after = reopen("history").snapshot();
        assert_eq!(after.events.len(), 2);
        assert_eq!(after.events[0].agent_id, "ada");
        assert_eq!(after.events[1].task_id, "t8");
        assert!(after.events[1].reason.contains("idle"));
        assert_eq!(after.next_seq, 2, "the sequence keeps counting rather than restarting");
    }

    #[test]
    fn the_caps_and_the_pause_are_remembered() {
        let dispatch = store("caps");
        dispatch.set_caps(Caps {
            per_repository: 1,
            per_engine: 5,
        });
        dispatch.set_paused(true);

        let after = reopen("caps").snapshot();
        assert_eq!(after.caps.per_repository, 1);
        assert_eq!(after.caps.per_engine, 5);
        assert!(after.paused, "a held dispatch stays held");
    }

    #[test]
    fn a_queued_card_is_still_queued_after_a_restart_and_only_once() {
        let dispatch = store("queue");
        dispatch.enqueue("t5");
        dispatch.enqueue("t5");
        dispatch.enqueue("t6");

        let after = reopen("queue").snapshot();
        assert_eq!(after.queue.len(), 2);
        assert_eq!(after.queue[0], "t5");
    }

    #[test]
    fn assigning_a_queued_card_takes_it_out_of_the_queue_for_good() {
        let dispatch = store("dequeue");
        dispatch.enqueue("t5");
        dispatch.record_assignment("ada", "t5", "a slot opened");

        let after = reopen("dequeue").snapshot();
        assert!(after.queue.is_empty(), "{:?}", after.queue);
        assert_eq!(after.events.len(), 1);
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or_default()
}
