use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

use anyhow::{bail, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepState {
    Waiting,
    Assigned,
    Done,
    Blocked,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanState {
    Running,
    Done,
    Abandoned,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Step {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub brief: String,
    #[serde(default)]
    pub needs: Vec<String>,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    pub state: StepState,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Plan {
    pub id: String,
    pub goal: String,
    pub repository_id: String,
    pub created_by: String,
    pub state: PlanState,
    pub steps: Vec<Step>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DraftStep {
    pub title: String,
    #[serde(default)]
    pub brief: String,
    #[serde(default)]
    pub needs: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DraftPlan {
    pub goal: String,
    pub repository_id: String,
    #[serde(default = "unknown_author")]
    pub created_by: String,
    pub steps: Vec<DraftStep>,
}

fn unknown_author() -> String {
    "x".to_owned()
}

impl Plan {
    /// The steps whose every dependency is done and which nobody is holding.
    pub fn ready(&self) -> Vec<&Step> {
        if self.state != PlanState::Running {
            return Vec::new();
        }

        let done: BTreeSet<&str> = self
            .steps
            .iter()
            .filter(|step| step.state == StepState::Done)
            .map(|step| step.id.as_str())
            .collect();

        self.steps
            .iter()
            .filter(|step| step.state == StepState::Waiting)
            .filter(|step| step.needs.iter().all(|need| done.contains(need.as_str())))
            .collect()
    }

    pub fn blocked_on(&self, step_id: &str) -> Vec<&Step> {
        self.steps
            .iter()
            .filter(|step| step.needs.iter().any(|need| need == step_id))
            .collect()
    }

    pub fn progress(&self) -> (usize, usize) {
        let done = self
            .steps
            .iter()
            .filter(|step| step.state == StepState::Done)
            .count();
        (done, self.steps.len())
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct State {
    #[serde(default)]
    plans: BTreeMap<String, Plan>,
    #[serde(default)]
    next_number: u32,
}

pub struct Plans {
    state: Mutex<State>,
    data_dir: PathBuf,
}

impl Plans {
    pub fn new(data_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&data_dir);
        let data_dir = fs::canonicalize(&data_dir).unwrap_or(data_dir);
        let state = crate::db::load_state(&data_dir, "plans");

        Self {
            state: Mutex::new(state),
            data_dir,
        }
    }

    fn persist(&self, state: &State) {
        crate::db::save_state(&self.data_dir, "plans", state);
    }

    pub fn list(&self) -> Vec<Plan> {
        self.state.lock().plans.values().cloned().collect()
    }

    pub fn get(&self, id: &str) -> Option<Plan> {
        self.state.lock().plans.get(id).cloned()
    }

    pub fn create(&self, draft: DraftPlan) -> Result<Plan> {
        let goal = draft.goal.trim().to_owned();
        if goal.is_empty() {
            bail!("a plan needs a goal");
        }
        if draft.steps.is_empty() {
            bail!("a plan with no steps is a wish, not a plan");
        }

        let mut state = self.state.lock();
        state.next_number += 1;
        let plan_id = format!("p{}", state.next_number);

        let steps = build_steps(&plan_id, &draft.steps)?;

        let plan = Plan {
            id: plan_id.clone(),
            goal,
            repository_id: draft.repository_id,
            created_by: draft.created_by,
            state: PlanState::Running,
            steps,
        };

        state.plans.insert(plan_id, plan.clone());
        self.persist(&state);
        Ok(plan)
    }

    pub fn attach_task(&self, plan_id: &str, step_id: &str, task_id: &str) -> Result<Plan> {
        self.change(plan_id, step_id, |step| {
            step.task_id = Some(task_id.to_owned());
            step.state = StepState::Assigned;
        })
    }

    pub fn mark(&self, plan_id: &str, step_id: &str, state: StepState, note: Option<String>) -> Result<Plan> {
        let plan = self.change(plan_id, step_id, |step| {
            step.state = state;
            if let Some(text) = note.clone() {
                step.note = Some(text);
            }
        })?;

        let mut held = self.state.lock();
        if let Some(stored) = held.plans.get_mut(plan_id) {
            let (done, total) = stored.progress();
            if done == total && stored.state == PlanState::Running {
                stored.state = PlanState::Done;
            }
            let updated = stored.clone();
            self.persist(&held);
            return Ok(updated);
        }

        Ok(plan)
    }

    pub fn abandon(&self, plan_id: &str, why: &str) -> Result<Plan> {
        let mut state = self.state.lock();
        let Some(plan) = state.plans.get_mut(plan_id) else {
            bail!("there is no plan called {plan_id}");
        };

        plan.state = PlanState::Abandoned;
        for step in plan.steps.iter_mut() {
            if step.state == StepState::Waiting {
                step.state = StepState::Blocked;
                step.note = Some(why.to_owned());
            }
        }

        let updated = plan.clone();
        self.persist(&state);
        Ok(updated)
    }

    /// Every step that is ready to start, across every running plan.
    pub fn ready_everywhere(&self) -> Vec<(String, Step)> {
        self.state
            .lock()
            .plans
            .values()
            .flat_map(|plan| {
                plan.ready()
                    .into_iter()
                    .map(|step| (plan.id.clone(), step.clone()))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    pub fn plan_of_task(&self, task_id: &str) -> Option<(Plan, Step)> {
        let state = self.state.lock();
        for plan in state.plans.values() {
            if let Some(step) = plan
                .steps
                .iter()
                .find(|step| step.task_id.as_deref() == Some(task_id))
            {
                return Some((plan.clone(), step.clone()));
            }
        }
        None
    }

    fn change(&self, plan_id: &str, step_id: &str, edit: impl FnOnce(&mut Step)) -> Result<Plan> {
        let mut state = self.state.lock();
        let Some(plan) = state.plans.get_mut(plan_id) else {
            bail!("there is no plan called {plan_id}");
        };

        let Some(step) = plan.steps.iter_mut().find(|step| step.id == step_id) else {
            bail!("{plan_id} has no step called {step_id}");
        };

        edit(step);
        let updated = plan.clone();
        self.persist(&state);
        Ok(updated)
    }
}

fn build_steps(plan_id: &str, drafts: &[DraftStep]) -> Result<Vec<Step>> {
    let ids: Vec<String> = (1..=drafts.len())
        .map(|number| format!("{plan_id}s{number}"))
        .collect();

    let by_title: BTreeMap<&str, &str> = drafts
        .iter()
        .zip(&ids)
        .map(|(draft, id)| (draft.title.trim(), id.as_str()))
        .collect();

    let mut steps = Vec::new();
    for (index, draft) in drafts.iter().enumerate() {
        let title = draft.title.trim().to_owned();
        if title.is_empty() {
            bail!("step {} has no title", index + 1);
        }

        let mut needs = Vec::new();
        for name in &draft.needs {
            let wanted = name.trim();
            let resolved = if let Some(position) = ids.iter().position(|id| id == wanted) {
                ids[position].clone()
            } else if let Some(id) = by_title.get(wanted) {
                (*id).to_owned()
            } else {
                bail!("step {} needs \"{wanted}\", which is not in this plan", index + 1);
            };

            if resolved == ids[index] {
                bail!("step {} cannot wait for itself", index + 1);
            }

            needs.push(resolved);
        }

        steps.push(Step {
            id: ids[index].clone(),
            title,
            brief: draft.brief.trim().to_owned(),
            needs,
            task_id: None,
            note: None,
            state: StepState::Waiting,
        });
    }

    refuse_cycles(&steps)?;
    Ok(steps)
}

fn refuse_cycles(steps: &[Step]) -> Result<()> {
    let mut pending: BTreeMap<&str, BTreeSet<&str>> = steps
        .iter()
        .map(|step| {
            (
                step.id.as_str(),
                step.needs.iter().map(String::as_str).collect(),
            )
        })
        .collect();

    while !pending.is_empty() {
        let settled: Vec<&str> = pending
            .iter()
            .filter(|(_, needs)| needs.is_empty())
            .map(|(id, _)| *id)
            .collect();

        if settled.is_empty() {
            let stuck: Vec<&str> = pending.keys().copied().collect();
            bail!("these steps wait for each other and none can start: {}", stuck.join(", "));
        }

        for id in settled {
            pending.remove(id);
            for needs in pending.values_mut() {
                needs.remove(id);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plans(name: &str) -> Plans {
        let dir = std::env::temp_dir().join(format!("agentland-plans-{name}"));
        let _ = fs::remove_dir_all(&dir);
        Plans::new(dir)
    }

    fn draft(steps: Vec<DraftStep>) -> DraftPlan {
        DraftPlan {
            goal: "let a phone token read the skills library".to_owned(),
            repository_id: "agentland".to_owned(),
            created_by: "x".to_owned(),
            steps,
        }
    }

    fn step(title: &str, needs: &[&str]) -> DraftStep {
        DraftStep {
            title: title.to_owned(),
            brief: format!("do the work for {title}"),
            needs: needs.iter().map(|name| (*name).to_owned()).collect(),
        }
    }

    #[test]
    fn a_step_waits_until_everything_it_needs_is_done() {
        let plans = plans("gating");
        let plan = plans
            .create(draft(vec![
                step("widen the scope matrix", &[]),
                step("show the skills on the phone", &["widen the scope matrix"]),
                step("write the release note", &["show the skills on the phone"]),
            ]))
            .expect("create");

        let ready: Vec<&str> = plan.ready().iter().map(|step| step.title.as_str()).collect();
        assert_eq!(ready, vec!["widen the scope matrix"], "only the first can start");

        let after = plans
            .mark(&plan.id, &plan.steps[0].id, StepState::Done, None)
            .expect("mark");
        let ready: Vec<&str> = after.ready().iter().map(|step| step.title.as_str()).collect();
        assert_eq!(ready, vec!["show the skills on the phone"]);
    }

    #[test]
    fn steps_that_wait_for_each_other_are_refused_at_creation() {
        let plans = plans("cycle");
        let error = plans
            .create(DraftPlan {
                goal: "go in circles".to_owned(),
                repository_id: "agentland".to_owned(),
                created_by: "x".to_owned(),
                steps: vec![step("a", &["b"]), step("b", &["a"])],
            })
            .expect_err("a cycle is not a plan");

        assert!(error.to_string().contains("wait for each other"), "{error}");
    }

    #[test]
    fn a_dependency_can_be_named_by_title_or_by_id() {
        let plans = plans("naming");
        let plan = plans
            .create(draft(vec![step("first", &[]), step("second", &["first"])]))
            .expect("create");

        assert_eq!(plan.steps[1].needs, vec![plan.steps[0].id.clone()]);

        let by_id = plans
            .create(draft(vec![step("one", &[]), step("two", &["p2s1"])]))
            .expect("create by id");
        assert_eq!(by_id.steps[1].needs, vec![by_id.steps[0].id.clone()]);
    }

    #[test]
    fn a_dependency_that_is_not_in_the_plan_is_refused_by_name() {
        let plans = plans("missing");
        let error = plans
            .create(draft(vec![step("only", &["a step nobody wrote"])]))
            .expect_err("should refuse");

        assert!(error.to_string().contains("not in this plan"), "{error}");
    }

    #[test]
    fn an_empty_plan_is_refused() {
        let plans = plans("empty");
        assert!(plans.create(draft(vec![])).is_err());

        let no_goal = plans.create(DraftPlan {
            goal: "   ".to_owned(),
            repository_id: "agentland".to_owned(),
            created_by: "x".to_owned(),
            steps: vec![step("something", &[])],
        });
        assert!(no_goal.unwrap_err().to_string().contains("needs a goal"));
    }

    #[test]
    fn a_plan_closes_itself_when_the_last_step_is_done() {
        let plans = plans("closing");
        let plan = plans
            .create(draft(vec![step("one", &[]), step("two", &["one"])]))
            .expect("create");

        let half = plans
            .mark(&plan.id, &plan.steps[0].id, StepState::Done, None)
            .expect("mark");
        assert_eq!(half.state, PlanState::Running);

        let whole = plans
            .mark(&plan.id, &plan.steps[1].id, StepState::Done, None)
            .expect("mark");
        assert_eq!(whole.state, PlanState::Done);
        assert!(whole.ready().is_empty(), "a finished plan hands out nothing");
    }

    #[test]
    fn a_card_can_be_traced_back_to_the_step_that_asked_for_it() {
        let plans = plans("tracing");
        let plan = plans.create(draft(vec![step("one", &[])])).expect("create");

        plans
            .attach_task(&plan.id, &plan.steps[0].id, "t42")
            .expect("attach");

        let (found, step) = plans.plan_of_task("t42").expect("traced");
        assert_eq!(found.id, plan.id);
        assert_eq!(step.state, StepState::Assigned);
        assert!(plans.plan_of_task("t43").is_none());
    }

    #[test]
    fn abandoning_a_plan_says_why_on_every_step_that_never_ran() {
        let plans = plans("abandon");
        let plan = plans
            .create(draft(vec![step("one", &[]), step("two", &["one"])]))
            .expect("create");
        plans
            .mark(&plan.id, &plan.steps[0].id, StepState::Done, None)
            .expect("mark");

        let dropped = plans.abandon(&plan.id, "the approach was wrong").expect("abandon");
        assert_eq!(dropped.state, PlanState::Abandoned);
        assert_eq!(dropped.steps[0].state, StepState::Done, "what was done stays done");
        assert_eq!(dropped.steps[1].state, StepState::Blocked);
        assert_eq!(dropped.steps[1].note.as_deref(), Some("the approach was wrong"));
        assert!(dropped.ready().is_empty());
    }

    #[test]
    fn what_is_ready_survives_a_restart() {
        let dir = std::env::temp_dir().join("agentland-plans-restart");
        let _ = fs::remove_dir_all(&dir);

        let plan = {
            let plans = Plans::new(dir.clone());
            let plan = plans
                .create(draft(vec![step("one", &[]), step("two", &["one"])]))
                .expect("create");
            plans
                .mark(&plan.id, &plan.steps[0].id, StepState::Done, None)
                .expect("mark")
        };

        let reopened = Plans::new(dir);
        let ready = reopened.ready_everywhere();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].0, plan.id);
        assert_eq!(ready[0].1.title, "two");
    }
}
