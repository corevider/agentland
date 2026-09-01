use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ContextReading {
    PercentLeft(u8),
    TokensUsed(u64),
}

pub fn read_context(output: &str) -> Option<ContextReading> {
    let plain = strip_escapes(output);

    if let Some(percent) = last_percent_left(&plain) {
        return Some(ContextReading::PercentLeft(percent));
    }

    last_tokens_used(&plain).map(ContextReading::TokensUsed)
}

/// How full a pane has to be before it is worth trading for a fresh one.
///
/// Measured on the commander that ran this app's own demos: at 195k it was
/// still working but every turn cost a fortune, and at 367k the engine
/// compacted itself mid-message and swallowed the goal it had just been given.
/// The work is not in the pane — plans, cards, evidence and the vault all live
/// in the core — so a fresh session loses only the transcript.
const TOKENS_ARE_TOO_MANY: u64 = 200_000;
const TOO_LITTLE_LEFT: u8 = 15;

/// A session has to have lived a little before it can be replaced, or a pane
/// that reads high the moment it opens would be restarted forever.
const SETTLE_FIRST: u64 = 10 * 60;

/// Whether this pane should be traded for a clean one.
pub fn wants_a_fresh_session(reading: Option<ContextReading>, alive_for: u64, busy: bool) -> bool {
    if busy || alive_for < SETTLE_FIRST {
        return false;
    }

    match reading {
        Some(ContextReading::TokensUsed(tokens)) => tokens >= TOKENS_ARE_TOO_MANY,
        Some(ContextReading::PercentLeft(left)) => left <= TOO_LITTLE_LEFT,
        None => false,
    }
}

/// What a pane says about being rate limited.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RateLimit {
    /// The wait in the engine's own words — `4hr 10m` — rather than a number
    /// recomputed from it. It is a countdown somebody else owns.
    pub resets_in: Option<String>,
}

/// Whether this pane is waiting out a rate limit rather than working.
///
/// A rate-limited pane looks exactly like a busy one: it redraws a retry
/// counter, so it never falls silent and `last_output_at` never settles. An
/// agent sat in that loop for forty-four minutes here while every panel showed
/// it working and nothing said a word.
///
/// The match is deliberately tight — the bracketed words *and* a status line's
/// pipes. Loose matching would find this sentence in a pane that happened to be
/// reading this repository, and an agent that greps for the phrase would report
/// itself throttled.
pub fn read_rate_limit(output: &str) -> Option<RateLimit> {
    let plain = strip_escapes(output);
    let lines: Vec<&str> = plain.lines().collect();

    let limited = lines
        .iter()
        .rev()
        .take(STATUS_LINES)
        .any(|line| line.contains("[Rate limited]") && line.contains('|'));

    if !limited {
        return None;
    }

    let resets_in = lines
        .iter()
        .rev()
        .take(STATUS_LINES)
        .find_map(|line| line.split("Reset:").nth(1))
        .map(|rest| rest.split('|').next().unwrap_or(rest).trim().to_owned())
        .filter(|value| !value.is_empty());

    Some(RateLimit { resets_in })
}

/// How far back a status line can be. A pane redraws, so the last thing written
/// is not always the last line of the buffer.
const STATUS_LINES: usize = 12;

pub fn strip_escapes(output: &str) -> String {
    let mut plain = String::with_capacity(output.len());
    let mut characters = output.chars().peekable();

    while let Some(character) = characters.next() {
        if character != '\u{1b}' {
            plain.push(if character == '\u{a0}' { ' ' } else { character });
            continue;
        }

        match characters.next() {
            Some('[') | Some(']') => {
                for inner in characters.by_ref() {
                    if inner.is_ascii_alphabetic() || inner == '\u{7}' {
                        break;
                    }
                }
            }
            Some(_) => {}
            None => break,
        }
    }

    plain
}

fn last_percent_left(plain: &str) -> Option<u8> {
    let lowered = plain.to_lowercase();
    let mut found = None;

    for phrase in ["until auto-compact", "context left", "context remaining"] {
        let mut from = 0;
        while let Some(offset) = lowered[from..].find(phrase) {
            let at = from + offset;
            let window_start = at.saturating_sub(24);
            let window_end = (at + phrase.len() + 24).min(lowered.len());

            if let Some(percent) = percent_in(&lowered[window_start..window_end]) {
                found = Some((at, percent));
            }

            from = at + phrase.len();
        }
    }

    found.map(|(_, percent)| percent)
}

fn percent_in(window: &str) -> Option<u8> {
    let bytes = window.as_bytes();

    for (index, byte) in bytes.iter().enumerate() {
        if *byte != b'%' {
            continue;
        }

        let mut start = index;
        while start > 0 && bytes[start - 1].is_ascii_digit() {
            start -= 1;
        }

        if start < index {
            if let Ok(value) = window[start..index].parse::<u32>() {
                return Some(value.min(100) as u8);
            }
        }
    }

    None
}

fn last_tokens_used(plain: &str) -> Option<u64> {
    let lowered = plain.to_lowercase();
    let mut found = None;
    let mut from = 0;

    while let Some(offset) = lowered[from..].find("ctx:") {
        let at = from + offset + "ctx:".len();
        if let Some(tokens) = tokens_at(&plain[at..]) {
            found = Some(tokens);
        }
        from = at;
    }

    found
}

fn tokens_at(rest: &str) -> Option<u64> {
    let trimmed = rest.trim_start();
    let mut digits = String::new();
    let mut characters = trimmed.chars();

    for character in characters.by_ref() {
        if character.is_ascii_digit() || (character == '.' && !digits.contains('.')) {
            digits.push(character);
        } else {
            return Some(scale(&digits, character));
        }
    }

    scale_or_none(&digits)
}

fn scale(digits: &str, suffix: char) -> u64 {
    let value: f64 = digits.parse().unwrap_or_default();

    let multiplier = match suffix.to_ascii_lowercase() {
        'k' => 1_000.0,
        'm' => 1_000_000.0,
        _ => 1.0,
    };

    (value * multiplier).round() as u64
}

fn scale_or_none(digits: &str) -> Option<u64> {
    if digits.is_empty() {
        None
    } else {
        Some(scale(digits, ' '))
    }
}

#[cfg(test)]
mod limit_tests {
    use super::*;

    /// Captured off a live pane, non-breaking spaces and all.
    const REAL: &str = "Model: Opus 5 | Ctx: 95.7k | \u{a0}agent/x-desk | [Rate\u{a0}limited] | [Rate\u{a0}limited] | (+0,-0)\nReset:\u{a0}4hr\u{a0}10m | Cost: $2.18\n";

    #[test]
    fn a_throttled_pane_is_read_off_its_status_line() {
        let held = read_rate_limit(REAL).expect("the pane says so");

        assert_eq!(held.resets_in.as_deref(), Some("4hr 10m"));
    }

    #[test]
    fn a_working_pane_is_not_read_as_throttled() {
        let busy = "Model: Opus 5 | Ctx: 95.7k | agent/x-desk | (+12,-3) | Cost: $2.18\n";

        assert_eq!(read_rate_limit(busy), None);
        assert_eq!(read_rate_limit(""), None);
    }

    #[test]
    fn the_words_alone_are_not_enough() {
        // An agent reading this repository would otherwise report itself
        // throttled for having the phrase on its screen.
        let prose = "the pane said [Rate limited] for forty-four minutes and nothing noticed\n";

        assert_eq!(read_rate_limit(prose), None, "prose is not a status line");
    }

    #[test]
    fn a_limit_with_no_reset_time_is_still_a_limit() {
        let terse = "Model: Opus 5 | [Rate limited] | Cost: $2.18\n";

        assert_eq!(read_rate_limit(terse), Some(RateLimit { resets_in: None }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_reads_the_tokens_claude_code_reports_in_its_status_line() {
        let line = "Model: Fable 5 | Ctx:\u{a0}44.5k | agent/scope-skills | Thinking: high";
        assert_eq!(read_context(line), Some(ContextReading::TokensUsed(44_500)));
    }

    #[test]
    fn the_last_reading_wins_because_the_line_redraws() {
        let redrawn = "Ctx: 40.3k\nCtx: 42.4k\nCtx: 44.5k\n";
        assert_eq!(read_context(redrawn), Some(ContextReading::TokensUsed(44_500)));
    }

    #[test]
    fn a_percentage_is_only_reported_when_the_engine_prints_one() {
        assert_eq!(
            read_context("Context left until auto-compact: 23%"),
            Some(ContextReading::PercentLeft(23))
        );
        assert_eq!(
            read_context("18% context left"),
            Some(ContextReading::PercentLeft(18))
        );
        assert_eq!(read_context("the context is getting full"), None);
        assert_eq!(read_context("Cost: $0.81 | 69.0% weekly"), None);
    }

    #[test]
    fn a_percentage_beats_a_token_count_because_it_needs_no_arithmetic() {
        let both = "Ctx: 44.5k\nContext left until auto-compact: 12%";
        assert_eq!(read_context(both), Some(ContextReading::PercentLeft(12)));
    }

    #[test]
    fn output_with_no_reading_stays_empty_rather_than_guessing() {
        assert_eq!(read_context(""), None);
        assert_eq!(read_context("cargo test\n40 passed"), None);
        assert_eq!(read_context("Ctx: "), None);
    }

    #[test]
    fn escape_sequences_do_not_hide_the_reading() {
        let painted = "\u{1b}[2m\u{1b}[38;5;244mCtx: 12.0k\u{1b}[0m";
        assert_eq!(read_context(painted), Some(ContextReading::TokensUsed(12_000)));
    }

    #[test]
    fn a_zero_reading_is_a_reading() {
        assert_eq!(read_context("Ctx: 0 | branch"), Some(ContextReading::TokensUsed(0)));
    }

    #[test]
    fn a_pane_is_left_alone_until_it_is_actually_full() {
        use super::{wants_a_fresh_session, ContextReading};

        let hour = 3600;

        assert!(!wants_a_fresh_session(Some(ContextReading::TokensUsed(40_000)), hour, false));
        assert!(wants_a_fresh_session(Some(ContextReading::TokensUsed(210_000)), hour, false));
        assert!(!wants_a_fresh_session(Some(ContextReading::PercentLeft(60)), hour, false));
        assert!(wants_a_fresh_session(Some(ContextReading::PercentLeft(9)), hour, false));
    }

    #[test]
    fn a_pane_in_the_middle_of_something_is_never_taken_away() {
        use super::{wants_a_fresh_session, ContextReading};

        assert!(!wants_a_fresh_session(Some(ContextReading::TokensUsed(300_000)), 3600, true));
    }

    #[test]
    fn a_pane_that_just_opened_is_not_restarted_in_a_loop() {
        use super::{wants_a_fresh_session, ContextReading};

        assert!(!wants_a_fresh_session(Some(ContextReading::TokensUsed(300_000)), 30, false));
    }

    #[test]
    fn a_pane_that_says_nothing_about_its_context_is_left_alone() {
        use super::wants_a_fresh_session;

        assert!(!wants_a_fresh_session(None, 3600, false));
    }
}
