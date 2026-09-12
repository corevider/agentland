//! One card, several agents at once, and a person keeping one result.
//!
//! The same brief handed to different engines and models comes back as
//! different work. Each entrant stands in a worktree of its own, so their
//! branches never meet, and nobody holds the card until a person has read the
//! results side by side and kept one. The rest are let go with their folders;
//! their branches stay behind, so nothing written is lost.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, bail, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

pub const FEWEST: usize = 2;
pub const MOST: usize = 4;

/// What one entrant runs on.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Lane {
    pub engine_id: String,
    /// Left to the engine when empty.
    #[serde(default)]
    pub model: Option<String>,
}

impl Lane {
    pub fn model(&self) -> Option<String> {
        self.model
            .as_deref()
            .map(str::trim)
            .filter(|model| !model.is_empty())
            .map(str::to_owned)
    }

    /// "claude · opus", or the engine alone when the model is the engine's own.
    pub fn words(&self) -> String {
        match self.model() {
            Some(model) => format!("{} · {model}", self.engine_id.trim()),
            None => self.engine_id.trim().to_owned(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Entrant {
    pub agent_id: String,
    pub name: String,
    pub engine_id: String,
    #[serde(default)]
    pub model: Option<String>,
    pub worktree: String,
    pub branch: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Race {
    pub id: String,
    pub task_id: String,
    pub repository_id: String,
    pub entrants: Vec<Entrant>,
    pub started_at: u64,
    /// The entrant whose work was kept. None for a race still running, and for
    /// one called off.
    #[serde(default)]
    pub winner: Option<String>,
    #[serde(default)]
    pub ended_at: Option<u64>,
}

impl Race {
    pub fn is_open(&self) -> bool {
        self.ended_at.is_none()
    }
}

/// Whether these lanes make a race: two to four, each naming an engine.
pub fn check_lanes(lanes: &[Lane]) -> Result<()> {
    if lanes.len() < FEWEST || lanes.len() > MOST {
        bail!("a race takes {FEWEST} to {MOST} entrants, not {}", lanes.len());
    }
    if lanes.iter().any(|lane| lane.engine_id.trim().is_empty()) {
        bail!("every entrant needs an engine");
    }
    Ok(())
}

fn letter(index: usize) -> char {
    (b'a' + (index % 26) as u8) as char
}

/// An entrant's name: the race and a letter, "r7a".
///
/// Named for the race rather than the card because race numbers are never used
/// twice, and a card can be raced again after a race is called off: its old
/// entrants' branches are still there, and a new entrant named like an old one
/// would have started on top of that work.
pub fn entrant_name(race_id: &str, index: usize) -> String {
    format!("{race_id}{}", letter(index))
}

pub fn worktree_name(race_id: &str, index: usize) -> String {
    format!("race-{}", entrant_name(race_id, index))
}

/// What each entrant is told ahead of the card itself.
///
/// Not the card's id: an entrant that knows it can move the card or open its
/// pull request, and in a race both come after a person has chosen.
pub fn race_brief(card_brief: &str, entrants: usize) -> String {
    format!(
        "You are one of {entrants} agents given this same work at the same time, each in a worktree of its own and on a different engine or model. A person will read every result side by side and keep one.\n\nWork only in this worktree, and commit what you make on its branch. Do not open a pull request and do not move the card: whoever is kept is told what comes next.\n\n{}",
        card_brief.trim()
    )
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct State {
    #[serde(default)]
    races: BTreeMap<String, Race>,
    #[serde(default)]
    next_number: u32,
}

pub struct Races {
    state: Mutex<State>,
    data_dir: PathBuf,
}

fn number(id: &str) -> u32 {
    id.trim_start_matches('r').parse().unwrap_or(0)
}

impl Races {
    pub fn new(data_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&data_dir);
        let data_dir = crate::exec::settled(&data_dir);
        let state = crate::db::load_state(&data_dir, "races");
        Self {
            state: Mutex::new(state),
            data_dir,
        }
    }

    fn persist(&self, state: &State) {
        crate::db::save_state(&self.data_dir, "races", state);
    }

    /// Newest first.
    pub fn list(&self) -> Vec<Race> {
        let mut races: Vec<Race> = self.state.lock().races.values().cloned().collect();
        races.sort_by(|one, other| number(&other.id).cmp(&number(&one.id)));
        races
    }

    pub fn get(&self, id: &str) -> Option<Race> {
        self.state.lock().races.get(id).cloned()
    }

    pub fn open_for(&self, task_id: &str) -> Option<Race> {
        self.state
            .lock()
            .races
            .values()
            .find(|race| race.task_id == task_id && race.is_open())
            .cloned()
    }

    /// Put a race on the card before anybody is entered, so the card is taken
    /// while entrants are being made and the race has a number to name them by.
    pub fn open(&self, task_id: &str, repository_id: &str, now: u64) -> Result<Race> {
        let mut state = self.state.lock();
        if state.races.values().any(|race| race.task_id == task_id && race.is_open()) {
            bail!("{task_id} is already being raced");
        }

        state.next_number += 1;
        let race = Race {
            id: format!("r{}", state.next_number),
            task_id: task_id.to_owned(),
            repository_id: repository_id.to_owned(),
            entrants: Vec::new(),
            started_at: now,
            winner: None,
            ended_at: None,
        };
        state.races.insert(race.id.clone(), race.clone());
        self.persist(&state);
        Ok(race)
    }

    pub fn set_entrants(&self, id: &str, entrants: Vec<Entrant>) -> Result<Race> {
        let mut state = self.state.lock();
        let race = state
            .races
            .get_mut(id)
            .ok_or_else(|| anyhow!("unknown race: {id}"))?;
        race.entrants = entrants;
        let held = race.clone();
        self.persist(&state);
        Ok(held)
    }

    /// Take back a race that never got going. Its number is not given out again.
    pub fn forget(&self, id: &str) {
        let mut state = self.state.lock();
        if state.races.remove(id).is_some() {
            self.persist(&state);
        }
    }

    pub fn decide(&self, id: &str, winner: &str, now: u64) -> Result<Race> {
        let mut state = self.state.lock();
        let race = state
            .races
            .get_mut(id)
            .ok_or_else(|| anyhow!("unknown race: {id}"))?;
        if !race.is_open() {
            bail!("{id} is already over");
        }
        if !race.entrants.iter().any(|entrant| entrant.agent_id == winner) {
            bail!("{winner} is not racing in {id}");
        }

        race.winner = Some(winner.to_owned());
        race.ended_at = Some(now);
        let decided = race.clone();
        self.persist(&state);
        Ok(decided)
    }

    pub fn call_off(&self, id: &str, now: u64) -> Result<Race> {
        let mut state = self.state.lock();
        let race = state
            .races
            .get_mut(id)
            .ok_or_else(|| anyhow!("unknown race: {id}"))?;
        if !race.is_open() {
            bail!("{id} is already over");
        }

        race.ended_at = Some(now);
        let ended = race.clone();
        self.persist(&state);
        Ok(ended)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("agentland-races-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn lane(engine_id: &str, model: Option<&str>) -> Lane {
        Lane {
            engine_id: engine_id.to_owned(),
            model: model.map(str::to_owned),
        }
    }

    fn entrant(agent_id: &str) -> Entrant {
        Entrant {
            agent_id: agent_id.to_owned(),
            name: agent_id.to_owned(),
            engine_id: "claude".to_owned(),
            model: None,
            worktree: format!("race-{agent_id}"),
            branch: format!("agent/race-{agent_id}"),
        }
    }

    #[test]
    fn a_race_takes_two_to_four_entrants_each_on_an_engine() {
        assert!(check_lanes(&[lane("claude", None)]).is_err());
        assert!(check_lanes(&[lane("claude", None), lane("codex", None)]).is_ok());
        assert!(check_lanes(&vec![lane("claude", None); 5]).is_err());
        assert!(check_lanes(&[lane("claude", None), lane("  ", None)]).is_err());
    }

    #[test]
    fn entrants_are_named_for_the_race_and_a_letter() {
        assert_eq!(entrant_name("r7", 0), "r7a");
        assert_eq!(worktree_name("r7", 2), "race-r7c");
    }

    #[test]
    fn a_lane_says_its_engine_and_model() {
        assert_eq!(lane("claude", Some("opus")).words(), "claude · opus");
        assert_eq!(lane("codex", None).words(), "codex");
        assert_eq!(lane("codex", Some("  ")).words(), "codex");
        assert_eq!(lane("codex", Some("  ")).model(), None);
    }

    #[test]
    fn the_brief_says_it_is_a_race_and_hands_over_nothing_else() {
        let said = race_brief("Add /health\n", 3);

        assert!(said.starts_with("You are one of 3 agents"));
        assert!(said.contains("Do not open a pull request"));
        assert!(said.ends_with("Add /health"));
    }

    #[test]
    fn one_card_is_raced_once_at_a_time() {
        let races = Races::new(scratch("once"));
        let race = races.open("t12", "shop", 10).unwrap();

        assert_eq!(race.id, "r1");
        assert!(races.open("t12", "shop", 11).is_err());
        assert_eq!(races.open_for("t12").map(|held| held.id), Some("r1".to_owned()));
        assert!(races.open("t13", "shop", 12).is_ok(), "another card is another race");
    }

    #[test]
    fn the_winner_has_to_be_racing_and_a_race_ends_once() {
        let races = Races::new(scratch("winner"));
        let race = races.open("t12", "shop", 10).unwrap();
        races.set_entrants(&race.id, vec![entrant("r1a"), entrant("r1b")]).unwrap();

        assert!(races.decide(&race.id, "ada", 20).is_err());

        let decided = races.decide(&race.id, "r1b", 20).unwrap();
        assert_eq!(decided.winner.as_deref(), Some("r1b"));
        assert!(!decided.is_open());
        assert!(races.open_for("t12").is_none());
        assert!(races.decide(&race.id, "r1a", 30).is_err());
        assert!(races.call_off(&race.id, 30).is_err());
    }

    #[test]
    fn a_race_called_off_frees_the_card_under_a_new_number() {
        let races = Races::new(scratch("called-off"));
        let first = races.open("t12", "shop", 10).unwrap();

        let ended = races.call_off(&first.id, 20).unwrap();
        assert_eq!(ended.winner, None);

        let again = races.open("t12", "shop", 30).unwrap();
        assert_eq!(again.id, "r2");
        assert_eq!(races.list().iter().map(|race| race.id.as_str()).collect::<Vec<_>>(), vec!["r2", "r1"]);
    }

    #[test]
    fn a_race_that_never_got_going_is_forgotten_but_its_number_is_not_reused() {
        let races = Races::new(scratch("forget"));
        let race = races.open("t12", "shop", 10).unwrap();

        races.forget(&race.id);
        assert!(races.get(&race.id).is_none());
        assert!(races.open_for("t12").is_none());
        assert_eq!(races.open("t12", "shop", 20).unwrap().id, "r2");
    }

    #[test]
    fn races_outlive_a_restart() {
        let dir = scratch("restart");
        {
            let races = Races::new(dir.clone());
            let race = races.open("t12", "shop", 10).unwrap();
            races.set_entrants(&race.id, vec![entrant("r1a"), entrant("r1b")]).unwrap();
        }

        let races = Races::new(dir);
        let race = races.open_for("t12").expect("the race is still on");
        assert_eq!(race.entrants.len(), 2);
        assert_eq!(races.open("t13", "shop", 20).unwrap().id, "r2");
    }
}
