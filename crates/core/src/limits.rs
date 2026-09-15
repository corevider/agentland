//! When an engine says the account has run out, and when it will be back.
//!
//! A turn that hits a usage limit does not fail loudly. The engine prints one
//! line, ends the turn and waits at its prompt — and to everything else in this
//! app a pane at rest is an agent that finished. Nobody comes back to it when
//! the window resets, so a crew left working overnight stops at the first wall
//! and stays stopped until a person types "continue" by hand in every pane.
//!
//! This reads that line, works out when the wall comes down, and holds the agent
//! until then. The supervisor's tick tells it to carry on once it has.

use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

/// What an agent is told once its limit has reset — the words the person used
/// to type by hand, every time, in every pane.
pub const CARRY_ON: &str = "I hit my usage limit while you were working, but it has reset now. Please continue from where you left off.";

/// Time after the stated reset before the agent is told. A window the engine
/// says resets at three has not always rolled over at three on the provider's
/// side, and being told too early earns nothing but the same line again.
pub const GRACE: u64 = 2 * 60;

/// How long to wait when the engine said it was limited and not until when.
/// Asking again costs nothing while the account is still limited: the engine
/// answers with the same line at once, and this time it usually says when.
pub const UNKNOWN_WAIT: u64 = 30 * 60;

/// How long a told agent gets to start its turn before it is told again.
pub const RETRY: u64 = 15 * 60;

/// How many times one limit is answered before a person is asked instead. A
/// pane that swallows the words six times running has something else wrong
/// with it, and typing into it all night would bury whatever that is.
pub const MOST_ATTEMPTS: u32 = 6;

/// How long a line already answered is recognised as the old one rather than
/// a new limit. Longer than a session window, so a line left on the screen
/// is never mistaken for a fresh wall; short enough that a reading that went
/// wrong stops an agent for an evening, not for good.
const REMEMBERED_FOR: u64 = 6 * 60 * 60;

/// Only the bottom of the pane: a line further up is history, not the state
/// the engine is in now.
const LOOK_BACK: usize = 20;

/// A reset said as a time of day with no date is at most one session window
/// away. Further than this and the time has already been, today.
const LONGEST_WINDOW: i64 = 5 * 60 * 60 + 15 * 60;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Window {
    Session,
    Weekly,
    Unknown,
}

impl Window {
    pub fn in_words(self) -> &'static str {
        match self {
            Window::Session => "session limit",
            Window::Weekly => "weekly limit",
            Window::Unknown => "usage limit",
        }
    }
}

/// What a pane says about having run out.
#[derive(Clone, Debug, PartialEq)]
pub struct Limit {
    pub window: Window,
    /// When the engine says the limit resets, as a unix time, or `None` when
    /// it did not say.
    pub resets_at: Option<u64>,
    /// The line as the engine printed it, trimmed of the glyphs around it.
    pub said: String,
}

/// Read a usage limit off the bottom of a pane.
///
/// The match is on how a line *starts*, after the glyphs an engine draws in
/// front of its own messages: `You've hit your session limit · resets 3pm`,
/// `5-hour limit reached ∙ resets 3pm`, `■ You've hit your usage limit … try
/// again at 3:45 PM`. A sentence that merely mentions a limit — an agent
/// reading this file, a test printing its fixtures, the words an agent is told
/// once one resets — starts some other way and is left alone.
///
/// A line that has been answered is not a limit any more: when a message sits
/// between it and the composer, somebody already said something after it.
pub fn read_limit<Tz: TimeZone>(frame: &str, now: &DateTime<Tz>) -> Option<Limit> {
    let plain = crate::context::strip_escapes(frame);
    let lines: Vec<&str> = plain
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let window = &lines[lines.len().saturating_sub(LOOK_BACK)..];

    let at = window.iter().rposition(|line| starts_a_limit(&bare(line).to_lowercase()))?;

    let answered = window[at + 1..].iter().filter(|line| is_a_prompt(line)).count() >= 2;
    if answered {
        return None;
    }

    let said = bare(window[at]).to_owned();
    let following: Vec<&str> = window[at + 1..]
        .iter()
        .take(2)
        .copied()
        .filter(|line| !is_a_prompt(line))
        .collect();
    let joined = std::iter::once(said.as_str())
        .chain(following)
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();

    Some(Limit {
        window: window_of(&joined),
        resets_at: resets_at(&joined, now),
        said,
    })
}

/// A line without the glyphs an engine draws in front of what it says.
fn bare(line: &str) -> &str {
    line.trim_start_matches(|c: char| {
        c.is_whitespace() || matches!(c, '⎿' | '●' | '■' | '│' | '•' | '⚠' | '✗' | '×' | '*' | '!' | '·' | '└' | '╰')
    })
    .trim()
}

fn starts_a_limit(lowered: &str) -> bool {
    const OPENINGS: [&str; 6] = [
        "you've hit your",
        "you’ve hit your",
        "you have hit your",
        "you've reached your",
        "you’ve reached your",
        "you have reached your",
    ];

    if OPENINGS.iter().any(|opening| lowered.starts_with(opening)) {
        return lowered.contains("limit");
    }

    // `5-hour limit reached`, `Weekly limit reached`, `Claude AI usage limit
    // reached|1789500000`: a few plain words, then the phrase.
    match lowered.find("limit reached") {
        Some(at) if at <= 32 => lowered[..at]
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == ' '),
        _ => false,
    }
}

/// A line the engine draws where a person types, or a message that was typed
/// there and sent.
fn is_a_prompt(line: &str) -> bool {
    let trimmed = line.trim_start_matches('│').trim_start();
    trimmed.starts_with('>') || trimmed.starts_with('❯') || trimmed.starts_with('›')
}

fn window_of(lowered: &str) -> Window {
    if lowered.contains("week") {
        Window::Weekly
    } else if lowered.contains("session") || lowered.contains("5-hour") || lowered.contains("5 hour") {
        Window::Session
    } else {
        Window::Unknown
    }
}

/// When the limit resets, from however the engine chose to say it.
fn resets_at<Tz: TimeZone>(lowered: &str, now: &DateTime<Tz>) -> Option<u64> {
    if let Some((_, after)) = lowered.split_once("limit reached|") {
        let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(stamp) = digits.parse::<u64>() {
            return Some(stamp);
        }
    }

    const SAYINGS: [&str; 7] = [
        "try again in",
        "resets in",
        "try again at",
        "reset at",
        "resets at",
        "resets",
        "available again at",
    ];

    let rest = SAYINGS.iter().find_map(|saying| {
        lowered
            .find(saying)
            .map(|at| (saying.ends_with(" in"), &lowered[at + saying.len()..]))
    });

    let (relative, rest) = rest?;
    let rest = rest.trim_start();

    if relative || rest.starts_with("in ") {
        let seconds = duration_in(rest.trim_start_matches("in "))?;
        return Some(now.timestamp().max(0) as u64 + seconds);
    }

    moment_in(rest, now)
}

/// `2 days 3 hours 5 minutes`, `4hr 10m`, `2h5m`, `45 minutes`.
fn duration_in(text: &str) -> Option<u64> {
    let mut total = 0u64;
    let mut found = false;
    let mut characters = text.chars().peekable();

    loop {
        while characters.peek().is_some_and(|c| c.is_whitespace() || *c == ',') {
            characters.next();
        }

        let digits: String = std::iter::from_fn(|| characters.next_if(char::is_ascii_digit)).collect();
        if digits.is_empty() {
            break;
        }

        while characters.peek().is_some_and(|c| c.is_whitespace()) {
            characters.next();
        }

        let unit: String = std::iter::from_fn(|| characters.next_if(char::is_ascii_alphabetic)).collect();
        let each = match unit.as_str() {
            "d" | "day" | "days" => 86_400,
            "h" | "hr" | "hrs" | "hour" | "hours" => 3_600,
            "m" | "min" | "mins" | "minute" | "minutes" => 60,
            "s" | "sec" | "secs" | "second" | "seconds" => 1,
            _ => break,
        };

        total += digits.parse::<u64>().ok()? * each;
        found = true;
    }

    found.then_some(total)
}

/// `3pm`, `3:30 pm`, `15:00`, `sep 18, 10am`, `sep 16th, 2026 3:45 pm`,
/// `mon 10am` — read in the machine's own time, which is the one the engine
/// prints.
fn moment_in<Tz: TimeZone>(text: &str, now: &DateTime<Tz>) -> Option<u64> {
    let words: Vec<String> = text
        .split(|c: char| c.is_whitespace() || c == ',')
        .map(|word| word.trim_matches(|c: char| matches!(c, '.' | '(' | ')' | '∙' | '·')).to_owned())
        .filter(|word| !word.is_empty())
        .take(6)
        .collect();

    let mut month = None;
    let mut day = None;
    let mut year = None;
    let mut weekday = None;
    let mut clock = None;

    let mut index = 0;
    while index < words.len() {
        let word = words[index].as_str();
        let next = words.get(index + 1).map(String::as_str);

        if let Some(found) = month_of(word) {
            month = Some(found);
        } else if let Some(found) = weekday_of(word) {
            weekday = Some(found);
        } else if month.is_some() && day.is_none() && ordinal(word).is_some() {
            day = ordinal(word);
        } else if word.len() == 4 && word.chars().all(|c| c.is_ascii_digit()) {
            year = word.parse::<i32>().ok();
        } else if let Some((found, used_next)) = clock_of(word, next) {
            clock = Some(found);
            if used_next {
                index += 1;
            }
        } else if word != "at" && word != "on" {
            break;
        }

        index += 1;
    }

    let clock = clock.unwrap_or(NaiveTime::MIN);
    let zone = now.timezone();
    let today = now.date_naive();
    let local = |date: NaiveDate| {
        zone.from_local_datetime(&NaiveDateTime::new(date, clock))
            .earliest()
            .map(|moment| moment.timestamp().max(0) as u64)
    };
    let now_secs = now.timestamp().max(0) as u64;

    if let (Some(month), Some(day)) = (month, day) {
        let mut date = NaiveDate::from_ymd_opt(year.unwrap_or(today.year()), month, day)?;
        if year.is_none() && date < today - Duration::days(1) {
            date = NaiveDate::from_ymd_opt(today.year() + 1, month, day)?;
        }
        return local(date);
    }

    if let Some(weekday) = weekday {
        let mut date = today;
        for _ in 0..8 {
            if date.weekday() == weekday && local(date).is_some_and(|at| at > now_secs) {
                return local(date);
            }
            date += Duration::days(1);
        }
        return None;
    }

    let today_at = local(today)?;
    if today_at > now_secs {
        return Some(today_at);
    }

    let tomorrow_at = local(today + Duration::days(1))?;
    if (tomorrow_at - now_secs) as i64 <= LONGEST_WINDOW {
        Some(tomorrow_at)
    } else {
        Some(today_at)
    }
}

fn month_of(word: &str) -> Option<u32> {
    const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
    if word.len() < 3 || !word.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    MONTHS
        .iter()
        .position(|month| word.starts_with(month))
        .map(|at| at as u32 + 1)
}

fn weekday_of(word: &str) -> Option<chrono::Weekday> {
    use chrono::Weekday::*;
    const DAYS: [(&str, chrono::Weekday); 7] =
        [("mon", Mon), ("tue", Tue), ("wed", Wed), ("thu", Thu), ("fri", Fri), ("sat", Sat), ("sun", Sun)];
    if word.len() < 3 || !word.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    DAYS.iter().find(|(name, _)| word.starts_with(name)).map(|(_, day)| *day)
}

fn ordinal(word: &str) -> Option<u32> {
    let digits = word.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    let suffix = &word[digits.len()..];
    if digits.is_empty() || digits.len() > 2 || !matches!(suffix, "" | "st" | "nd" | "rd" | "th") {
        return None;
    }
    digits.parse().ok().filter(|day| (1..=31).contains(day))
}

/// A time of day, and whether it took the next word (`3:45 pm`) to say it. A
/// bare number is not a time — `18` is as likely the day of a month.
fn clock_of(word: &str, next: Option<&str>) -> Option<(NaiveTime, bool)> {
    let (body, meridiem, used_next) = if let Some(body) = word.strip_suffix("am") {
        (body, Some(false), false)
    } else if let Some(body) = word.strip_suffix("pm") {
        (body, Some(true), false)
    } else {
        match next {
            Some("am") | Some("a.m") => (word, Some(false), true),
            Some("pm") | Some("p.m") => (word, Some(true), true),
            _ => (word, None, false),
        }
    };

    let (hours, minutes) = match body.split_once(':') {
        Some((hours, minutes)) => (hours.parse::<u32>().ok()?, minutes.parse::<u32>().ok()?),
        None if meridiem.is_some() => (body.parse::<u32>().ok()?, 0),
        None => return None,
    };

    let hours = match meridiem {
        Some(afternoon) if (1..=12).contains(&hours) => (hours % 12) + if afternoon { 12 } else { 0 },
        Some(_) => return None,
        None => hours,
    };

    NaiveTime::from_hms_opt(hours, minutes, 0).map(|time| (time, used_next))
}

/// An agent waiting for its limit to reset.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Hold {
    pub agent_id: String,
    /// The allowance it spends from: the whole login is out, not only this
    /// agent, so nothing else is started on it until the reset either.
    pub identity: String,
    pub window: Window,
    /// When it is told to carry on: the stated reset, plus the grace.
    pub resets_at: u64,
    /// Whether the engine said when, or the wait is a guess.
    #[serde(default)]
    pub stated: bool,
    pub said: String,
    pub noticed_at: u64,
    #[serde(default)]
    pub told_at: Option<u64>,
    #[serde(default)]
    pub attempts: u32,
}

/// What noticing a limit changed.
#[derive(Clone, Debug, PartialEq)]
pub enum Noticed {
    /// A new wall.
    New(Hold),
    /// Told to carry on, and stopped again at a later reset.
    Again(Hold),
    /// What was already known.
    Same,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct State {
    #[serde(default)]
    holds: BTreeMap<String, Hold>,
    /// The last line each agent was held on after it was let go, and when, so
    /// a line still on the screen is not taken for a new one.
    #[serde(default)]
    answered: BTreeMap<String, (String, u64)>,
}

pub struct Limits {
    state: Mutex<State>,
    data_dir: PathBuf,
}

impl Limits {
    pub fn new(data_dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&data_dir);
        let data_dir = crate::exec::settled(&data_dir);
        let state = crate::db::load_state(&data_dir, "limits");

        Self {
            state: Mutex::new(state),
            data_dir,
        }
    }

    fn persist(&self, state: &State) {
        crate::db::save_state(&self.data_dir, "limits", state);
    }

    pub fn list(&self) -> Vec<Hold> {
        self.state.lock().holds.values().cloned().collect()
    }

    pub fn held(&self, agent_id: &str) -> Option<Hold> {
        self.state.lock().holds.get(agent_id).cloned()
    }

    pub fn is_held(&self, agent_id: &str) -> bool {
        self.state.lock().holds.contains_key(agent_id)
    }

    /// Whether this allowance is out right now: an agent spending from it is
    /// waiting for a reset that has not come.
    pub fn spent(&self, identity: &str, now: u64) -> bool {
        self.state
            .lock()
            .holds
            .values()
            .any(|hold| hold.identity == identity && hold.resets_at > now)
    }

    /// Take in what a pane says about its limit.
    pub fn notice(&self, agent_id: &str, identity: &str, limit: &Limit, now: u64) -> Noticed {
        let mut state = self.state.lock();

        if let Some((said, at)) = state.answered.get(agent_id) {
            if *said == limit.said && now.saturating_sub(*at) < REMEMBERED_FOR {
                return Noticed::Same;
            }
        }

        let resets_at = limit.resets_at.map(|at| at.max(now)).unwrap_or(now + UNKNOWN_WAIT) + GRACE;
        let fresh = Hold {
            agent_id: agent_id.to_owned(),
            identity: identity.to_owned(),
            window: limit.window,
            resets_at,
            stated: limit.resets_at.is_some(),
            said: limit.said.clone(),
            noticed_at: now,
            told_at: None,
            attempts: 0,
        };

        let changed = match state.holds.get(agent_id) {
            None => Noticed::New(fresh.clone()),
            Some(held) => {
                let told_and_settled = held.told_at.is_some_and(|told| now >= told + GRACE);
                let moved = held.said != limit.said || resets_at > held.resets_at + GRACE;

                if !(told_and_settled && moved) {
                    return Noticed::Same;
                }

                Noticed::Again(Hold {
                    attempts: held.attempts,
                    ..fresh.clone()
                })
            }
        };

        let kept = match &changed {
            Noticed::New(hold) | Noticed::Again(hold) => hold.clone(),
            Noticed::Same => fresh,
        };
        state.answered.remove(agent_id);
        state.holds.insert(agent_id.to_owned(), kept);
        self.persist(&state);
        changed
    }

    /// The holds whose reset has come and that are owed a word: never told,
    /// or told long enough ago that the words evidently did not land.
    pub fn due(&self, now: u64) -> Vec<Hold> {
        self.state
            .lock()
            .holds
            .values()
            .filter(|hold| hold.resets_at <= now)
            .filter(|hold| hold.told_at.is_none_or(|told| now >= told + RETRY))
            .cloned()
            .collect()
    }

    pub fn told(&self, agent_id: &str, now: u64) -> Option<Hold> {
        let mut state = self.state.lock();
        let hold = state.holds.get_mut(agent_id)?;
        hold.told_at = Some(now);
        hold.attempts += 1;
        let updated = hold.clone();
        self.persist(&state);
        Some(updated)
    }

    /// Let an agent go: it is working again, a person stepped in, or it has
    /// been told as many times as is worth telling it.
    pub fn release(&self, agent_id: &str, now: u64) -> Option<Hold> {
        let mut state = self.state.lock();
        let hold = state.holds.remove(agent_id)?;
        state.answered.insert(agent_id.to_owned(), (hold.said.clone(), now));
        self.persist(&state);
        Some(hold)
    }

    /// Forget an agent entirely, when it leaves the crew.
    pub fn forget(&self, agent_id: &str) {
        let mut state = self.state.lock();
        let had = state.holds.remove(agent_id).is_some() | state.answered.remove(agent_id).is_some();
        if had {
            self.persist(&state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    fn istanbul(text: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(text).expect("a time")
    }

    fn at(text: &str) -> u64 {
        istanbul(text).timestamp() as u64
    }

    /// A Claude pane at rest after the line: the transcript, the composer with
    /// its placeholder, the status lines under it.
    fn claude_pane(line: &str) -> String {
        format!(
            "● Running the migration tests now.\n\n  ⎿  {line}\n     /upgrade to increase your usage limit.\n\n╭──────────────╮\n│ > Try \"fix lint errors\" │\n╰──────────────╯\n  Model: Opus 5 | Ctx: 95.7k\n  ? for shortcuts\n"
        )
    }

    #[test]
    fn a_session_limit_resets_at_the_time_it_names_today() {
        let now = istanbul("2026-09-15T12:10:00+03:00");
        let limit = read_limit(&claude_pane("You've hit your session limit · resets 3pm (Europe/Istanbul)"), &now)
            .expect("the pane says so");

        assert_eq!(limit.window, Window::Session);
        assert_eq!(limit.resets_at, Some(at("2026-09-15T15:00:00+03:00")));
        assert_eq!(limit.said, "You've hit your session limit · resets 3pm (Europe/Istanbul)");
    }

    #[test]
    fn the_older_wordings_are_read_too() {
        let now = istanbul("2026-09-15T12:10:00+03:00");

        let hourly = read_limit(&claude_pane("5-hour limit reached ∙ resets 3:30pm"), &now).expect("read");
        assert_eq!(hourly.window, Window::Session);
        assert_eq!(hourly.resets_at, Some(at("2026-09-15T15:30:00+03:00")));

        let older = read_limit(
            &claude_pane("Claude usage limit reached. Your limit will reset at 5pm (Europe/Istanbul)."),
            &now,
        )
        .expect("read");
        assert_eq!(older.resets_at, Some(at("2026-09-15T17:00:00+03:00")));

        let stamped = read_limit(&claude_pane("Claude AI usage limit reached|1789500000"), &now).expect("read");
        assert_eq!(stamped.resets_at, Some(1_789_500_000));
    }

    #[test]
    fn a_weekly_limit_resets_on_the_day_it_names() {
        let now = istanbul("2026-09-15T12:10:00+03:00");

        let dated = read_limit(&claude_pane("Weekly limit reached ∙ resets Sep 18, 10am"), &now).expect("read");
        assert_eq!(dated.window, Window::Weekly);
        assert_eq!(dated.resets_at, Some(at("2026-09-18T10:00:00+03:00")));

        let spelled = read_limit(
            &claude_pane("You've hit your weekly limit · resets Sep 18 at 10am (Europe/Istanbul)"),
            &now,
        )
        .expect("read");
        assert_eq!(spelled.resets_at, Some(at("2026-09-18T10:00:00+03:00")));

        let by_day = read_limit(&claude_pane("You've hit your weekly limit · resets Fri 9am"), &now).expect("read");
        assert_eq!(by_day.resets_at, Some(at("2026-09-18T09:00:00+03:00")));
    }

    #[test]
    fn codex_is_read_whether_it_names_a_moment_or_a_wait() {
        let now = istanbul("2026-09-15T12:10:00+03:00");

        let dated = "■ You've hit your usage limit. Upgrade to Pro (https://openai.com/chatgpt/pricing), visit https://chatgpt.com/codex/settings/usage to purchase more credits or try again at Sep 16th, 2026 3:45 PM.\n\n› Ask Codex to do anything\n";
        let limit = read_limit(dated, &now).expect("read");
        assert_eq!(limit.resets_at, Some(at("2026-09-16T15:45:00+03:00")));

        let wrapped = "■ You've hit your usage limit. Upgrade to Pro or try again\nat 3:45 PM.\n\n› Ask Codex to do anything\n";
        assert_eq!(
            read_limit(wrapped, &now).expect("read").resets_at,
            Some(at("2026-09-15T15:45:00+03:00"))
        );

        let waiting = "■ You've hit your usage limit. Try again in 2 days 3 hours 5 minutes.\n› \n";
        assert_eq!(
            read_limit(waiting, &now).expect("read").resets_at,
            Some(now.timestamp() as u64 + 2 * 86_400 + 3 * 3_600 + 5 * 60)
        );
    }

    #[test]
    fn a_limit_that_says_nothing_about_when_is_still_a_limit() {
        let now = istanbul("2026-09-15T12:10:00+03:00");
        let limit = read_limit(&claude_pane("You've hit your limit"), &now).expect("read");

        assert_eq!(limit.window, Window::Unknown);
        assert_eq!(limit.resets_at, None);
    }

    #[test]
    fn a_time_just_gone_is_now_and_a_time_long_gone_is_not_tomorrow() {
        // Seen a few minutes after three: the reset has come.
        let just_after = istanbul("2026-09-15T15:20:00+03:00");
        assert_eq!(
            read_limit(&claude_pane("You've hit your session limit · resets 3pm"), &just_after)
                .expect("read")
                .resets_at,
            Some(at("2026-09-15T15:00:00+03:00"))
        );

        // Seen hours later — the app was closed — it is still gone, not
        // tomorrow: a session window is never a day long.
        let evening = istanbul("2026-09-15T21:00:00+03:00");
        assert_eq!(
            read_limit(&claude_pane("You've hit your session limit · resets 3pm"), &evening)
                .expect("read")
                .resets_at,
            Some(at("2026-09-15T15:00:00+03:00"))
        );

        // Seen late at night about one in the morning: that is tomorrow.
        let late = istanbul("2026-09-15T23:00:00+03:00");
        assert_eq!(
            read_limit(&claude_pane("You've hit your session limit · resets 1am"), &late)
                .expect("read")
                .resets_at,
            Some(at("2026-09-16T01:00:00+03:00"))
        );
    }

    #[test]
    fn prose_about_a_limit_is_not_a_limit() {
        let now = istanbul("2026-09-15T12:10:00+03:00");

        let source = "    const LINE: &str = \"You've hit your session limit · resets 3pm\";\n> \n";
        assert_eq!(read_limit(source, &now), None);

        let chatter = "● It looks like you've hit your limit on open files.\n> \n";
        assert_eq!(read_limit(chatter, &now), None);

        let told = format!("> {CARRY_ON}\n● Picking the migration back up.\n> \n");
        assert_eq!(read_limit(&told, &now), None);
    }

    #[test]
    fn a_limit_somebody_already_answered_is_history() {
        let now = istanbul("2026-09-15T15:10:00+03:00");
        let answered = "  ⎿  You've hit your session limit · resets 3pm\n\n> continue\n\n● Done — the tests pass.\n\n╭────╮\n│ >  │\n╰────╯\n";

        assert_eq!(read_limit(answered, &now), None);
    }

    #[test]
    fn a_limit_scrolled_far_up_the_pane_is_history() {
        let now = istanbul("2026-09-15T15:10:00+03:00");
        let mut pane = String::from("  ⎿  You've hit your session limit · resets 3pm\n");
        for step in 0..30 {
            pane.push_str(&format!("● step {step}\n"));
        }

        assert_eq!(read_limit(&pane, &now), None);
    }

    fn scratch(name: &str) -> Limits {
        let dir = std::env::temp_dir().join(format!("agentland-limits-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Limits::new(dir)
    }

    fn limit(said: &str, resets_at: Option<u64>) -> Limit {
        Limit {
            window: Window::Session,
            resets_at,
            said: said.to_owned(),
        }
    }

    #[test]
    fn an_agent_is_held_until_the_reset_and_a_little_after() {
        let limits = scratch("held");
        let line = limit("You've hit your session limit · resets 3pm", Some(10_000));

        let Noticed::New(hold) = limits.notice("ada", "claude", &line, 1_000) else {
            panic!("a new limit");
        };
        assert_eq!(hold.resets_at, 10_000 + GRACE);
        assert!(limits.spent("claude", 5_000), "the login is out until the reset");
        assert!(!limits.spent("claude/second", 5_000), "another login is not");

        assert!(limits.due(10_000).is_empty(), "not before the grace");
        assert_eq!(limits.due(10_000 + GRACE).len(), 1);

        assert_eq!(limits.notice("ada", "claude", &line, 2_000), Noticed::Same, "the same line again");
    }

    #[test]
    fn a_limit_that_did_not_say_when_is_asked_about_again_later() {
        let limits = scratch("unstated");
        let Noticed::New(hold) = limits.notice("ada", "claude", &limit("You've hit your limit", None), 1_000)
        else {
            panic!("a new limit");
        };

        assert_eq!(hold.resets_at, 1_000 + UNKNOWN_WAIT + GRACE);
        assert!(!hold.stated);
    }

    #[test]
    fn told_and_not_taken_is_told_again_and_then_left_to_a_person() {
        let limits = scratch("retry");
        limits.notice("ada", "claude", &limit("line", Some(1_000)), 500);

        let due_at = 1_000 + GRACE;
        limits.told("ada", due_at);
        assert!(limits.due(due_at + RETRY - 1).is_empty(), "given time to start its turn");
        assert_eq!(limits.due(due_at + RETRY).len(), 1, "told again when nothing happened");

        for attempt in 2..=MOST_ATTEMPTS {
            let told = limits.told("ada", due_at + RETRY * attempt as u64).expect("held");
            assert_eq!(told.attempts, attempt);
        }
    }

    #[test]
    fn stopped_again_at_a_later_reset_is_a_new_wait() {
        let limits = scratch("again");
        limits.notice("ada", "claude", &limit("resets 3pm", Some(1_000)), 500);
        limits.told("ada", 1_200);

        let later = limit("resets 8pm", Some(20_000));
        let Noticed::Again(hold) = limits.notice("ada", "claude", &later, 1_200 + GRACE) else {
            panic!("a later wall");
        };
        assert_eq!(hold.resets_at, 20_000 + GRACE);
        assert_eq!(hold.told_at, None);
        assert_eq!(hold.attempts, 1, "the tries so far are kept");
    }

    #[test]
    fn a_line_left_on_the_screen_after_carrying_on_is_not_a_new_wall() {
        let limits = scratch("answered");
        let line = limit("You've hit your session limit · resets 3pm", Some(1_000));
        limits.notice("ada", "claude", &line, 500);
        limits.told("ada", 1_200);
        limits.release("ada", 1_300);

        assert_eq!(limits.notice("ada", "claude", &line, 1_400), Noticed::Same);
        assert!(!limits.is_held("ada"));

        let tomorrow = 1_300 + REMEMBERED_FOR;
        assert!(matches!(limits.notice("ada", "claude", &line, tomorrow), Noticed::New(_)));
    }

    #[test]
    fn holds_survive_a_restart() {
        let dir = std::env::temp_dir().join(format!("agentland-limits-restart-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        Limits::new(dir.clone()).notice("ada", "claude", &limit("line", Some(9_000)), 100);
        let reopened = Limits::new(dir);

        assert_eq!(reopened.held("ada").map(|hold| hold.resets_at), Some(9_000 + GRACE));
    }
}
