use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchState {
    Working,
    Settled,
    Abandoned,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Watch {
    pub id: String,
    pub plan_id: String,
    pub step_id: String,
    pub task_id: String,
    pub agent_id: String,
    pub session_id: String,
    pub repository_id: String,
    pub worktree: String,
    /// A short piece of the brief, used to prove it ever reached the pane.
    pub fingerprint: String,
    #[serde(default)]
    pub delivered: bool,
    #[serde(default)]
    pub resends: u32,
    pub state: WatchState,
    pub started_at: u64,
    #[serde(default)]
    pub settled_at: u64,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub told_leader: bool,
    #[serde(default)]
    pub wake_attempts: u32,
    #[serde(default)]
    pub last_wake: u64,
    #[serde(default)]
    pub reaped: bool,
}

/// What the core can see about a watched agent at one moment.
#[derive(Clone, Debug, Default)]
pub struct Observation {
    pub session_alive: bool,
    /// The pane is stable, no turn is running and the composer is empty — the
    /// engine is waiting rather than working. Byte-idleness cannot say this: a
    /// TUI redraws its status line forever, so `last_output_at` never settles.
    pub quiet_turn: bool,
    pub idle_seconds: u64,
    pub tail: String,
    pub changed_files: usize,
    pub card_has_evidence: bool,
    /// What the engine's own transcript says about the brief arriving.
    /// `None` when the engine keeps no transcript to consult.
    pub transcript_says: Option<bool>,
    pub age_seconds: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct Rules {
    pub delivery_grace: u64,
    pub idle_before_finished: u64,
    pub max_resends: u32,
    pub wake_backoff: u64,
    pub max_wakes: u32,
    /// How long a settled agent may sit at its prompt before the pane is taken back.
    pub reap_after: u64,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            delivery_grace: 45,
            idle_before_finished: 90,
            max_resends: 2,
            wake_backoff: 60,
            max_wakes: 5,
            reap_after: 45,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Verdict {
    Working,
    Resend,
    Finished(String),
    LostIt(String),
}

/// The whole judgement, with no I/O in it.
pub fn judge(watch: &Watch, seen: &Observation, rules: &Rules) -> Verdict {
    if watch.state != WatchState::Working {
        return Verdict::Working;
    }

    if let Some(marker) = done_marker(&seen.tail, &watch.step_id) {
        return Verdict::Finished(marker);
    }

    if !watch.delivered && !delivered_now(seen, &watch.fingerprint) {
        if seen.age_seconds < rules.delivery_grace {
            return Verdict::Working;
        }

        return if watch.resends >= rules.max_resends {
            Verdict::LostIt(format!(
                "the brief never reached {} after {} attempts",
                watch.agent_id,
                watch.resends + 1
            ))
        } else {
            Verdict::Resend
        };
    }

    if !seen.session_alive {
        return Verdict::Finished(if seen.changed_files > 0 {
            format!(
                "{} finished and left {} changed file(s)",
                watch.agent_id, seen.changed_files
            )
        } else {
            format!("{} stopped without changing anything", watch.agent_id)
        });
    }

    if seen.card_has_evidence {
        return Verdict::Finished(format!("{} attached evidence to {}", watch.agent_id, watch.task_id));
    }

    if seen.changed_files > 0 {
        if seen.quiet_turn {
            return Verdict::Finished(format!(
                "{} is waiting at an empty prompt with {} changed file(s)",
                watch.agent_id, seen.changed_files
            ));
        }

        if seen.idle_seconds >= rules.idle_before_finished {
            return Verdict::Finished(format!(
                "{} has written nothing for {}s with {} changed file(s)",
                watch.agent_id, seen.idle_seconds, seen.changed_files
            ));
        }
    }

    Verdict::Working
}

/// Whether the brief actually reached the engine.
///
/// The transcript wins when there is one. A pane can show a message that never
/// arrived — text left sitting in a composer that was never submitted, or a
/// resume picker that swallowed it — and believing the screen is how a step
/// waits forever on an agent that was never asked.
fn delivered_now(seen: &Observation, fingerprint: &str) -> bool {
    let needle = fingerprint.trim();
    if needle.is_empty() {
        return true;
    }

    match seen.transcript_says {
        Some(told) => told,
        None => squash(&seen.tail).contains(&squash(needle)),
    }
}

fn done_marker(tail: &str, step_id: &str) -> Option<String> {
    let wanted = format!("done:{}", step_id.to_lowercase());
    squash(tail)
        .contains(&squash(&wanted))
        .then(|| format!("the pane printed DONE:{step_id}"))
}

/// Terminals wrap, pad and redraw; comparing squashed text is what survives that.
fn squash(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_whitespace())
        .flat_map(|character| character.to_lowercase())
        .collect()
}

/// Whether a settled agent's pane should be handed back.
///
/// A finished agent does not exit: the engine sits at its prompt holding a
/// worktree and a slot under the caps, so the next step cannot start. Taking the
/// pane back is only safe when the work is settled, the engine is not mid-turn,
/// nobody has typed into it, and it has not been given something new.
pub fn should_reap(watch: &Watch, seen: &Observation, rules: &Rules, busy_with_new_work: bool, now: u64) -> bool {
    if watch.state == WatchState::Working || watch.reaped {
        return false;
    }

    if busy_with_new_work || !seen.session_alive {
        return false;
    }

    if now.saturating_sub(watch.settled_at) < rules.reap_after {
        return false;
    }

    // The same guard that protects the leader protects a person at this pane.
    safe_to_type(&seen.tail, &seen.tail)
}

/// Whether it is safe to type into a TUI right now.
///
/// The contract is two consecutive reads, but comparing whole frames does not
/// work here: the pane log is append-only and the engine redraws its footer —
/// the clock, the context counter, the spinner — so two reads are never equal
/// and the leader would never be woken at all. What is compared instead is the
/// composer line: refuse while a turn is running, refuse if there is text in the
/// composer now, and refuse if there was text in it a moment ago, because a
/// person may be mid-sentence between redraws.
pub fn safe_to_type(previous: &str, latest: &str) -> bool {
    if turn_running(latest) {
        return false;
    }

    match composer_line(latest) {
        Some(line) if line.trim().is_empty() => {}
        _ => return false,
    }

    !matches!(composer_line(previous), Some(line) if !line.trim().is_empty())
}

/// Whether the engine is in the middle of a turn.
///
/// Measured rather than assumed: across the recorded sessions on this machine,
/// "esc to interrupt" appears once, while the spinner line — a glyph, a gerund and
/// an ellipsis, `✶ Skedaddling… (3s · thinking)` — appears 547 times. Keying only
/// on the interrupt hint would have typed into running turns all day.
pub fn turn_running(frame: &str) -> bool {
    let lowered = frame.to_lowercase();
    if lowered.contains("esc to interrupt") || lowered.contains("esc to cancel") {
        return true;
    }

    frame
        .lines()
        .rev()
        .take(16)
        .any(|line| spinner_line(line.trim()))
}

fn spinner_line(line: &str) -> bool {
    let mut characters = line.chars();
    let Some(first) = characters.next() else {
        return false;
    };

    if !matches!(first, '✻' | '✽' | '✶' | '✢' | '*' | '·' | '⠋' | '⠙' | '⠹') {
        return false;
    }

    let rest = characters.as_str();
    rest.contains('…') && !rest.to_lowercase().contains("done")
}

/// The text the human has typed, as best it can be read from a redrawn frame.
///
/// A real frame is not just a composer: the engine draws rules, a status line, a
/// model line and hints below it. Reading only the last line finds the status
/// line and concludes "unknown", which never wakes the leader at all. So the scan
/// walks up through the chrome and stops at the first prompt it recognises.
fn composer_line(frame: &str) -> Option<String> {
    const CHROME: usize = 16;

    let lines: Vec<&str> = frame
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();

    let window = lines.len().saturating_sub(CHROME);

    // The last prompt in the visible chrome is the composer. Anything printed
    // after it is footer the engine redrew — counters, timers, stray fragments —
    // and reading only the final line mistakes that noise for "no composer here",
    // which is how a leader ends up never being woken.
    lines[window..]
        .iter()
        .rev()
        .find_map(|line| {
            let bare = line.trim_start_matches('│').trim_end_matches('│').trim();
            bare.strip_prefix('❯')
                .or_else(|| bare.strip_prefix('>'))
                .map(|rest| rest.trim().to_owned())
        })
}

fn is_chrome(line: &str) -> bool {
    if line.is_empty() {
        return true;
    }

    if line.chars().all(|c| matches!(c, '─' | '-' | '═' | '╭' | '╮' | '╯' | '╰' | '│' | ' ')) {
        return true;
    }

    let lowered = line.to_lowercase();
    const KNOWN: &[&str] = &[
        "? for shortcuts",
        "⏵",
        "⚠",
        "model:",
        "session:",
        "reset:",
        "context left",
        "bypass permissions",
        "transcript saving",
        "shift+tab",
        "auto mode",
        "✻",
        "✽",
        "✶",
        "·",
    ];

    KNOWN.iter().any(|mark| lowered.starts_with(mark))
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct State {
    #[serde(default)]
    watches: BTreeMap<String, Watch>,
    #[serde(default)]
    pending_for_leader: Vec<String>,
    #[serde(default)]
    next_number: u32,
}

pub struct Supervisor {
    state: Mutex<State>,
    data_dir: PathBuf,
    pub rules: Rules,
}

impl Supervisor {
    pub fn new(data_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&data_dir);
        let data_dir = fs::canonicalize(&data_dir).unwrap_or(data_dir);
        let state = crate::db::load_state(&data_dir, "supervisor");

        Self {
            state: Mutex::new(state),
            data_dir,
            rules: Rules::default(),
        }
    }

    fn persist(&self, state: &State) {
        crate::db::save_state(&self.data_dir, "supervisor", state);
    }

    pub fn list(&self) -> Vec<Watch> {
        self.state.lock().watches.values().cloned().collect()
    }

    pub fn working(&self) -> Vec<Watch> {
        self.state
            .lock()
            .watches
            .values()
            .filter(|watch| watch.state == WatchState::Working)
            .cloned()
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn watch(
        &self,
        plan_id: &str,
        step_id: &str,
        task_id: &str,
        agent_id: &str,
        session_id: &str,
        repository_id: &str,
        worktree: &str,
        fingerprint: &str,
        now: u64,
    ) -> Watch {
        let mut state = self.state.lock();
        state.next_number += 1;

        let watch = Watch {
            id: format!("w{}", state.next_number),
            plan_id: plan_id.to_owned(),
            step_id: step_id.to_owned(),
            task_id: task_id.to_owned(),
            agent_id: agent_id.to_owned(),
            session_id: session_id.to_owned(),
            repository_id: repository_id.to_owned(),
            worktree: worktree.to_owned(),
            fingerprint: fingerprint.chars().take(60).collect(),
            delivered: false,
            resends: 0,
            state: WatchState::Working,
            started_at: now,
            settled_at: 0,
            reason: None,
            told_leader: false,
            wake_attempts: 0,
            last_wake: 0,
            reaped: false,
        };

        state.watches.insert(watch.id.clone(), watch.clone());
        self.persist(&state);
        watch
    }

    pub fn mark_delivered(&self, id: &str) {
        let mut state = self.state.lock();
        if let Some(watch) = state.watches.get_mut(id) {
            if !watch.delivered {
                watch.delivered = true;
                self.persist(&state);
            }
        }
    }

    pub fn count_resend(&self, id: &str) {
        let mut state = self.state.lock();
        if let Some(watch) = state.watches.get_mut(id) {
            watch.resends += 1;
            self.persist(&state);
        }
    }

    pub fn settle(&self, id: &str, reason: String, now: u64) -> Option<Watch> {
        let mut state = self.state.lock();
        let watch = state.watches.get_mut(id)?;

        watch.state = WatchState::Settled;
        watch.settled_at = now;
        watch.reason = Some(reason);
        let settled = watch.clone();

        state.pending_for_leader.push(settled.id.clone());
        self.persist(&state);
        Some(settled)
    }

    pub fn give_up(&self, id: &str, reason: String, now: u64) -> Option<Watch> {
        let mut state = self.state.lock();
        let watch = state.watches.get_mut(id)?;

        watch.state = WatchState::Abandoned;
        watch.settled_at = now;
        watch.reason = Some(reason);
        let stopped = watch.clone();

        state.pending_for_leader.push(stopped.id.clone());
        self.persist(&state);
        Some(stopped)
    }

    /// What the leader has not been told yet.
    pub fn news_for_leader(&self) -> Vec<Watch> {
        let state = self.state.lock();
        state
            .pending_for_leader
            .iter()
            .filter_map(|id| state.watches.get(id))
            .cloned()
            .collect()
    }

    pub fn leader_was_told(&self, ids: &[String], now: u64) {
        let mut state = self.state.lock();
        state.pending_for_leader.retain(|id| !ids.contains(id));

        for id in ids {
            if let Some(watch) = state.watches.get_mut(id) {
                watch.told_leader = true;
                watch.last_wake = now;
                watch.wake_attempts += 1;
            }
        }

        self.persist(&state);
    }

    pub fn mark_reaped(&self, id: &str) {
        let mut state = self.state.lock();
        if let Some(watch) = state.watches.get_mut(id) {
            watch.reaped = true;
            self.persist(&state);
        }
    }

    /// Watches that have finished but whose pane may still be held.
    pub fn settled(&self) -> Vec<Watch> {
        self.state
            .lock()
            .watches
            .values()
            .filter(|watch| watch.state != WatchState::Working && !watch.reaped)
            .cloned()
            .collect()
    }

    pub fn wake_is_due(&self, now: u64) -> bool {
        let state = self.state.lock();
        if state.pending_for_leader.is_empty() {
            return false;
        }

        let last = state
            .pending_for_leader
            .iter()
            .filter_map(|id| state.watches.get(id))
            .map(|watch| watch.last_wake)
            .max()
            .unwrap_or(0);

        let attempts = state
            .pending_for_leader
            .iter()
            .filter_map(|id| state.watches.get(id))
            .map(|watch| watch.wake_attempts)
            .max()
            .unwrap_or(0);

        if attempts >= self.rules.max_wakes {
            return false;
        }

        last == 0 || now.saturating_sub(last) >= self.rules.wake_backoff * (attempts as u64 + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn watch() -> Watch {
        Watch {
            id: "w1".into(),
            plan_id: "p1".into(),
            step_id: "p1s2".into(),
            task_id: "t9".into(),
            agent_id: "ada".into(),
            session_id: "pane-1".into(),
            repository_id: "svc".into(),
            worktree: "ada-tree".into(),
            fingerprint: "Prove /health with a node test".into(),
            delivered: false,
            resends: 0,
            state: WatchState::Working,
            started_at: 0,
            settled_at: 0,
            reason: None,
            told_leader: false,
            wake_attempts: 0,
            last_wake: 0,
            reaped: false,
        }
    }

    fn seen() -> Observation {
        Observation {
            session_alive: true,
            quiet_turn: false,
            idle_seconds: 0,
            tail: String::new(),
            changed_files: 0,
            card_has_evidence: false,
            transcript_says: None,
            age_seconds: 0,
        }
    }

    #[test]
    fn the_transcript_outranks_the_screen_in_both_directions() {
        let rules = Rules::default();
        let held = watch();

        // The pane shows it, but the engine never received it: a composer that
        // was never submitted looks exactly like this.
        let on_screen_only = Observation {
            age_seconds: 90,
            tail: "❯ Prove /health with a node test".into(),
            transcript_says: Some(false),
            ..seen()
        };
        assert_eq!(
            judge(&held, &on_screen_only, &rules),
            Verdict::Resend,
            "the screen is not proof of delivery"
        );

        // And the other way: the transcript has it even though the visible
        // buffer has scrolled past.
        let scrolled_away = Observation {
            age_seconds: 90,
            tail: "some later output entirely".into(),
            transcript_says: Some(true),
            ..seen()
        };
        assert_eq!(judge(&held, &scrolled_away, &rules), Verdict::Working);
    }

    #[test]
    fn without_a_transcript_the_screen_is_still_the_best_evidence_there_is() {
        let held = watch();
        let echoed = Observation {
            age_seconds: 90,
            tail: "❯ Prove /health with a node test".into(),
            transcript_says: None,
            ..seen()
        };

        assert_eq!(judge(&held, &echoed, &Rules::default()), Verdict::Working);
    }

    #[test]
    fn a_brief_that_never_arrived_is_sent_again_but_not_forever() {
        let rules = Rules::default();
        let mut held = watch();

        let young = Observation { age_seconds: 10, ..seen() };
        assert_eq!(judge(&held, &young, &rules), Verdict::Working, "give it a moment first");

        let waited = Observation { age_seconds: 60, ..seen() };
        assert_eq!(judge(&held, &waited, &rules), Verdict::Resend);

        held.resends = rules.max_resends;
        match judge(&held, &waited, &rules) {
            Verdict::LostIt(why) => assert!(why.contains("never reached ada"), "{why}"),
            other => panic!("should have given up: {other:?}"),
        }
    }

    #[test]
    fn the_brief_counts_as_delivered_when_the_pane_shows_it_wrapped() {
        let held = watch();
        let wrapped = Observation {
            age_seconds: 60,
            tail: "  > Prove /health with a\n  node test\n".into(),
            ..seen()
        };

        assert_eq!(judge(&held, &wrapped, &Rules::default()), Verdict::Working);
    }

    #[test]
    fn a_done_marker_settles_it_whatever_else_is_true() {
        let held = watch();
        let printed = Observation {
            tail: "running tests\nDONE:p1s2\n".into(),
            age_seconds: 5,
            ..seen()
        };

        match judge(&held, &printed, &Rules::default()) {
            Verdict::Finished(why) => assert!(why.contains("DONE:p1s2"), "{why}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn an_agent_that_exited_is_finished_and_the_reason_says_whether_it_left_work() {
        let mut held = watch();
        held.delivered = true;

        let empty_handed = Observation { session_alive: false, ..seen() };
        match judge(&held, &empty_handed, &Rules::default()) {
            Verdict::Finished(why) => assert!(why.contains("without changing anything"), "{why}"),
            other => panic!("{other:?}"),
        }

        let with_work = Observation { session_alive: false, changed_files: 3, ..seen() };
        match judge(&held, &with_work, &Rules::default()) {
            Verdict::Finished(why) => assert!(why.contains("3 changed file"), "{why}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn quiet_alone_is_not_finished_but_quiet_with_a_diff_is() {
        let mut held = watch();
        held.delivered = true;
        let rules = Rules::default();

        let thinking = Observation { idle_seconds: 200, ..seen() };
        assert_eq!(judge(&held, &thinking, &rules), Verdict::Working, "silence is not proof");

        let done = Observation { idle_seconds: 200, changed_files: 1, ..seen() };
        assert!(matches!(judge(&held, &done, &rules), Verdict::Finished(_)));
    }

    #[test]
    fn an_engine_waiting_at_its_prompt_counts_even_though_it_keeps_redrawing() {
        let mut held = watch();
        held.delivered = true;

        // What a live TUI actually looks like: never byte-idle for long.
        let waiting = Observation {
            quiet_turn: true,
            idle_seconds: 12,
            changed_files: 4,
            ..seen()
        };

        match judge(&held, &waiting, &Rules::default()) {
            Verdict::Finished(why) => assert!(why.contains("waiting at an empty prompt"), "{why}"),
            other => panic!("a redrawing status line hid a finished agent: {other:?}"),
        }

        let still_working = Observation { quiet_turn: false, idle_seconds: 12, changed_files: 4, ..seen() };
        assert_eq!(judge(&held, &still_working, &Rules::default()), Verdict::Working);
    }

    #[test]
    fn a_quiet_prompt_with_nothing_to_show_is_not_finished() {
        let mut held = watch();
        held.delivered = true;

        let empty_handed = Observation { quiet_turn: true, changed_files: 0, ..seen() };
        assert_eq!(judge(&held, &empty_handed, &Rules::default()), Verdict::Working);
    }

    #[test]
    fn a_settled_watch_is_never_judged_again() {
        let mut held = watch();
        held.state = WatchState::Settled;
        let gone = Observation { session_alive: false, ..seen() };

        assert_eq!(judge(&held, &gone, &Rules::default()), Verdict::Working);
    }

    #[test]
    fn typing_waits_for_an_empty_composer_and_a_finished_turn() {
        let idle = "╭──────────────╮\n│ >            │\n╰──────────────╯\n  ? for shortcuts";
        assert!(safe_to_type(idle, idle));

        let typing = "╭──────────────╮\n│ > give ada t │\n╰──────────────╯\n  ? for shortcuts";
        assert!(!safe_to_type(typing, typing), "never clobber a half-typed line");
        assert!(!safe_to_type(typing, idle), "they may be mid-sentence between redraws");

        let working = "✻ Cooking… (7s · esc to interrupt)\n╭────╮\n│ >  │\n╰────╯";
        assert!(!safe_to_type(working, working), "a running turn is busy");
        assert!(safe_to_type(working, idle), "but a turn that just ended is not");
    }

    #[test]
    fn a_plain_prompt_without_a_box_is_read_too() {
        assert!(safe_to_type("$ ls\n❯ ", "$ ls\n❯ "));
        assert!(!safe_to_type("$ ls\n❯ half a sentence", "$ ls\n❯ half a sentence"));
        assert!(!safe_to_type("no prompt at all", "no prompt at all"), "unclear means no");
    }

    #[test]
    fn a_finished_agent_hands_its_pane_back_but_only_when_nobody_is_using_it() {
        let rules = Rules::default();
        let mut held = watch();
        held.state = WatchState::Settled;
        held.settled_at = 100;

        let idle_frame = "╭────╮\n│ >  │\n╰────╯\n  ? for shortcuts";
        let waiting = Observation { session_alive: true, tail: idle_frame.into(), ..seen() };

        assert!(!should_reap(&held, &waiting, &rules, false, 120), "give it a moment first");
        assert!(should_reap(&held, &waiting, &rules, false, 200), "then take the pane back");

        assert!(
            !should_reap(&held, &waiting, &rules, true, 200),
            "not while it is working on something new"
        );

        let typing = Observation {
            session_alive: true,
            tail: "╭──────────────╮\n│ > wait, look │\n╰──────────────╯".into(),
            ..seen()
        };
        assert!(!should_reap(&held, &typing, &rules, false, 200), "somebody is at this pane");

        let mid_turn = Observation {
            session_alive: true,
            tail: "✻ Cooking… (4s)\n╭────╮\n│ >  │\n╰────╯".into(),
            ..seen()
        };
        assert!(!should_reap(&held, &mid_turn, &rules, false, 200), "a turn is running");
    }

    #[test]
    fn a_pane_is_reaped_once_and_a_working_watch_never_is() {
        let rules = Rules::default();
        let idle_frame = "╭────╮\n│ >  │\n╰────╯";
        let waiting = Observation { session_alive: true, tail: idle_frame.into(), ..seen() };

        let mut working = watch();
        working.settled_at = 0;
        assert!(!should_reap(&working, &waiting, &rules, false, 999), "it is still working");

        let mut done = watch();
        done.state = WatchState::Settled;
        done.settled_at = 0;
        assert!(should_reap(&done, &waiting, &rules, false, 999));

        done.reaped = true;
        assert!(!should_reap(&done, &waiting, &rules, false, 999), "once is enough");
    }

    #[test]
    fn an_agent_that_already_exited_needs_no_reaping() {
        let mut held = watch();
        held.state = WatchState::Settled;
        let gone = Observation { session_alive: false, ..seen() };

        assert!(!should_reap(&held, &gone, &Rules::default(), false, 999));
    }

    fn store(name: &str) -> Supervisor {
        let dir = std::env::temp_dir().join(format!("agentland-supervisor-{name}"));
        let _ = fs::remove_dir_all(&dir);
        Supervisor::new(dir)
    }

    #[test]
    fn a_watch_survives_a_restart_because_that_is_the_whole_point() {
        let dir = std::env::temp_dir().join("agentland-supervisor-restart");
        let _ = fs::remove_dir_all(&dir);

        {
            let supervisor = Supervisor::new(dir.clone());
            supervisor.watch("p1", "p1s1", "t1", "ada", "pane-1", "svc", "tree", "do the thing", 100);
        }

        let reopened = Supervisor::new(dir);
        let held = reopened.working();
        assert_eq!(held.len(), 1);
        assert_eq!(held[0].step_id, "p1s1");
        assert!(!held[0].delivered);
    }

    #[test]
    fn news_waits_for_the_leader_and_stops_once_it_is_told() {
        let supervisor = store("news");
        let watch = supervisor.watch("p1", "p1s1", "t1", "ada", "pane-1", "svc", "tree", "brief", 0);

        assert!(supervisor.news_for_leader().is_empty(), "nothing to say yet");
        supervisor.settle(&watch.id, "ada finished".into(), 10);

        let news = supervisor.news_for_leader();
        assert_eq!(news.len(), 1);
        assert_eq!(news[0].reason.as_deref(), Some("ada finished"));

        supervisor.leader_was_told(&[watch.id.clone()], 12);
        assert!(supervisor.news_for_leader().is_empty(), "said once is enough");
    }

    #[test]
    fn waking_the_leader_backs_off_and_eventually_stops() {
        let supervisor = store("backoff");
        let watch = supervisor.watch("p1", "p1s1", "t1", "ada", "pane-1", "svc", "tree", "brief", 0);
        supervisor.settle(&watch.id, "done".into(), 0);

        assert!(supervisor.wake_is_due(0), "the first wake is immediate");
        supervisor.leader_was_told(&[watch.id.clone()], 0);

        // it was told, so nothing is pending
        assert!(!supervisor.wake_is_due(1000));

        let second = supervisor.watch("p1", "p1s2", "t2", "kai", "pane-2", "svc", "tree", "brief", 0);
        supervisor.settle(&second.id, "done".into(), 0);
        supervisor.leader_was_told(&[second.id.clone()], 100);

        // pending again after a fresh settle
        let third = supervisor.settle(&second.id, "done again".into(), 200).expect("resettle");
        assert!(!supervisor.wake_is_due(210), "one attempt in, the backoff holds");
        assert!(supervisor.wake_is_due(400), "and lifts later");
        assert_eq!(third.wake_attempts, 1);
    }
}
