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

/// How well a role fits a card, and whether the card is one to be taken apart.
///
/// A card that came from a plan step is already one piece of work: somebody
/// decided what it is, and it wants the hands that do it. A card that came from
/// nobody's plan is an outcome — what a person wrote on the board — and taking
/// an outcome apart is the commander's whole job.
///
/// The commander is ranked for the second and not the first. Before this it was
/// ranked for neither, which meant it scored zero and still won whenever it was
/// the only agent on a repository: a commander was handed a step to implement
/// because nobody else existed, and had to notice and hand it back.
///
/// A card that names its own role beats both. "Review the auth changes" wants a
/// reviewer whether or not a plan made it.
fn role_affinity(role: &str, task: &Task, came_from_a_step: bool) -> u8 {
    let haystack = format!("{} {}", task.title, task.body).to_lowercase();

    let hints: &[(&str, &str)] = &[
        ("reviewer", "review"),
        ("tester", "test"),
        ("researcher", "research"),
        ("ops", "deploy"),
    ];

    for (candidate_role, keyword) in hints {
        if role == *candidate_role && haystack.contains(keyword) {
            return 3;
        }
    }

    if role == "commander" {
        return if came_from_a_step { 0 } else { 2 };
    }

    if role == "implementer" {
        1
    } else {
        0
    }
}

pub fn decide(
    state: &DispatchState,
    task: &Task,
    crew: &[Agent],
    came_from_a_step: bool,
) -> Decision {
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

    // Ranking alone left the commander holding steps: when it was the only
    // agent on a repository it scored lowest and still won, because lowest of
    // one is first. A step wants hands, and the answer when there are none is
    // to say so — the commander reads this and hires, which is the thing it
    // was going to have to do anyway.
    if came_from_a_step {
        let hands: Vec<&&Agent> = candidates
            .iter()
            .copied()
            .filter(|agent| agent.role != "commander")
            .collect();

        if hands.is_empty() {
            return Decision::Queue {
                reason: format!(
                    "{} is a step to be done and only the commander is free on {} — hire someone to do it",
                    task.id, task.repository_id
                ),
            };
        }

        candidates = hands;
    }

    candidates
        .sort_by_key(|agent| std::cmp::Reverse(role_affinity(&agent.role, task, came_from_a_step)));
    let chosen = candidates[0];

    let fit = role_affinity(&chosen.role, task, came_from_a_step);
    let reason = if fit == 3 {
        format!(
            "{} is free and the task reads like {} work",
            chosen.name, chosen.role
        )
    } else if fit == 2 {
        format!(
            "{} is the commander and {} is an outcome to take apart, not a step",
            chosen.name, task.id
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
        let data_dir = crate::exec::settled(&data_dir);
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

    pub fn decide(&self, task: &Task, crew: &[Agent], came_from_a_step: bool) -> Decision {
        decide(&self.state.lock(), task, crew, came_from_a_step)
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
            workspace_id: None,
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

        match decide(&state, &card, &crew, false) {
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

        match decide(&state, &card, &crew, false) {
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

        match decide(&state, &task("review the auth changes"), &crew, false) {
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

        match decide(&state, &task("anything"), &crew, false) {
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
            decide(&state, &task("anything"), &crew, false),
            Decision::Queue { .. }
        ));
    }

    #[test]
    fn refuses_when_nobody_is_hired_on_that_repository() {
        let state = DispatchState::default();
        assert!(matches!(
            decide(&state, &task("anything"), &[], false),
            Decision::Refuse { .. }
        ));
    }

    /// The bug this was written for: a commander alone on a repository was
    /// handed a step to implement, because lowest of one is still first. It
    /// noticed and handed the card back, which is a thing it should not have
    /// had to do.
    #[test]
    fn a_commander_alone_is_not_handed_a_step_to_implement() {
        let state = DispatchState::default();
        let crew = vec![agent("x2", "commander", AgentState::Idle)];

        match decide(&state, &task("add a test beside greet"), &crew, true) {
            Decision::Queue { reason } => {
                assert!(reason.contains("hire"), "the reason says what to do: {reason}");
            }
            other => panic!("expected a queue, got {other:?}"),
        }
    }

    #[test]
    fn a_step_goes_to_the_hands_rather_than_the_commander() {
        let state = DispatchState::default();
        let crew = vec![
            agent("x2", "commander", AgentState::Idle),
            agent("ada", "implementer", AgentState::Idle),
        ];

        match decide(&state, &task("add a test beside greet"), &crew, true) {
            Decision::Assign { agent_id, .. } => assert_eq!(agent_id, "ada"),
            other => panic!("expected an assignment, got {other:?}"),
        }
    }

    /// The other half of the same rule, and the reason the commander is ranked
    /// at all: a card nobody planned is an outcome, and taking one apart is its
    /// job rather than the implementer's.
    #[test]
    fn an_outcome_nobody_planned_goes_to_the_commander() {
        let state = DispatchState::default();
        let crew = vec![
            agent("ada", "implementer", AgentState::Idle),
            agent("x2", "commander", AgentState::Idle),
        ];

        match decide(&state, &task("the checkout page loses the basket"), &crew, false) {
            Decision::Assign { agent_id, reason } => {
                assert_eq!(agent_id, "x2");
                assert!(reason.contains("take apart"), "the reason explains: {reason}");
            }
            other => panic!("expected an assignment, got {other:?}"),
        }
    }

    /// A card that names its own role beats both, planned or not.
    #[test]
    fn a_card_that_reads_like_one_role_still_goes_to_that_role() {
        let state = DispatchState::default();
        let crew = vec![
            agent("x2", "commander", AgentState::Idle),
            agent("rex", "reviewer", AgentState::Idle),
        ];

        match decide(&state, &task("review the auth changes"), &crew, false) {
            Decision::Assign { agent_id, .. } => assert_eq!(agent_id, "rex"),
            other => panic!("expected an assignment, got {other:?}"),
        }
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
