use serde::Serialize;

/// What an engine says about the account's quota.
///
/// The number that decides whether work can start is not one this app could
/// count for itself: the quota belongs to the account, and every engine on the
/// machine spends from it — including the ones nobody here started. So it is
/// read rather than tallied, from the one place that knows.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Usage {
    /// Percent of this session's allowance spent.
    pub session: f32,
    /// Percent of the week's allowance spent. The one that stops work.
    pub weekly: f32,
}

/// Which allowance an agent spends from.
///
/// An engine is knowable — it is how the agent was hired. The account within it
/// is not: a status line says `Weekly: 55%` and never whose week that is. So the
/// engine separates providers on its own, and an account label separates logins
/// within one provider only when somebody says which is which.
///
/// Getting this wrong is not a rounding error. A single global quota meant a
/// Claude account at ninety-five per cent would stop a Codex agent whose own
/// allowance had not been touched.
pub fn identity_of(engine: &str, account: Option<&str>) -> String {
    match account.map(str::trim).filter(|held| !held.is_empty()) {
        Some(account) => format!("{engine}/{account}"),
        None => engine.to_owned(),
    }
}

/// How much room is left, as a decision rather than a number.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Room {
    /// Work as normal.
    Plenty,
    /// Finish what is running; start nothing new.
    Tight,
    /// Nothing but what a person asks for by hand.
    Spent,
}

/// Where the week's allowance stops being somebody else's problem.
///
/// Below the first, nothing changes. Above it the crew stops taking on new work
/// but keeps finishing what it holds, because abandoning a half-done step wastes
/// what was already spent on it. Above the second, only a person's own request
/// gets through.
const TIGHT: f32 = 80.0;
const SPENT: f32 = 92.0;

impl Usage {
    pub fn room(self) -> Room {
        if self.weekly >= SPENT {
            Room::Spent
        } else if self.weekly >= TIGHT {
            Room::Tight
        } else {
            Room::Plenty
        }
    }
}

impl Room {
    /// Whether a new agent may be started or a new card handed out.
    pub fn may_start_work(self) -> bool {
        matches!(self, Room::Plenty)
    }

    /// Whether the supervisor may spend a turn telling the commander the news.
    ///
    /// A wake is a turn and a turn is money. When the week is tight, news waits
    /// for the person who opens the pane rather than costing a round trip to
    /// deliver itself.
    pub fn may_wake_the_commander(self) -> bool {
        matches!(self, Room::Plenty)
    }

    /// Whether an agent already holding a card may be nudged to finish it.
    pub fn may_finish_what_is_held(self) -> bool {
        !matches!(self, Room::Spent)
    }

    pub fn in_a_line(self) -> &'static str {
        match self {
            Room::Plenty => "the week has room",
            Room::Tight => "the week is tight — finishing what is held, starting nothing new",
            Room::Spent => "the week is spent — only what a person asks for by hand",
        }
    }
}

fn percent_after(plain: &str, label: &str) -> Option<f32> {
    let at = plain.find(label)? + label.len();
    let rest = plain[at..].trim_start();

    let digits: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();

    if digits.is_empty() || !rest[digits.len()..].trim_start().starts_with('%') {
        return None;
    }

    digits.parse().ok()
}

/// Read the quota off a pane's own status line.
///
/// The engine prints `Session: 4.0% | Weekly: 55.0%` and separates it with
/// non-breaking spaces, which is why the text is folded first. A line without
/// both numbers says nothing, and saying nothing is different from saying zero:
/// an engine that reports no usage is not an engine with none left to spend.
pub fn read_usage(output: &str) -> Option<Usage> {
    let plain = crate::context::strip_escapes(output);

    for line in plain.lines().rev().take(STATUS_LINES) {
        if let (Some(session), Some(weekly)) =
            (percent_after(line, "Session:"), percent_after(line, "Weekly:"))
        {
            return Some(Usage { session, weekly });
        }
    }

    None
}

const STATUS_LINES: usize = 12;

#[cfg(test)]
mod tests {
    use super::*;

    /// Captured off a live pane, non-breaking spaces and all.
    const REAL: &str = "Model:\u{a0}Opus\u{a0}5\u{a0}|\u{a0}Ctx:\u{a0}41.2k\u{a0}|\u{a0}\u{2387}\u{a0}agent/x-desk\nSession:\u{a0}4.0%\u{a0}|\u{a0}Weekly:\u{a0}55.0%\u{a0}|\u{a0}(+0,-0)\nReset:\u{a0}2hr\u{a0}33m\u{a0}|\u{a0}Cost:\u{a0}$0.65\n";

    #[test]
    fn an_allowance_belongs_to_an_engine_and_maybe_a_login() {
        assert_eq!(identity_of("claude", None), "claude");
        assert_eq!(identity_of("claude", Some("work")), "claude/work");
        assert_eq!(identity_of("codex", None), "codex");

        // A blank label is not a second account.
        assert_eq!(identity_of("claude", Some("  ")), "claude");

        // Two engines never share an allowance, whatever the labels say.
        assert_ne!(identity_of("claude", None), identity_of("codex", None));
        assert_ne!(identity_of("claude", Some("work")), identity_of("claude", Some("home")));
    }

    #[test]
    fn the_quota_is_read_off_a_real_status_line() {
        let held = read_usage(REAL).expect("the pane says so");

        assert_eq!(held.session, 4.0);
        assert_eq!(held.weekly, 55.0);
        assert_eq!(held.room(), Room::Plenty);
    }

    #[test]
    fn a_pane_that_says_nothing_about_it_is_not_read_as_empty() {
        // Saying nothing is different from saying zero: an engine that reports
        // no usage is not one with none left to spend, and reading it as zero
        // would let work start on an account that is already finished.
        assert_eq!(read_usage("Model: Opus 5 | Ctx: 41.2k\n"), None);
        assert_eq!(read_usage(""), None);
        assert_eq!(read_usage("Session: 4.0%\n"), None, "half the line is not the line");
    }

    #[test]
    fn a_number_that_is_not_a_percentage_is_not_a_percentage() {
        assert_eq!(read_usage("Session: 4.0 | Weekly: 55.0\n"), None);
        assert_eq!(read_usage("Session: many% | Weekly: 55.0%\n"), None);
    }

    #[test]
    fn the_week_decides_and_the_session_does_not() {
        // A session at its own limit is one pane's problem; the week is the
        // account's, and the account is what stops.
        let one_pane_spent = Usage { session: 99.0, weekly: 12.0 };
        assert_eq!(one_pane_spent.room(), Room::Plenty);

        let week_gone = Usage { session: 1.0, weekly: 95.0 };
        assert_eq!(week_gone.room(), Room::Spent);
    }

    #[test]
    fn tight_finishes_what_is_held_and_starts_nothing() {
        let tight = Usage { session: 10.0, weekly: 85.0 }.room();

        assert_eq!(tight, Room::Tight);
        assert!(!tight.may_start_work(), "a new card would not get finished either");
        assert!(tight.may_finish_what_is_held(), "abandoning it wastes what it already cost");
        assert!(!tight.may_wake_the_commander(), "a wake is a turn and a turn is money");
    }

    #[test]
    fn spent_leaves_only_what_a_person_asks_for() {
        let spent = Usage { session: 10.0, weekly: 99.9 }.room();

        assert!(!spent.may_start_work());
        assert!(!spent.may_finish_what_is_held());
        assert!(!spent.may_wake_the_commander());
    }

    #[test]
    fn the_thresholds_leave_room_to_land_what_is_flying() {
        // Stopping at the wall is stopping too late: whatever is mid-turn still
        // has to finish, and finishing costs.
        assert!(TIGHT < SPENT && SPENT < 100.0);
        assert!(Usage { session: 0.0, weekly: 79.9 }.room() == Room::Plenty);
        assert!(Usage { session: 0.0, weekly: 80.0 }.room() == Room::Tight);
        assert!(Usage { session: 0.0, weekly: 91.9 }.room() == Room::Tight);
        assert!(Usage { session: 0.0, weekly: 92.0 }.room() == Room::Spent);
    }
}
