use std::collections::VecDeque;

use serde::Serialize;

use crate::budget::Room;

/// What one turn cost, and when it happened.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Spend {
    pub at: u64,
    /// Everything sent, including what the cache served.
    ///
    /// Counting cache reads is the conservative choice and deliberately so. If
    /// the provider discounts them this throttles a little early; if it does
    /// not and they were left out, this would throttle too late — and too late
    /// is the wall, which is the thing being avoided.
    pub input: u64,
    pub output: u64,
}

/// The per-minute ceilings this account is held to.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Ceilings {
    pub requests: u32,
    pub input: u64,
    pub output: u64,
}

impl Default for Ceilings {
    fn default() -> Self {
        Self {
            requests: 500,
            input: 1_000_000,
            output: 200_000,
        }
    }
}

/// What the last minute actually cost.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Rate {
    pub requests: u32,
    pub input: u64,
    pub output: u64,
}

impl Rate {
    /// How close the tightest of the three is to its ceiling, as a fraction.
    ///
    /// The tightest one decides: being at a tenth of the token ceiling is no
    /// comfort while the request count is at the top of its own.
    pub fn how_close(&self, ceilings: &Ceilings) -> f32 {
        let share = |used: f64, ceiling: f64| if ceiling <= 0.0 { 0.0 } else { used / ceiling };

        [
            share(self.requests as f64, ceilings.requests as f64),
            share(self.input as f64, ceilings.input as f64),
            share(self.output as f64, ceilings.output as f64),
        ]
        .into_iter()
        .fold(0.0_f64, f64::max) as f32
    }

    /// Which of the three is the one to worry about.
    pub fn tightest(&self, ceilings: &Ceilings) -> &'static str {
        let requests = self.requests as f64 / ceilings.requests.max(1) as f64;
        let input = self.input as f64 / ceilings.input.max(1) as f64;
        let output = self.output as f64 / ceilings.output.max(1) as f64;

        if requests >= input && requests >= output {
            "requests"
        } else if input >= output {
            "input tokens"
        } else {
            "output tokens"
        }
    }

    pub fn room(&self, ceilings: &Ceilings) -> Room {
        let close = self.how_close(ceilings);

        // A turn already in flight still lands, and it lands after this
        // decision. Stopping at the ceiling is stopping too late, so the room
        // runs out before the ceiling does.
        if close >= SPENT {
            Room::Spent
        } else if close >= TIGHT {
            Room::Tight
        } else {
            Room::Plenty
        }
    }
}

const TIGHT: f32 = 0.70;
const SPENT: f32 = 0.90;

/// A minute of spending, kept only as long as the minute lasts.
#[derive(Debug, Default)]
pub struct Window {
    spends: VecDeque<Spend>,
}

const A_MINUTE: u64 = 60;

impl Window {
    pub fn record(&mut self, spend: Spend) {
        self.spends.push_back(spend);
    }

    /// Drop what is older than a minute. A window nobody trims is a leak.
    pub fn forget_older_than(&mut self, now: u64) {
        let cutoff = now.saturating_sub(A_MINUTE);
        while self.spends.front().is_some_and(|held| held.at < cutoff) {
            self.spends.pop_front();
        }
    }

    pub fn in_the_last_minute(&self, now: u64) -> Rate {
        let cutoff = now.saturating_sub(A_MINUTE);

        self.spends
            .iter()
            .filter(|held| held.at >= cutoff)
            .fold(Rate::default(), |mut rate, held| {
                rate.requests += 1;
                rate.input += held.input;
                rate.output += held.output;
                rate
            })
    }

    pub fn len(&self) -> usize {
        self.spends.len()
    }

    pub fn is_empty(&self) -> bool {
        self.spends.is_empty()
    }
}

/// The tighter of two readings, because either wall stops the same work.
pub fn tighter(one: Room, other: Room) -> Room {
    match (one, other) {
        (Room::Spent, _) | (_, Room::Spent) => Room::Spent,
        (Room::Tight, _) | (_, Room::Tight) => Room::Tight,
        _ => Room::Plenty,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spend(at: u64, input: u64, output: u64) -> Spend {
        Spend { at, input, output }
    }

    #[test]
    fn a_minute_is_a_minute_and_the_rest_is_forgotten() {
        let mut window = Window::default();
        window.record(spend(100, 10, 1));
        window.record(spend(150, 20, 2));
        window.record(spend(159, 30, 3));

        // At 160 everything is inside the minute.
        let all = window.in_the_last_minute(160);
        assert_eq!(all.requests, 3);
        assert_eq!(all.input, 60);

        // At 165 the first has aged out.
        let some = window.in_the_last_minute(165);
        assert_eq!(some.requests, 2);
        assert_eq!(some.input, 50);

        window.forget_older_than(165);
        assert_eq!(window.len(), 2, "and it is dropped rather than carried forever");
    }

    #[test]
    fn the_tightest_of_the_three_decides() {
        let ceilings = Ceilings::default();

        // A tenth of the tokens, but the requests are at the top.
        let busy = Rate { requests: 480, input: 100_000, output: 10_000 };
        assert!(busy.how_close(&ceilings) > 0.9);
        assert_eq!(busy.tightest(&ceilings), "requests");
        assert_eq!(busy.room(&ceilings), Room::Spent);

        // Few requests, but each enormous.
        let heavy = Rate { requests: 5, input: 950_000, output: 1_000 };
        assert!(heavy.how_close(&ceilings) > 0.9);
        assert_eq!(heavy.tightest(&ceilings), "input tokens");

        let loud = Rate { requests: 5, input: 1_000, output: 190_000 };
        assert_eq!(loud.tightest(&ceilings), "output tokens");
    }

    #[test]
    fn the_room_runs_out_before_the_ceiling_does() {
        let ceilings = Ceilings::default();

        // A turn already in flight lands after this decision, so stopping at
        // the ceiling is stopping too late.
        assert_eq!(Rate { requests: 300, ..Rate::default() }.room(&ceilings), Room::Plenty);
        assert_eq!(Rate { requests: 360, ..Rate::default() }.room(&ceilings), Room::Tight);
        assert_eq!(Rate { requests: 460, ..Rate::default() }.room(&ceilings), Room::Spent);
        assert!(Rate { requests: 460, ..Rate::default() }.how_close(&ceilings) < 1.0,
            "it is spent before it is over");
    }

    #[test]
    fn an_idle_minute_is_all_room() {
        let window = Window::default();
        let rate = window.in_the_last_minute(1000);

        assert_eq!(rate, Rate::default());
        assert_eq!(rate.room(&Ceilings::default()), Room::Plenty);
    }

    #[test]
    fn either_wall_stops_the_same_work() {
        assert_eq!(tighter(Room::Plenty, Room::Spent), Room::Spent);
        assert_eq!(tighter(Room::Tight, Room::Plenty), Room::Tight);
        assert_eq!(tighter(Room::Spent, Room::Tight), Room::Spent);
        assert_eq!(tighter(Room::Plenty, Room::Plenty), Room::Plenty);
    }

    #[test]
    fn the_ceilings_are_the_ones_this_account_is_held_to() {
        let held = Ceilings::default();

        assert_eq!(held.requests, 500);
        assert_eq!(held.input, 1_000_000);
        assert_eq!(held.output, 200_000);
    }
}
