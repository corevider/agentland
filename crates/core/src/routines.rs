use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, bail, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

const FAILURES_BEFORE_PAUSE: u32 = 2;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Routine {
    pub id: String,
    pub name: String,
    pub agent_id: String,
    pub brief: String,
    pub every_minutes: u32,
    #[serde(default)]
    pub draft_only: bool,
    #[serde(default = "enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub last_run: u64,
    #[serde(default)]
    pub consecutive_failures: u32,
    #[serde(default)]
    pub last_result: Option<String>,
}

fn enabled() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize)]
pub struct CreateRoutine {
    pub name: String,
    pub agent_id: String,
    pub brief: String,
    #[serde(default = "default_interval")]
    pub every_minutes: u32,
    #[serde(default)]
    pub draft_only: bool,
}

fn default_interval() -> u32 {
    60
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct State {
    #[serde(default)]
    routines: BTreeMap<String, Routine>,
    #[serde(default)]
    next_number: u32,
}

pub struct Routines {
    state: Mutex<State>,
    data_dir: PathBuf,
}

pub fn is_due(routine: &Routine, now: u64) -> bool {
    if !routine.enabled {
        return false;
    }

    let interval = (routine.every_minutes.max(1) as u64) * 60;
    now.saturating_sub(routine.last_run) >= interval
}

impl Routines {
    pub fn new(data_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&data_dir);
        let data_dir = crate::exec::settled(&data_dir);
        let state = crate::db::load_state(&data_dir, "routines");

        Self {
            state: Mutex::new(state),
            data_dir,
        }
    }

    fn persist(&self, state: &State) {
        crate::db::save_state(&self.data_dir, "routines", state);
    }

    pub fn list(&self) -> Vec<Routine> {
        self.state.lock().routines.values().cloned().collect()
    }

    pub fn create(&self, request: CreateRoutine) -> Result<Routine> {
        if request.name.trim().is_empty() {
            bail!("a routine needs a name");
        }
        if request.brief.trim().is_empty() {
            bail!("a routine needs a brief — the agent has to be told what to do");
        }

        let mut state = self.state.lock();
        state.next_number += 1;
        let id = format!("r{}", state.next_number);

        let routine = Routine {
            id: id.clone(),
            name: request.name,
            agent_id: request.agent_id,
            brief: request.brief,
            every_minutes: request.every_minutes.max(1),
            draft_only: request.draft_only,
            enabled: true,
            last_run: 0,
            consecutive_failures: 0,
            last_result: None,
        };

        state.routines.insert(id, routine.clone());
        self.persist(&state);
        Ok(routine)
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<Routine> {
        let mut state = self.state.lock();
        let routine = state
            .routines
            .get_mut(id)
            .ok_or_else(|| anyhow!("unknown routine: {id}"))?;
        routine.enabled = enabled;
        if enabled {
            routine.consecutive_failures = 0;
        }
        let updated = routine.clone();
        self.persist(&state);
        Ok(updated)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let mut state = self.state.lock();
        state
            .routines
            .remove(id)
            .ok_or_else(|| anyhow!("unknown routine: {id}"))?;
        self.persist(&state);
        Ok(())
    }

    pub fn due(&self, now: u64) -> Vec<Routine> {
        self.state
            .lock()
            .routines
            .values()
            .filter(|routine| is_due(routine, now))
            .cloned()
            .collect()
    }

    pub fn record(&self, id: &str, now: u64, outcome: Result<String, String>) -> Option<Routine> {
        let mut state = self.state.lock();
        let routine = state.routines.get_mut(id)?;
        routine.last_run = now;

        match outcome {
            Ok(detail) => {
                routine.consecutive_failures = 0;
                routine.last_result = Some(detail);
            }
            Err(detail) => {
                routine.consecutive_failures += 1;
                routine.last_result = Some(detail);

                if routine.consecutive_failures >= FAILURES_BEFORE_PAUSE {
                    routine.enabled = false;
                    routine.last_result = Some(format!(
                        "{} — paused after {} failures in a row",
                        routine.last_result.clone().unwrap_or_default(),
                        routine.consecutive_failures
                    ));
                }
            }
        }

        let updated = routine.clone();
        self.persist(&state);
        Some(updated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn routine() -> Routine {
        Routine {
            id: "r1".to_owned(),
            name: "morning triage".to_owned(),
            agent_id: "ada".to_owned(),
            brief: "read overnight failures".to_owned(),
            every_minutes: 60,
            draft_only: true,
            enabled: true,
            last_run: 0,
            consecutive_failures: 0,
            last_result: None,
        }
    }

    #[test]
    fn a_routine_is_due_only_after_its_interval() {
        let mut entry = routine();
        entry.last_run = 1_000;

        assert!(!is_due(&entry, 1_000 + 3_599));
        assert!(is_due(&entry, 1_000 + 3_600));
    }

    #[test]
    fn a_disabled_routine_never_runs() {
        let mut entry = routine();
        entry.enabled = false;
        assert!(!is_due(&entry, u64::MAX));
    }

    #[test]
    fn two_failures_in_a_row_pause_the_routine() {
        let dir = std::env::temp_dir().join("agentland-routines-test");
        let _ = fs::remove_dir_all(&dir);
        let routines = Routines::new(dir);

        let created = routines
            .create(CreateRoutine {
                name: "morning triage".to_owned(),
                agent_id: "ada".to_owned(),
                brief: "read overnight failures".to_owned(),
                every_minutes: 60,
                draft_only: true,
            })
            .expect("create");

        let after_one = routines
            .record(&created.id, 100, Err("agent missing".to_owned()))
            .expect("record");
        assert!(after_one.enabled, "one failure should not pause it");

        let after_two = routines
            .record(&created.id, 200, Err("agent missing".to_owned()))
            .expect("record");
        assert!(!after_two.enabled, "two failures should pause it");
        assert!(after_two
            .last_result
            .unwrap()
            .contains("paused after 2 failures"));
    }

    #[test]
    fn a_success_clears_the_failure_streak() {
        let dir = std::env::temp_dir().join("agentland-routines-success");
        let _ = fs::remove_dir_all(&dir);
        let routines = Routines::new(dir);

        let created = routines
            .create(CreateRoutine {
                name: "recap".to_owned(),
                agent_id: "ada".to_owned(),
                brief: "write the weekly recap".to_owned(),
                every_minutes: 1,
                draft_only: false,
            })
            .expect("create");

        routines.record(&created.id, 100, Err("boom".to_owned()));
        let recovered = routines
            .record(&created.id, 200, Ok("card t7 created".to_owned()))
            .expect("record");

        assert_eq!(recovered.consecutive_failures, 0);
        assert!(recovered.enabled);
    }
}
