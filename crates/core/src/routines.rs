use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, bail, Result};
use chrono::{DateTime, Datelike, Days, Local, NaiveDate, NaiveTime, TimeZone, Weekday};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

/// Two failures in a row pause a routine unless somebody asked for another
/// number. One failure is weather; two is a broken assumption being paid for on
/// a timer.
const DEFAULT_PAUSE_AFTER: u32 = 2;

/// How many runs a routine remembers. Enough to see a pattern — "it has been
/// skipped every morning this week" — and short enough to read.
const HISTORY_KEPT: usize = 20;

/// How long a routine waits for a busy agent before letting the run go.
///
/// An agent mid-turn is not a failure: it is doing something, possibly the
/// very thing the routine would have asked for. So the run waits for the pane
/// to come to rest, and only after half an hour of it staying busy is the run
/// written down as skipped — never as a failure, which is what used to pause a
/// routine aimed at a commander that simply happened to be working.
pub const BUSY_GRACE: u64 = 30 * 60;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Day {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

impl Day {
    fn of(weekday: Weekday) -> Self {
        match weekday {
            Weekday::Mon => Day::Mon,
            Weekday::Tue => Day::Tue,
            Weekday::Wed => Day::Wed,
            Weekday::Thu => Day::Thu,
            Weekday::Fri => Day::Fri,
            Weekday::Sat => Day::Sat,
            Weekday::Sun => Day::Sun,
        }
    }
}

const WEEKDAYS: [Day; 5] = [Day::Mon, Day::Tue, Day::Wed, Day::Thu, Day::Fri];

/// When a routine runs, in the machine's own clock.
///
/// An interval alone could say "every hour" and nothing else, so a morning
/// triage ran at whatever minute the app happened to be opened and a check
/// meant for working hours ran all night. Two shapes cover what people ask
/// for: every so often inside an optional window of the day, or at set times.
/// Both can be held to chosen weekdays; no days means every day.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Schedule {
    Every {
        minutes: u32,
        /// The active window, as `HH:MM`. Both or neither; `from` later than
        /// `to` is a window across midnight.
        #[serde(default)]
        from: Option<String>,
        #[serde(default)]
        to: Option<String>,
        #[serde(default)]
        days: Vec<Day>,
    },
    Daily {
        times: Vec<String>,
        #[serde(default)]
        days: Vec<Day>,
    },
}

/// Where a routine's brief goes.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Delivery {
    /// A card on the agent's project, handed to the agent. The card is the
    /// record: it carries the result and goes through review like any work.
    #[default]
    Card,
    /// Said into the agent's own pane, with no card. For a commander or a chief,
    /// whose job on a timer is to look around and decide, not to produce a diff.
    Pane,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunOutcome {
    Ran,
    Failed,
    Skipped,
}

/// One run, as it is remembered.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Run {
    pub at: u64,
    pub outcome: RunOutcome,
    pub detail: String,
    #[serde(default)]
    pub card: Option<String>,
}

/// What the ticker made of a routine that was due.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Ran { detail: String, card: Option<String> },
    Failed(String),
    /// Deliberately not run: the week was tight, the last card is still open,
    /// the agent stayed busy. Not a failure, so it never pauses anything.
    Skipped(String),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(from = "Stored")]
pub struct Routine {
    pub id: String,
    pub name: String,
    pub agent_id: String,
    pub brief: String,
    pub schedule: Schedule,
    pub delivery: Delivery,
    pub draft_only: bool,
    /// Leave a run out when the agent's allowance says to start nothing new.
    pub skip_when_tight: bool,
    /// Leave a run out while the card from the last one is still open, so a
    /// routine nobody is keeping up with does not bury the board.
    pub one_at_a_time: bool,
    pub pause_after_failures: u32,
    pub enabled: bool,
    /// The agent that proposed it; `None` is a person.
    pub created_by: Option<String>,
    pub created_at: u64,
    pub last_run: u64,
    pub consecutive_failures: u32,
    pub last_result: Option<String>,
    pub last_card: Option<String>,
    /// When the current run started waiting for a busy agent; 0 when it is not.
    pub waiting_since: u64,
    /// Newest first.
    pub history: Vec<Run>,
    /// Worked out when the routines are listed, so a screen never has to
    /// reimplement the schedule to say when it runs next.
    pub next_run: Option<u64>,
}

/// A routine as it may have been written by any version of this app.
///
/// The first routines were an interval and nothing else, stored as
/// `every_minutes`. They are read as `Every` with no window rather than
/// dropped: a person's routine disappearing on an update is worse than any
/// schedule it might have had.
#[derive(Deserialize)]
struct Stored {
    id: String,
    name: String,
    agent_id: String,
    brief: String,
    #[serde(default)]
    schedule: Option<Schedule>,
    #[serde(default)]
    every_minutes: Option<u32>,
    #[serde(default)]
    delivery: Delivery,
    #[serde(default)]
    draft_only: bool,
    #[serde(default = "yes")]
    skip_when_tight: bool,
    #[serde(default = "yes")]
    one_at_a_time: bool,
    #[serde(default = "default_pause_after")]
    pause_after_failures: u32,
    #[serde(default = "yes")]
    enabled: bool,
    #[serde(default)]
    created_by: Option<String>,
    #[serde(default)]
    created_at: u64,
    #[serde(default)]
    last_run: u64,
    #[serde(default)]
    consecutive_failures: u32,
    #[serde(default)]
    last_result: Option<String>,
    #[serde(default)]
    last_card: Option<String>,
    #[serde(default)]
    waiting_since: u64,
    #[serde(default)]
    history: Vec<Run>,
}

impl From<Stored> for Routine {
    fn from(stored: Stored) -> Self {
        let schedule = stored.schedule.unwrap_or(Schedule::Every {
            minutes: stored.every_minutes.unwrap_or(60).max(1),
            from: None,
            to: None,
            days: Vec::new(),
        });

        Routine {
            id: stored.id,
            name: stored.name,
            agent_id: stored.agent_id,
            brief: stored.brief,
            schedule,
            delivery: stored.delivery,
            draft_only: stored.draft_only,
            skip_when_tight: stored.skip_when_tight,
            one_at_a_time: stored.one_at_a_time,
            pause_after_failures: stored.pause_after_failures.max(1),
            enabled: stored.enabled,
            created_by: stored.created_by,
            created_at: stored.created_at,
            last_run: stored.last_run,
            consecutive_failures: stored.consecutive_failures,
            last_result: stored.last_result,
            last_card: stored.last_card,
            waiting_since: stored.waiting_since,
            history: stored.history,
            next_run: None,
        }
    }
}

fn yes() -> bool {
    true
}

fn default_pause_after() -> u32 {
    DEFAULT_PAUSE_AFTER
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct CreateRoutine {
    pub name: String,
    pub agent_id: String,
    pub brief: String,
    #[serde(default)]
    pub schedule: Option<Schedule>,
    /// What an older window sends instead of a schedule.
    #[serde(default)]
    pub every_minutes: Option<u32>,
    #[serde(default)]
    pub delivery: Delivery,
    #[serde(default)]
    pub draft_only: bool,
    #[serde(default)]
    pub skip_when_tight: Option<bool>,
    #[serde(default)]
    pub one_at_a_time: Option<bool>,
    #[serde(default)]
    pub pause_after_failures: Option<u32>,
    #[serde(default)]
    pub created_by: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

/// A change to a routine: only what is given is touched.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct UpdateRoutine {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub brief: Option<String>,
    #[serde(default)]
    pub schedule: Option<Schedule>,
    #[serde(default)]
    pub delivery: Option<Delivery>,
    #[serde(default)]
    pub draft_only: Option<bool>,
    #[serde(default)]
    pub skip_when_tight: Option<bool>,
    #[serde(default)]
    pub one_at_a_time: Option<bool>,
    #[serde(default)]
    pub pause_after_failures: Option<u32>,
    #[serde(default)]
    pub enabled: Option<bool>,
    /// Who is asking, so a notice about the change can name them.
    #[serde(default)]
    pub by: Option<String>,
}

impl UpdateRoutine {
    /// Whether this change does anything but switch the routine on or off.
    pub fn changes_more_than_the_switch(&self) -> bool {
        self.name.is_some()
            || self.agent_id.is_some()
            || self.brief.is_some()
            || self.schedule.is_some()
            || self.delivery.is_some()
            || self.draft_only.is_some()
            || self.skip_when_tight.is_some()
            || self.one_at_a_time.is_some()
            || self.pause_after_failures.is_some()
    }
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

fn clock(text: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(text.trim(), "%H:%M").ok()
}

fn window_of(from: &Option<String>, to: &Option<String>) -> Option<(NaiveTime, NaiveTime)> {
    Some((clock(from.as_deref()?)?, clock(to.as_deref()?)?))
}

fn allowed_on(days: &[Day], date: NaiveDate) -> bool {
    days.is_empty() || days.contains(&Day::of(date.weekday()))
}

/// The earliest time at or after `start`, on one day, that the window allows.
fn earliest_in_window(start: NaiveTime, window: Option<(NaiveTime, NaiveTime)>) -> Option<NaiveTime> {
    let Some((from, to)) = window else {
        return Some(start);
    };

    if from == to {
        return Some(start);
    }

    if from < to {
        if start < from {
            Some(from)
        } else if start < to {
            Some(start)
        } else {
            None
        }
    } else if start < to || start >= from {
        Some(start)
    } else {
        Some(from)
    }
}

/// A wall-clock time on a date, in a zone. A time the clocks skip over on the
/// night they go forward is taken an hour later rather than never.
fn at<Tz: TimeZone>(zone: &Tz, date: NaiveDate, time: NaiveTime) -> Option<DateTime<Tz>> {
    let naive = date.and_time(time);
    zone.from_local_datetime(&naive)
        .earliest()
        .or_else(|| zone.from_local_datetime(&(naive + chrono::Duration::hours(1))).earliest())
}

fn seconds_of<Tz: TimeZone>(moment: &DateTime<Tz>) -> u64 {
    moment.timestamp().max(0) as u64
}

/// The first moment at or after `secs` that the days and the window allow.
fn first_allowed<Tz: TimeZone>(
    zone: &Tz,
    secs: u64,
    window: Option<(NaiveTime, NaiveTime)>,
    days: &[Day],
) -> Option<u64> {
    let local = zone.timestamp_opt(secs as i64, 0).single()?.naive_local();

    for offset in 0..=7u64 {
        let date = local.date().checked_add_days(Days::new(offset))?;
        if !allowed_on(days, date) {
            continue;
        }

        let start = if offset == 0 { local.time() } else { NaiveTime::MIN };
        if let Some(time) = earliest_in_window(start, window) {
            if offset == 0 && time == local.time() {
                return Some(secs);
            }
            return at(zone, date, time).map(|moment| seconds_of(&moment));
        }
    }

    None
}

/// When a routine runs next, in the given zone.
///
/// Never earlier than `now`: a run that was missed — the app was closed at
/// nine — is due now, once, rather than once for every time it was missed. An
/// interval counts from the last run, and a routine that has never run is due
/// the moment its window opens; set times count from the last run, or from
/// when the routine was made, so a routine made at noon for nine in the
/// morning waits for tomorrow instead of firing the moment it is saved.
pub fn next_run_in<Tz: TimeZone>(routine: &Routine, now: u64, zone: &Tz) -> Option<u64> {
    match &routine.schedule {
        Schedule::Every { minutes, from, to, days } => {
            let candidate = if routine.last_run == 0 {
                now
            } else {
                routine.last_run + (*minutes).max(1) as u64 * 60
            };

            first_allowed(zone, candidate.max(now), window_of(from, to), days)
        }
        Schedule::Daily { times, days } => {
            let anchor = if routine.last_run > 0 {
                routine.last_run
            } else if routine.created_at > 0 {
                routine.created_at.min(now)
            } else {
                now
            };

            let mut clocks: Vec<NaiveTime> = times.iter().filter_map(|time| clock(time)).collect();
            clocks.sort();
            clocks.dedup();

            let local = zone.timestamp_opt(anchor as i64, 0).single()?.naive_local();

            for offset in 0..=8u64 {
                let date = local.date().checked_add_days(Days::new(offset))?;
                if !allowed_on(days, date) {
                    continue;
                }

                for time in &clocks {
                    let Some(moment) = at(zone, date, *time) else {
                        continue;
                    };
                    let secs = seconds_of(&moment);
                    if secs > anchor {
                        return Some(secs.max(now));
                    }
                }
            }

            None
        }
    }
}

pub fn next_run(routine: &Routine, now: u64) -> Option<u64> {
    next_run_in(routine, now, &Local)
}

pub fn is_due_in<Tz: TimeZone>(routine: &Routine, now: u64, zone: &Tz) -> bool {
    routine.enabled && next_run_in(routine, now, zone).is_some_and(|at| at <= now)
}

pub fn is_due(routine: &Routine, now: u64) -> bool {
    is_due_in(routine, now, &Local)
}

/// A schedule as it should be kept: times written the one way, days in order,
/// and anything that cannot be a schedule refused in words that say how to
/// write it instead.
pub fn settle_schedule(schedule: Schedule) -> Result<Schedule> {
    fn days_in_order(mut days: Vec<Day>) -> Vec<Day> {
        days.sort();
        days.dedup();
        days
    }

    fn written(text: &str) -> Result<String> {
        clock(text)
            .map(|time| time.format("%H:%M").to_string())
            .ok_or_else(|| anyhow!("\"{}\" is not a time — write it as 24-hour HH:MM, like 09:00", text.trim()))
    }

    match schedule {
        Schedule::Every { minutes, from, to, days } => {
            if minutes == 0 {
                bail!("a routine runs at most once a minute — every 0 minutes is not a schedule");
            }

            let from = from.filter(|value| !value.trim().is_empty());
            let to = to.filter(|value| !value.trim().is_empty());
            let (from, to) = match (from, to) {
                (Some(from), Some(to)) => (Some(written(&from)?), Some(written(&to)?)),
                (None, None) => (None, None),
                _ => bail!("an active window needs both ends — from and to, like 09:00 and 18:00"),
            };

            Ok(Schedule::Every { minutes, from, to, days: days_in_order(days) })
        }
        Schedule::Daily { times, days } => {
            let mut kept = times
                .iter()
                .filter(|time| !time.trim().is_empty())
                .map(|time| written(time))
                .collect::<Result<Vec<_>>>()?;
            kept.sort();
            kept.dedup();

            if kept.is_empty() {
                bail!("a daily routine needs at least one time to run at, like 09:00");
            }

            Ok(Schedule::Daily { times: kept, days: days_in_order(days) })
        }
    }
}

/// The words a brief may carry in braces, filled in when it runs, and what
/// each becomes.
pub const VARIABLES: [(&str, &str); 7] = [
    ("date", "today, as 2026-09-15"),
    ("time", "the time it runs, as 09:00"),
    ("weekday", "today's name, as Tuesday"),
    ("routine", "this routine's name"),
    ("agent", "the name of the agent it runs on"),
    ("last_run", "when it last ran, or never"),
    ("last_result", "what the last run came to"),
];

/// What each variable is on this run.
pub fn values_in<Tz: TimeZone>(
    routine: &Routine,
    agent: &str,
    now: u64,
    zone: &Tz,
) -> Vec<(&'static str, String)> {
    let local = |secs: u64| {
        zone.timestamp_opt(secs as i64, 0)
            .single()
            .map(|moment| moment.naive_local())
    };
    let here = local(now);
    let stamp = |format: &str| here.map(|moment| moment.format(format).to_string()).unwrap_or_default();

    let last_run = if routine.last_run == 0 {
        "never".to_owned()
    } else {
        local(routine.last_run)
            .map(|moment| moment.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "never".to_owned())
    };

    vec![
        ("date", stamp("%Y-%m-%d")),
        ("time", stamp("%H:%M")),
        ("weekday", stamp("%A")),
        ("routine", routine.name.clone()),
        ("agent", agent.to_owned()),
        ("last_run", last_run),
        (
            "last_result",
            routine
                .last_result
                .clone()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "nothing yet".to_owned()),
        ),
    ]
}

/// A brief with its variables filled in. Braces around anything that is not
/// a variable are left as they were: a brief quoting a JSON object is not a
/// template.
pub fn expand(brief: &str, values: &[(&str, String)]) -> String {
    values.iter().fold(brief.to_owned(), |text, (name, value)| {
        text.replace(&format!("{{{name}}}"), value)
    })
}

/// A routine worth starting from.
#[derive(Clone, Debug, Serialize)]
pub struct Template {
    pub id: &'static str,
    pub name: &'static str,
    pub summary: &'static str,
    /// Who it is written for: `commander` for a brief that looks across the
    /// crew, `any` for work one agent does.
    pub suits: &'static str,
    pub brief: &'static str,
    pub schedule: Schedule,
    pub delivery: Delivery,
    pub draft_only: bool,
    pub skip_when_tight: bool,
    pub one_at_a_time: bool,
}

fn times(list: &[&str]) -> Vec<String> {
    list.iter().map(|time| (*time).to_owned()).collect()
}

/// The routines a crew most often wants, written once here so the panel and
/// the crew's own tools offer the same ones.
pub fn templates() -> Vec<Template> {
    vec![
        Template {
            id: "morning-triage",
            name: "Morning triage",
            summary: "weekdays at 09:00, on a card: what needs a person and what the crew can take",
            suits: "any",
            brief: "It is {weekday} {date}, {time}. Read what happened since {last_run}: failing checks, new issues, cards that stalled in working or review, and anything the crew wrote down in the vault. On this card, write one short list — what needs a person, what the crew can take, most urgent first. Fix nothing yet.",
            schedule: Schedule::Daily { times: times(&["09:00"]), days: WEEKDAYS.to_vec() },
            delivery: Delivery::Card,
            draft_only: false,
            skip_when_tight: true,
            one_at_a_time: true,
        },
        Template {
            id: "pull-request-sweep",
            name: "Open pull request sweep",
            summary: "weekdays at 10:00 and 16:00, in the commander's pane: move what is stuck",
            suits: "commander",
            brief: "Sweep the open pull requests ({weekday} {time}). For each one: is it reviewed, green and mergeable, and who is it waiting on? Nudge whoever is holding a stuck one with crew_message, put what is ready in front of the person, and say in three lines what moved since {last_run}.",
            schedule: Schedule::Daily { times: times(&["10:00", "16:00"]), days: WEEKDAYS.to_vec() },
            delivery: Delivery::Pane,
            draft_only: false,
            skip_when_tight: true,
            one_at_a_time: true,
        },
        Template {
            id: "dependency-check",
            name: "Nightly dependency check",
            summary: "every night at 02:30, on a card, prepared but never pushed",
            suits: "any",
            brief: "Nightly dependency and security check for {date}. List outdated and vulnerable dependencies with the tools this project already uses — npm audit, cargo audit, pip-audit, whichever apply — and read the advisories that matter. Prepare the smallest upgrade that fixes what is serious, on this card's branch. The last check came to: {last_result}.",
            schedule: Schedule::Daily { times: times(&["02:30"]), days: Vec::new() },
            delivery: Delivery::Card,
            draft_only: true,
            skip_when_tight: true,
            one_at_a_time: true,
        },
        Template {
            id: "weekly-recap",
            name: "Weekly recap",
            summary: "Fridays at 17:00, in the commander's pane: what shipped and what was learned",
            suits: "commander",
            brief: "Write the recap of the week ending {date}: what shipped, what is still open and why, and what the crew learned that belongs in the vault. Put it in one note with note_write, linked to the notes it draws on, then tell the person in five lines.",
            schedule: Schedule::Daily { times: times(&["17:00"]), days: vec![Day::Fri] },
            delivery: Delivery::Pane,
            draft_only: false,
            skip_when_tight: true,
            one_at_a_time: true,
        },
        Template {
            id: "vault-tidy",
            name: "Vault tidy",
            summary: "Mondays at 09:30, in the pane: note_lint, chores fixed, decisions proposed",
            suits: "commander",
            brief: "Tidy the vault; the last tidy was {last_run}. Run note_lint. Fix what is a chore — a link pointing at a note that was renamed, a note nothing points at that belongs on a map. Propose what is a decision: two memories saying the same thing, a correction that left both sides in force. Then say what you changed and what waits on a person.",
            schedule: Schedule::Daily { times: times(&["09:30"]), days: vec![Day::Mon] },
            delivery: Delivery::Pane,
            draft_only: false,
            skip_when_tight: true,
            one_at_a_time: true,
        },
        Template {
            id: "flaky-test-hunt",
            name: "Flaky test hunt",
            summary: "every 4 hours in working hours on weekdays, on a card: one flaky test found and fixed",
            suits: "any",
            brief: "Hunt one flaky test. Run the suite a few times, or read the recent check runs, and find a test that passes and fails on the same code. Find why, fix it on this card's branch, and write down in a note what made it flaky. If nothing flakes, say so and stop. The previous hunt came to: {last_result}.",
            schedule: Schedule::Every {
                minutes: 240,
                from: Some("09:00".to_owned()),
                to: Some("19:00".to_owned()),
                days: WEEKDAYS.to_vec(),
            },
            delivery: Delivery::Card,
            draft_only: false,
            skip_when_tight: true,
            one_at_a_time: true,
        },
    ]
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

    /// Every routine, each with when it next runs. A paused one runs never.
    pub fn list(&self, now: u64) -> Vec<Routine> {
        self.state
            .lock()
            .routines
            .values()
            .cloned()
            .map(|mut routine| {
                routine.next_run = routine
                    .enabled
                    .then(|| next_run(&routine, now))
                    .flatten();
                routine
            })
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<Routine> {
        self.state.lock().routines.get(id).cloned()
    }

    /// Make a routine. One an agent proposes starts paused: a routine spends
    /// tokens on a timer, and only a person decides that the crew spends them.
    pub fn create(&self, request: CreateRoutine, now: u64, proposed_by_an_agent: bool) -> Result<Routine> {
        if request.name.trim().is_empty() {
            bail!("a routine needs a name");
        }
        if request.brief.trim().is_empty() {
            bail!("a routine needs a brief — the agent has to be told what to do");
        }
        if request.agent_id.trim().is_empty() {
            bail!("a routine needs an agent to run on");
        }

        let schedule = settle_schedule(request.schedule.unwrap_or(Schedule::Every {
            minutes: request.every_minutes.unwrap_or(60).max(1),
            from: None,
            to: None,
            days: Vec::new(),
        }))?;

        let mut state = self.state.lock();
        state.next_number += 1;
        let id = format!("r{}", state.next_number);

        let routine = Routine {
            id: id.clone(),
            name: request.name.trim().to_owned(),
            agent_id: request.agent_id.trim().to_owned(),
            brief: request.brief.trim().to_owned(),
            schedule,
            delivery: request.delivery,
            draft_only: request.draft_only,
            skip_when_tight: request.skip_when_tight.unwrap_or(true),
            one_at_a_time: request.one_at_a_time.unwrap_or(true),
            pause_after_failures: request.pause_after_failures.unwrap_or(DEFAULT_PAUSE_AFTER).max(1),
            enabled: !proposed_by_an_agent && request.enabled.unwrap_or(true),
            created_by: request
                .created_by
                .map(|by| by.trim().to_owned())
                .filter(|by| !by.is_empty()),
            created_at: now,
            last_run: 0,
            consecutive_failures: 0,
            last_result: proposed_by_an_agent.then(|| "proposed — waiting for a person to turn it on".to_owned()),
            last_card: None,
            waiting_since: 0,
            history: Vec::new(),
            next_run: None,
        };

        state.routines.insert(id, routine.clone());
        self.persist(&state);
        drop(state);

        Ok(self.with_next_run(routine, now))
    }

    fn with_next_run(&self, mut routine: Routine, now: u64) -> Routine {
        routine.next_run = routine.enabled.then(|| next_run(&routine, now)).flatten();
        routine
    }

    /// Change what was asked for. The record of what happened — runs, results,
    /// the failure streak — is left alone, except that switching a routine back
    /// on starts it with a clean streak.
    pub fn update(&self, id: &str, change: UpdateRoutine, now: u64) -> Result<Routine> {
        let schedule = change.schedule.clone().map(settle_schedule).transpose()?;

        let mut state = self.state.lock();
        let routine = state
            .routines
            .get_mut(id)
            .ok_or_else(|| anyhow!("unknown routine: {id}"))?;

        if let Some(name) = change.name.map(|name| name.trim().to_owned()) {
            if name.is_empty() {
                bail!("a routine needs a name");
            }
            routine.name = name;
        }
        if let Some(brief) = change.brief.map(|brief| brief.trim().to_owned()) {
            if brief.is_empty() {
                bail!("a routine needs a brief — the agent has to be told what to do");
            }
            routine.brief = brief;
        }
        if let Some(agent_id) = change.agent_id.map(|agent| agent.trim().to_owned()) {
            if agent_id.is_empty() {
                bail!("a routine needs an agent to run on");
            }
            routine.agent_id = agent_id;
        }
        if let Some(schedule) = schedule {
            routine.schedule = schedule;
        }
        if let Some(delivery) = change.delivery {
            routine.delivery = delivery;
        }
        if let Some(value) = change.draft_only {
            routine.draft_only = value;
        }
        if let Some(value) = change.skip_when_tight {
            routine.skip_when_tight = value;
        }
        if let Some(value) = change.one_at_a_time {
            routine.one_at_a_time = value;
        }
        if let Some(value) = change.pause_after_failures {
            routine.pause_after_failures = value.max(1);
        }
        if let Some(enabled) = change.enabled {
            if enabled && !routine.enabled {
                routine.consecutive_failures = 0;
                routine.waiting_since = 0;
            }
            routine.enabled = enabled;
        }

        let updated = routine.clone();
        self.persist(&state);
        drop(state);

        Ok(self.with_next_run(updated, now))
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<Routine> {
        self.update(
            id,
            UpdateRoutine {
                enabled: Some(enabled),
                ..Default::default()
            },
            now_secs(),
        )
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

    /// Write down what a run came to.
    pub fn record(&self, id: &str, now: u64, outcome: Outcome) -> Option<Routine> {
        let mut state = self.state.lock();
        let routine = state.routines.get_mut(id)?;
        routine.last_run = now;
        routine.waiting_since = 0;

        let run = match outcome {
            Outcome::Ran { detail, card } => {
                routine.consecutive_failures = 0;
                routine.last_result = Some(detail.clone());
                if card.is_some() {
                    routine.last_card = card.clone();
                }
                Run { at: now, outcome: RunOutcome::Ran, detail, card }
            }
            Outcome::Skipped(detail) => {
                routine.last_result = Some(detail.clone());
                Run { at: now, outcome: RunOutcome::Skipped, detail, card: None }
            }
            Outcome::Failed(detail) => {
                routine.consecutive_failures += 1;
                routine.last_result = Some(detail.clone());

                if routine.consecutive_failures >= routine.pause_after_failures.max(1) {
                    routine.enabled = false;
                    routine.last_result = Some(format!(
                        "{detail} — paused after {} failure{} in a row",
                        routine.consecutive_failures,
                        if routine.consecutive_failures == 1 { "" } else { "s" }
                    ));
                }
                Run { at: now, outcome: RunOutcome::Failed, detail, card: None }
            }
        };

        routine.history.insert(0, run);
        routine.history.truncate(HISTORY_KEPT);

        let updated = routine.clone();
        self.persist(&state);
        Some(updated)
    }

    /// A run that has to wait: the agent is mid-turn, or held by its engine.
    /// It stays due and is tried again on the next tick; once it has waited
    /// longer than `BUSY_GRACE` it is let go as skipped.
    pub fn defer(&self, id: &str, now: u64, why: &str) -> Option<Routine> {
        let waited = {
            let mut state = self.state.lock();
            let routine = state.routines.get_mut(id)?;

            if routine.waiting_since == 0 {
                routine.waiting_since = now;
                routine.last_result = Some(format!("waiting — {why}"));
                let updated = routine.clone();
                self.persist(&state);
                return Some(updated);
            }

            now.saturating_sub(routine.waiting_since)
        };

        if waited < BUSY_GRACE {
            return self.get(id);
        }

        self.record(
            id,
            now,
            Outcome::Skipped(format!("skipped — {why} for {} min", waited / 60)),
        )
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use chrono::FixedOffset;

    use super::*;

    /// Istanbul keeps no daylight saving, so a fixed +03:00 is its real clock.
    fn istanbul() -> FixedOffset {
        FixedOffset::east_opt(3 * 3600).expect("offset")
    }

    /// Seconds since the epoch for a wall-clock time in Istanbul.
    fn at_istanbul(date: &str, time: &str) -> u64 {
        let naive = chrono::NaiveDateTime::parse_from_str(&format!("{date} {time}"), "%Y-%m-%d %H:%M")
            .expect("a date and a time");
        istanbul()
            .from_local_datetime(&naive)
            .single()
            .expect("one moment")
            .timestamp() as u64
    }

    fn routine(schedule: Schedule) -> Routine {
        Routine {
            id: "r1".to_owned(),
            name: "morning triage".to_owned(),
            agent_id: "ada".to_owned(),
            brief: "read overnight failures".to_owned(),
            schedule,
            delivery: Delivery::Card,
            draft_only: true,
            skip_when_tight: true,
            one_at_a_time: true,
            pause_after_failures: 2,
            enabled: true,
            created_by: None,
            created_at: 0,
            last_run: 0,
            consecutive_failures: 0,
            last_result: None,
            last_card: None,
            waiting_since: 0,
            history: Vec::new(),
            next_run: None,
        }
    }

    fn every(minutes: u32) -> Schedule {
        Schedule::Every { minutes, from: None, to: None, days: Vec::new() }
    }

    fn store(name: &str) -> Routines {
        let dir = std::env::temp_dir().join(name);
        let _ = fs::remove_dir_all(&dir);
        Routines::new(dir)
    }

    fn ask(name: &str) -> CreateRoutine {
        CreateRoutine {
            name: name.to_owned(),
            agent_id: "ada".to_owned(),
            brief: "read overnight failures".to_owned(),
            every_minutes: Some(60),
            draft_only: true,
            ..Default::default()
        }
    }

    #[test]
    fn a_routine_is_due_only_after_its_interval() {
        let mut entry = routine(every(60));
        entry.last_run = 1_000;

        assert!(!is_due_in(&entry, 1_000 + 3_599, &istanbul()));
        assert!(is_due_in(&entry, 1_000 + 3_600, &istanbul()));
    }

    #[test]
    fn a_disabled_routine_never_runs() {
        let mut entry = routine(every(60));
        entry.enabled = false;
        assert!(!is_due_in(&entry, 2_000_000_000, &istanbul()));
    }

    #[test]
    fn a_routine_written_before_schedules_existed_still_loads() {
        let old = r#"{"routines":{"r1":{"id":"r1","name":"recap","agent_id":"ada","brief":"write the weekly recap","every_minutes":30,"draft_only":true,"enabled":true,"last_run":1789000000,"consecutive_failures":1,"last_result":"card t9 handed to Ada"}},"next_number":1}"#;
        let state: State = serde_json::from_str(old).expect("the old shape still parses");
        let held = &state.routines["r1"];

        assert_eq!(held.schedule, every(30));
        assert_eq!(held.delivery, Delivery::Card);
        assert!(held.draft_only);
        assert_eq!(held.pause_after_failures, 2);
        assert_eq!(held.consecutive_failures, 1);
        assert!(held.history.is_empty());
    }

    #[test]
    fn an_interval_waits_for_its_window_to_open() {
        let mut entry = routine(Schedule::Every {
            minutes: 240,
            from: Some("09:00".to_owned()),
            to: Some("19:00".to_owned()),
            days: Vec::new(),
        });
        entry.last_run = at_istanbul("2026-09-15", "18:00");

        // Four hours on is ten at night, outside the window: the next run is
        // when the window opens the next morning.
        let next = next_run_in(&entry, at_istanbul("2026-09-15", "18:30"), &istanbul());
        assert_eq!(next, Some(at_istanbul("2026-09-16", "09:00")));
    }

    #[test]
    fn an_overdue_interval_outside_its_window_still_waits_for_it() {
        let mut entry = routine(Schedule::Every {
            minutes: 60,
            from: Some("09:00".to_owned()),
            to: Some("18:00".to_owned()),
            days: Vec::new(),
        });
        entry.last_run = at_istanbul("2026-09-14", "17:30");
        let now = at_istanbul("2026-09-15", "03:00");

        assert!(!is_due_in(&entry, now, &istanbul()), "long overdue, but the window is shut");
        assert_eq!(next_run_in(&entry, now, &istanbul()), Some(at_istanbul("2026-09-15", "09:00")));
    }

    #[test]
    fn a_window_across_midnight_is_open_on_both_sides_of_it() {
        let entry = routine(Schedule::Every {
            minutes: 30,
            from: Some("22:00".to_owned()),
            to: Some("06:00".to_owned()),
            days: Vec::new(),
        });

        assert!(is_due_in(&entry, at_istanbul("2026-09-15", "23:15"), &istanbul()));
        assert!(is_due_in(&entry, at_istanbul("2026-09-15", "05:15"), &istanbul()));
        assert_eq!(
            next_run_in(&entry, at_istanbul("2026-09-15", "12:00"), &istanbul()),
            Some(at_istanbul("2026-09-15", "22:00"))
        );
    }

    #[test]
    fn weekdays_skip_the_weekend() {
        // 2026-09-18 is a Friday.
        let mut entry = routine(Schedule::Daily {
            times: vec!["09:00".to_owned()],
            days: WEEKDAYS.to_vec(),
        });
        entry.last_run = at_istanbul("2026-09-18", "09:00");

        assert_eq!(
            next_run_in(&entry, at_istanbul("2026-09-18", "12:00"), &istanbul()),
            Some(at_istanbul("2026-09-21", "09:00")),
            "Friday's run is followed by Monday's"
        );
    }

    #[test]
    fn set_times_run_in_order_through_the_day() {
        let mut entry = routine(Schedule::Daily {
            times: vec!["16:00".to_owned(), "10:00".to_owned()],
            days: Vec::new(),
        });
        entry.last_run = at_istanbul("2026-09-15", "10:00");

        assert_eq!(
            next_run_in(&entry, at_istanbul("2026-09-15", "11:00"), &istanbul()),
            Some(at_istanbul("2026-09-15", "16:00"))
        );
    }

    #[test]
    fn a_daily_routine_made_after_its_time_waits_for_tomorrow() {
        let mut entry = routine(Schedule::Daily { times: vec!["09:00".to_owned()], days: Vec::new() });
        entry.created_at = at_istanbul("2026-09-15", "12:00");

        assert_eq!(
            next_run_in(&entry, at_istanbul("2026-09-15", "12:00"), &istanbul()),
            Some(at_istanbul("2026-09-16", "09:00"))
        );
    }

    #[test]
    fn a_missed_run_is_due_once_not_once_per_miss() {
        let mut entry = routine(Schedule::Daily { times: vec!["02:30".to_owned()], days: Vec::new() });
        entry.last_run = at_istanbul("2026-09-12", "02:30");
        let now = at_istanbul("2026-09-15", "10:00");

        assert_eq!(next_run_in(&entry, now, &istanbul()), Some(now), "three nights missed, one run");

        entry.last_run = now;
        assert_eq!(
            next_run_in(&entry, now + 60, &istanbul()),
            Some(at_istanbul("2026-09-16", "02:30"))
        );
    }

    #[test]
    fn a_schedule_is_kept_the_one_way_and_refused_in_words() {
        let settled = settle_schedule(Schedule::Daily {
            times: vec!["16:00".to_owned(), "09:00".to_owned(), "09:00".to_owned(), " ".to_owned()],
            days: vec![Day::Fri, Day::Mon, Day::Fri],
        })
        .expect("a fine schedule");
        assert_eq!(
            settled,
            Schedule::Daily {
                times: vec!["09:00".to_owned(), "16:00".to_owned()],
                days: vec![Day::Mon, Day::Fri],
            }
        );

        let wrong = settle_schedule(Schedule::Daily { times: vec!["9am".to_owned()], days: Vec::new() });
        assert!(wrong.unwrap_err().to_string().contains("HH:MM"));

        let half = settle_schedule(Schedule::Every {
            minutes: 30,
            from: Some("09:00".to_owned()),
            to: None,
            days: Vec::new(),
        });
        assert!(half.unwrap_err().to_string().contains("both ends"));

        assert!(settle_schedule(every(0)).is_err());
        assert!(settle_schedule(Schedule::Daily { times: Vec::new(), days: Vec::new() }).is_err());
    }

    #[test]
    fn a_brief_is_filled_in_when_it_runs() {
        let mut entry = routine(every(60));
        entry.brief = "{weekday} {date} at {time}: {routine} for {agent}, last {last_run} ({last_result}) — keep {json}".to_owned();
        entry.last_run = at_istanbul("2026-09-14", "09:00");
        entry.last_result = Some("card t9 handed to Ada".to_owned());

        let values = values_in(&entry, "Ada", at_istanbul("2026-09-15", "09:05"), &istanbul());
        assert_eq!(
            expand(&entry.brief, &values),
            "Tuesday 2026-09-15 at 09:05: morning triage for Ada, last 2026-09-14 09:00 (card t9 handed to Ada) — keep {json}"
        );

        entry.last_run = 0;
        entry.last_result = None;
        let values = values_in(&entry, "Ada", at_istanbul("2026-09-15", "09:05"), &istanbul());
        assert_eq!(expand("{last_run} / {last_result}", &values), "never / nothing yet");
    }

    #[test]
    fn every_template_is_a_schedule_the_core_would_keep() {
        for template in templates() {
            assert!(settle_schedule(template.schedule.clone()).is_ok(), "{}", template.id);
            assert!(template.brief.contains('{'), "{} teaches a variable", template.id);
        }
    }

    #[test]
    fn a_routine_an_agent_proposes_starts_paused() {
        let routines = store("agentland-routines-proposed");

        let proposed = routines
            .create(
                CreateRoutine { created_by: Some("x".to_owned()), enabled: Some(true), ..ask("sweep") },
                100,
                true,
            )
            .expect("create");

        assert!(!proposed.enabled, "an agent does not decide what the crew spends");
        assert_eq!(proposed.created_by.as_deref(), Some("x"));
        assert_eq!(proposed.next_run, None);
    }

    #[test]
    fn two_failures_in_a_row_pause_the_routine() {
        let routines = store("agentland-routines-test");
        let created = routines.create(ask("morning triage"), 50, false).expect("create");

        let after_one = routines
            .record(&created.id, 100, Outcome::Failed("agent missing".to_owned()))
            .expect("record");
        assert!(after_one.enabled, "one failure should not pause it");

        let after_two = routines
            .record(&created.id, 200, Outcome::Failed("agent missing".to_owned()))
            .expect("record");
        assert!(!after_two.enabled, "two failures should pause it");
        assert!(after_two.last_result.unwrap().contains("paused after 2 failures"));
        assert_eq!(after_two.history.len(), 2);
        assert_eq!(after_two.history[0].at, 200, "newest first");
    }

    #[test]
    fn the_failure_count_that_pauses_is_the_routines_own() {
        let routines = store("agentland-routines-patience");
        let created = routines
            .create(CreateRoutine { pause_after_failures: Some(3), ..ask("patient") }, 50, false)
            .expect("create");

        routines.record(&created.id, 100, Outcome::Failed("boom".to_owned()));
        let after_two = routines
            .record(&created.id, 200, Outcome::Failed("boom".to_owned()))
            .expect("record");
        assert!(after_two.enabled);

        let after_three = routines
            .record(&created.id, 300, Outcome::Failed("boom".to_owned()))
            .expect("record");
        assert!(!after_three.enabled);
    }

    #[test]
    fn a_skip_is_not_a_failure() {
        let routines = store("agentland-routines-skip");
        let created = routines.create(ask("tight week"), 50, false).expect("create");

        for at in [100, 200, 300] {
            routines.record(&created.id, at, Outcome::Skipped("the week is tight".to_owned()));
        }

        let held = routines.get(&created.id).expect("still there");
        assert!(held.enabled);
        assert_eq!(held.consecutive_failures, 0);
        assert_eq!(held.history.len(), 3);
    }

    #[test]
    fn a_success_clears_the_failure_streak() {
        let routines = store("agentland-routines-success");
        let created = routines.create(ask("recap"), 50, false).expect("create");

        routines.record(&created.id, 100, Outcome::Failed("boom".to_owned()));
        let recovered = routines
            .record(
                &created.id,
                200,
                Outcome::Ran { detail: "card t7 created".to_owned(), card: Some("t7".to_owned()) },
            )
            .expect("record");

        assert_eq!(recovered.consecutive_failures, 0);
        assert!(recovered.enabled);
        assert_eq!(recovered.last_card.as_deref(), Some("t7"));
    }

    #[test]
    fn a_busy_agent_is_waited_for_then_let_go_as_a_skip() {
        let routines = store("agentland-routines-busy");
        let created = routines.create(ask("sweep"), 50, false).expect("create");

        let waiting = routines.defer(&created.id, 1_000, "the agent is mid-turn").expect("defer");
        assert_eq!(waiting.waiting_since, 1_000);
        assert_eq!(waiting.last_run, 0, "still due");
        assert!(waiting.history.is_empty());

        let still = routines.defer(&created.id, 1_000 + BUSY_GRACE - 1, "the agent is mid-turn").expect("defer");
        assert_eq!(still.waiting_since, 1_000);

        let gone = routines.defer(&created.id, 1_000 + BUSY_GRACE, "the agent is mid-turn").expect("defer");
        assert_eq!(gone.waiting_since, 0);
        assert_eq!(gone.last_run, 1_000 + BUSY_GRACE);
        assert_eq!(gone.consecutive_failures, 0);
        assert_eq!(gone.history[0].outcome, RunOutcome::Skipped);
        assert!(gone.history[0].detail.contains("for 30 min"));
    }

    #[test]
    fn an_update_touches_only_what_it_names() {
        let routines = store("agentland-routines-update");
        let created = routines.create(ask("triage"), 50, false).expect("create");

        let changed = routines
            .update(
                &created.id,
                UpdateRoutine {
                    schedule: Some(Schedule::Daily { times: vec!["9:00".to_owned()], days: vec![Day::Mon] }),
                    delivery: Some(Delivery::Pane),
                    ..Default::default()
                },
                100,
            )
            .expect("update");

        assert_eq!(changed.name, "triage");
        assert_eq!(changed.delivery, Delivery::Pane);
        assert_eq!(
            changed.schedule,
            Schedule::Daily { times: vec!["09:00".to_owned()], days: vec![Day::Mon] }
        );

        assert!(routines
            .update(&created.id, UpdateRoutine { brief: Some("  ".to_owned()), ..Default::default() }, 100)
            .is_err());
    }
}
