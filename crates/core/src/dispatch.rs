use std::collections::VecDeque;

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
