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
}
