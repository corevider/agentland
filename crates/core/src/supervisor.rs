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
    /// Whether a turn has been seen running since this watch began.
    ///
    /// An agent that has not started cannot have finished. Without this, a card
    /// handed into a worktree that already had changes was called done three
    /// seconds later: the pane was quiet because the turn had not begun, and
    /// the changes were somebody else's, from an hour before.
    #[serde(default)]
    pub worked: bool,
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

    // Asked before anything about delivery. A pane that is gone cannot be
    // written to, so an undelivered brief on a dead pane asked to be resent
    // forever: resending needs a pane, the attempt was never counted, and the
    // watch never gave up. Measured: 95 watches on disk, 72 of them still
    // "working", every one undelivered, the oldest two days old — each one read
    // a pane, a transcript and a worktree on every tick.
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

    if seen.card_has_evidence {
        return Verdict::Finished(format!("{} attached evidence to {}", watch.agent_id, watch.task_id));
    }

    // Changed files in a worktree are not this agent's work unless this agent
    // has worked. A card handed into a worktree somebody else had already
    // changed settled three seconds later, on a pane that was quiet because its
    // turn had not started yet.
    if seen.changed_files > 0 && watch.worked {
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

/// A pane that printed its step's own marker is finished.
///
/// A card with no plan behind it has no step id, and "done:" on its own is not
/// a marker — it is the first word of any summary. Measured on the commander's
/// pane: two cards it held settled on "the pane printed DONE:" the moment it
/// wrote up what it had delegated, with nothing written and nowhere to go.
fn done_marker(tail: &str, step_id: &str) -> Option<String> {
    if step_id.trim().is_empty() {
        return None;
    }

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
/// Whether the pane is holding a question open — the engine's own picker, not
/// ours. An agent that has stopped on one is waiting on a person, and saying it
/// is "at a prompt" hides that from the only person who can answer.
/// The last `lines` lines that actually say something, lowercased.
///
/// Blank lines are not counted, because a redrawing pane is mostly blank lines:
/// measured on a plan picker, taking the last 16 lines reached only as far back
/// as its second option — the question itself was four blank lines further up
/// and the pane sat unanswered.
fn the_end_of(frame: &str, lines: usize) -> String {
    let lowered = frame.to_lowercase();
    let mut held: Vec<&str> = lowered
        .lines()
        .rev()
        .filter(|line| !line.trim().is_empty())
        .take(lines)
        .collect();

    held.reverse();
    held.join("\n")
}

pub fn asking_the_human(frame: &str) -> bool {
    let tail = the_end_of(frame, 16);

    // A picker draws its hint line differently depending on what it is asking —
    // "enter to select" for a list, "esc to cancel · tab to amend" for a command
    // it wants approved — so the hint alone is not the signal. What every one of
    // them has is a way out and a numbered choice waiting.
    let squashed: String = tail.chars().filter(|c| !c.is_whitespace()).collect();
    let has_a_way_out = tail.contains("esc to cancel")
        || squashed.contains("esctocancel")
        || tail.contains("to navigate")
        // The plan picker has no escape hint at all: it offers "shift+tab to
        // approve with this feedback" and a path to edit in Vim. A reviewer sat
        // on one of these having finished its review, and nothing in the app
        // counted it as a question, so nobody ever answered it.
        || squashed.contains("shift+tabtoapprove")
        || squashed.contains("wouldyouliketoproceed");
    // Both halves are read squashed as well as spaced. A pane redraws its hint
    // line a character at a time, and what lands in the frame is often
    // "Entertoconfirm·Esctocancel" — which matched the way out and missed the
    // choice, so a pane sitting on Claude's own "do you trust this folder?"
    // counted as nobody asking anything and was left there.
    let offers_a_choice = tail.contains("enter to select")
        || tail.contains("enter to confirm")
        || squashed.contains("entertoselect")
        || squashed.contains("entertoconfirm")
        || squashed.contains("doyouwanttoproceed")
        || squashed.contains("1.yes");

    has_a_way_out && offers_a_choice
}

/// Claude having written a plan and waiting to be told to run it.
///
/// This is not a permission question — the agent was handed a step and the plan
/// is for that step — so it does not belong in front of a person. It sat
/// unanswered because nothing recognised it: a reviewer finished its review,
/// wrote a plan, and waited at "Would you like to proceed?" until somebody
/// noticed by eye.
pub fn plan_is_waiting(frame: &str) -> bool {
    let tail = the_end_of(frame, 16);
    let squashed: String = tail.chars().filter(|c| !c.is_whitespace()).collect();

    squashed.contains("wouldyouliketoproceed")
        && squashed.contains("1.yes")
        && squashed.contains("2.yes")
}

/// The engine asking whether to resume the whole of a long session.
///
/// Ours to answer: this app is what started the pane with `--resume`, and the
/// summary is both the cheaper answer and the one the engine recommends. A
/// commander sat on one of these for hours — eight hours of conversation and
/// 183k tokens, waiting to be told which.
pub fn resume_is_waiting(frame: &str) -> bool {
    let squashed: String = the_end_of(frame, 16)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    squashed.contains("resumefromsummary") && squashed.contains("resumefullsession")
}

/// Which way to answer it: the way this agent was hired to work.
///
/// "Auto mode" for somebody already trusted to edit without asking, and
/// "manually approve edits" for anybody else — the picker is answered, but not
/// with more rope than the role was given.
pub fn answer_for_the_plan(permissions: Option<&str>) -> &'static str {
    match permissions {
        Some("bypassPermissions") | Some("acceptEdits") => "1",
        _ => "2",
    }
}

pub fn turn_running(frame: &str) -> bool {
    let lowered = frame.to_lowercase();
    // "esc to cancel" is the picker's way out, not a turn in flight — a pane
    // stopped on "do you want to proceed?" carries it while nothing runs, and
    // reading it as work is how a held question hides behind "working".
    if lowered.contains("esc to interrupt") {
        return true;
    }

    // The engine redraws counters and fragments below its spinner, so the
    // spinner is not always near the bottom — measured at 30-odd lines deep on a
    // wrapped pane, which read as "idle" while a turn was plainly running.
    frame
        .lines()
        .rev()
        .take(40)
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
        let data_dir = crate::exec::settled(&data_dir);
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
            worked: false,
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

    /// Note that this watch's pane has actually run a turn.
    pub fn mark_worked(&self, id: &str) {
        let mut state = self.state.lock();
        if let Some(watch) = state.watches.get_mut(id) {
            if !watch.worked {
                watch.worked = true;
                self.persist(&state);
            }
        }
    }

    pub fn mark_reaped(&self, id: &str) {
        let mut state = self.state.lock();
        if let Some(watch) = state.watches.get_mut(id) {
            watch.reaped = true;
            self.persist(&state);
        }
    }

    /// Drop watches nothing will ever hear from again.
    ///
    /// A watch is kept so a step can settle and the commander can be told. One
    /// whose brief never landed, whose pane died with the process it ran in,
    /// and which has sat there for hours will never do either — it only costs a
    /// pane read, a transcript read and a worktree read on every tick. Ninety-five
    /// had gathered, and seventy-two of them were still being watched.
    ///
    /// Returns how many were let go.
    pub fn forget_the_stranded(&self, now: u64, older_than: u64) -> usize {
        let mut state = self.state.lock();
        let before = state.watches.len();

        state.watches.retain(|_, watch| {
            let stranded = !watch.delivered
                && watch.state == WatchState::Working
                && now.saturating_sub(watch.started_at) > older_than;

            !stranded
        });

        let gone = before - state.watches.len();
        if gone > 0 {
            let kept: Vec<String> = state.watches.keys().cloned().collect();
            state.pending_for_leader.retain(|id| kept.contains(id));
            self.persist(&state);
        }

        gone
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
    #[test]
    fn a_command_the_engine_wants_approved_is_a_question_held_open() {
        // Recorded from zen's pane on the /version plan, where this read as
        // "waiting at a prompt" and the step stood still for four minutes.
        let frame = [
            "  Bash command",
            "npm test 2>&1",
            "Run the test suite",
            "This command requires approval",
            "Do you want to proceed?",
            "❯ 1. Yes",
            "  2. No",
            "Esc to cancel · Tab to amend · ctrl+e to explain",
        ]
        .join("\n");

        assert!(super::asking_the_human(&frame));
        assert!(!super::turn_running(&frame));
    }

    #[test]
    fn a_list_the_engine_offers_is_still_a_question_held_open() {
        let frame = "❯ 1. rebase onto ada-tree\n  2. start fresh\nEnter to select · Esc to cancel";

        assert!(super::asking_the_human(frame));
    }

    #[test]
    fn a_plain_prompt_is_not_a_question() {
        let frame = "❯\n⏵⏵ bypass permissions on (shift+tab to cycle)\nModel: Opus 5";

        assert!(!super::asking_the_human(frame));
    }

    #[test]
    fn a_spinner_buried_under_redrawn_footer_still_reads_as_a_running_turn() {
        let mut lines = vec!["✻ Gesticulating… (28s · ↓ 2.1k tokens)".to_owned()];
        for count in 0..30 {
            lines.push(format!("{count}"));
        }

        assert!(super::turn_running(&lines.join("\n")));
    }

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
            worked: false,
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
    fn a_card_with_no_step_has_no_marker_to_print() {
        let watch = Watch { step_id: String::new(), ..watch() };
        let seen = Observation {
            session_alive: true,
            tail: "Done: handed the fix to ada and the test to iris\n".into(),
            ..Observation::default()
        };

        assert!(
            matches!(judge(&watch, &seen, &Rules::default()), Verdict::Working),
            "a summary that starts with done: is not a marker"
        );
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
        held.worked = true;
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
        held.worked = true;

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

    /// Measured: a card handed into a worktree that already carried four
    /// changed files was called finished three seconds later, on a pane whose
    /// turn had not started. The changes were an hour old and somebody else's.
    #[test]
    fn a_quiet_pane_that_has_not_started_is_not_a_finished_step() {
        let fresh = Watch { delivered: true, worked: false, ..watch() };
        let quiet = Observation { quiet_turn: true, changed_files: 4, ..seen() };

        assert!(matches!(judge(&fresh, &quiet, &Rules::default()), Verdict::Working));

        let ran = Watch { worked: true, ..fresh };
        assert!(
            matches!(judge(&ran, &quiet, &Rules::default()), Verdict::Finished(_)),
            "once it has actually run, a quiet pane over changed files is done"
        );
    }

    #[test]
    fn a_watch_on_a_pane_that_is_gone_settles_instead_of_asking_forever() {
        let held = Watch { delivered: false, ..watch() };
        let gone = Observation {
            session_alive: false,
            age_seconds: 10_000,
            ..seen()
        };

        assert!(
            matches!(judge(&held, &gone, &Rules::default()), Verdict::Finished(_)),
            "resending needs a pane; without one the watch used to ask forever"
        );
    }

    #[test]
    fn a_stranded_watch_is_let_go_and_a_live_one_is_not() {
        let held = store("stranded");
        let now = 100_000;

        let stale = held.watch("p1", "s1", "t1", "ada", "pane-1", "repo", "tree", "do it", 1_000);
        let fresh = held.watch("p1", "s2", "t2", "ada", "pane-2", "repo", "tree", "do it", now);

        assert_eq!(held.forget_the_stranded(now, 6 * 60 * 60), 1);

        let left: Vec<String> = held.list().into_iter().map(|watch| watch.id).collect();
        assert!(!left.contains(&stale.id), "nothing will ever hear from that one");
        assert!(left.contains(&fresh.id), "and the one just handed out is untouched");
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

#[cfg(test)]
mod asking_tests {
    use super::asking_the_human;

    /// Measured on a reviewer that had finished its review and written a plan:
    /// the pane sat here and the app saw nothing to answer.
    const PLAN_PICKER: &str = "Claudehaswrittenupaplanandisreadytoexecute.Wouldyouliketoproceed?\n❯1.Yes,anduseautomode\n2.Yes,manuallyapproveedits\n3.TellClaudewhattochange\nshift+tabtoapprovewiththisfeedback";

    /// The frame as the pane actually draws it, blank lines and all. Taking the
    /// last sixteen lines of this reaches only the second option.
    #[test]
    fn the_blank_lines_a_pane_redraws_do_not_hide_the_question() {
        let frame = "Claude has written up a plan and is ready to execute. Would you like to proceed?\n\n\n❯1.Yes,anduseautomode\n\n\n2.Yes,manuallyapproveedits\n\n\n3.TellClaudewhattochange\n\n\nshift+tabtoapprovewiththisfeedback\n\n\n\n\n\nctrl+gtoeditinVim·~/.claude/plans/one.md\n\n\n";

        assert!(super::plan_is_waiting(frame));
        assert!(asking_the_human(frame));
    }

    /// Word for word off a commander's pane, blank lines and all.
    const RESUME_PICKER: &str = "This session is 8h 36m old and 182.9k tokens.\n\nResuming the full session will consume a substantial portion of your usage limits. We recommend resuming from a summary.\n\n❯ 1. Resume from summary (recommended)\n  2. Resume full session as-is\n  3. Don't ask me again\n\nEnter to confirm · Esc to cancel";

    #[test]
    fn being_asked_which_way_to_resume_is_ours_to_answer() {
        assert!(asking_the_human(RESUME_PICKER));
        assert!(super::resume_is_waiting(RESUME_PICKER));
    }

    #[test]
    fn a_plan_picker_is_not_a_resume_picker() {
        assert!(!super::resume_is_waiting(PLAN_PICKER));
        assert!(!super::plan_is_waiting(RESUME_PICKER));
    }

    #[test]
    fn a_plan_waiting_to_run_is_a_question() {
        assert!(asking_the_human(PLAN_PICKER));
        assert!(super::plan_is_waiting(PLAN_PICKER));
    }

    #[test]
    fn it_is_answered_the_way_the_agent_was_hired_to_work() {
        assert_eq!(super::answer_for_the_plan(Some("bypassPermissions")), "1");
        assert_eq!(super::answer_for_the_plan(Some("acceptEdits")), "1");
        assert_eq!(super::answer_for_the_plan(Some("default")), "2");
        assert_eq!(super::answer_for_the_plan(None), "2");
    }

    #[test]
    fn an_ordinary_permission_question_is_not_a_plan() {
        let frame = "❯ No, exit\n  Yes, I trust this folder\nEnter to confirm · Esc to cancel";
        assert!(!super::plan_is_waiting(frame));
    }

    /// Measured off a live pane: Claude asking whether the folder is trusted,
    /// with the hint line as the frame actually carried it.
    #[test]
    fn a_hint_line_the_pane_drew_without_spaces_is_still_a_question() {
        let frame = "Accessingworkspace:\n/home/ege\n❯No,exit\nYes,Itrustthisfolder\nEntertoconfirm·Esctocancel";
        assert!(asking_the_human(frame));
    }

    #[test]
    fn the_same_line_with_its_spaces_is_read_the_same_way() {
        let frame = "❯ No, exit\n  Yes, I trust this folder\nEnter to confirm · Esc to cancel";
        assert!(asking_the_human(frame));
    }

    #[test]
    fn a_picker_on_screen_is_an_agent_waiting_on_a_person() {
        let frame = "What base should the /version work build on?\n❯ 1. Rebase onto ada-tree\n  2. Merge first\nEnter to select · ↑/↓ to navigate · Esc to cancel";
        assert!(asking_the_human(frame));
    }

    #[test]
    fn a_pane_at_an_ordinary_prompt_is_not_asking_anything() {
        let frame = "● Done.\n\n❯\n\nModel: Opus 5 | Ctx: 42k\n⏵⏵ bypass permissions on (shift+tab to cycle)";
        assert!(!asking_the_human(frame));
    }

    #[test]
    fn the_words_have_to_be_on_screen_now_not_somewhere_up_the_scrollback() {
        let old = "Enter to select · Esc to cancel\n".to_owned() + &"a line of work\n".repeat(30);
        assert!(!asking_the_human(&old), "a picker answered long ago is not a question");
    }
}
