/// The pane as it looks on screen, rather than as it arrived.
///
/// A terminal draws by moving the cursor about: "Enter to confirm" can arrive
/// as a dozen writes with jumps between them. Stripping the escapes and keeping
/// the letters gives "Entertoconfirm", which is why the phone was reading
/// "RemoteControlnotstartedhere". Playing the bytes into a screen puts every
/// character where it belongs, and then the screen can simply be read.
pub fn on_screen(raw: &[u8], rows: u16, cols: u16) -> String {
    let mut screen = vt100::Parser::new(rows, cols, 0);
    screen.process(raw);

    let held = screen.screen();
    (0..rows)
        .map(|row| held.contents_between(row, 0, row, cols))
        .collect::<Vec<_>>()
        .join("\n")
}

/// What an engine actually said, out of everything its pane drew.
///
/// A terminal user interface redraws a status line, a spinner and a box border
/// several times a second. Reading a pane for what somebody said means throwing
/// almost all of it away — measured on real panes, where the last twenty lines
/// were nineteen redraws and one sentence.
pub fn last_words(frame: &str, wanted: usize) -> Vec<String> {
    let mut said: Vec<String> = frame
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !is_furniture(line))
        .map(|line| line.trim_start_matches(['❯', '>', '·', '⏵', '●', '○', '◐', '◑', '◒', '◓']).trim().to_owned())
        .filter(|line| line.chars().count() > 3)
        .filter(|line| !is_a_fragment(line))
        .collect();

    // A line drawn again is said once, wherever it was drawn: a notice the
    // engine repaints whole shows up as the same paragraph twice, and the
    // later copy is the one still on screen.
    let mut kept: Vec<String> = Vec::new();
    for line in said.drain(..).rev() {
        if !kept.contains(&line) {
            kept.push(line);
        }
    }
    kept.reverse();

    let from = kept.len().saturating_sub(wanted);
    kept.split_off(from)
}

/// A piece of a word the pane was in the middle of drawing — "onnecting…" —
/// or a bare status word: one token, no spaces, nothing a person said.
fn is_a_fragment(line: &str) -> bool {
    if line.contains(char::is_whitespace) {
        return false;
    }
    let lowered = line.to_lowercase();
    lowered.ends_with('…') || matches!(lowered.as_str(), "effort" | "thinking" | "connecting" | "working")
}

/// Whether a line is the interface rather than the conversation.
fn is_furniture(line: &str) -> bool {
    let squashed: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    let lowered = squashed.to_lowercase();

    // A rule, a border, or a bar of blocks.
    if squashed
        .chars()
        .all(|c| matches!(c, '─' | '━' | '╌' | '╍' | '═' | '·' | '⎯' | '▁'..='▓' | '-' | '_' | '=' | '⏵' | '▐' | '▛' | '▜' | '▝' | '▗'))
    {
        return true;
    }

    // The status line an engine keeps painting, and the hints beside it.
    const PAINTED: &[&str] = &[
        "model:",
        "session:",
        "reset:",
        "ctx:",
        "cost:",
        "weekly:",
        "bypasspermissions",
        "accepteditson",
        "planmodeon",
        "shift+tabtocycle",
        "tmuxfocus-events",
        "foragents",
        "esctointerrupt",
        "claude.ai/code",
        "?from=cli",
        "/rcconnecting",
        "checkingforupdates",
        "cwd:",
        // Claude Code's notice that another terminal holds Remote Control,
        // wrapped over four lines and repainted whole.
        "remotecontrol",
        "/remote-control",
        "moveittothisterminal",
        "thisterminalcan'tsee",
        "standingdown",
        "(code4090)",
        "sessionsonothermachines",
    ];

    if PAINTED.iter().any(|held| lowered.contains(held)) {
        return true;
    }

    // A spinner: a word or two and a stopwatch, drawn again every tick.
    lowered.contains("esctointerrupt") || lowered.starts_with("✻") && lowered.contains("tokens)")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lines taken off a real commander's pane.
    const PANE: &str = "\
Two things are yours to decide, and I've started nothing:
────────────────────────────────────────────
Model: Opus 5 | Ctx: 60.7k | ⎇ agent/x-desk
Session: 19.0% | Weekly: 4.0% | (+0,-0) | Cache...
Reset: 4hr 12m | Cost: $1.52
⏵⏵ bypass permissions on (shift+tab to cycle) · ← for agents
tmux focus-events off · add 'set -g focus-events on'
❯ discard the phantom cards and dismiss the idle crew
";

    #[test]
    fn the_status_line_an_engine_repaints_is_not_something_it_said() {
        let said = last_words(PANE, 10);

        assert!(said.iter().all(|line| !line.contains("Model:")));
        assert!(said.iter().all(|line| !line.contains("Cost:")));
        assert!(said.iter().all(|line| !line.contains("bypass permissions")));
        assert!(said.iter().all(|line| !line.contains("tmux")));
    }

    #[test]
    fn what_it_actually_said_survives() {
        let said = last_words(PANE, 10);

        assert!(said.iter().any(|line| line.starts_with("Two things are yours")));
        assert!(said.iter().any(|line| line == "discard the phantom cards and dismiss the idle crew"));
    }

    #[test]
    fn a_line_drawn_by_moving_the_cursor_reads_as_words() {
        // Written the way a terminal writes it: a jump, a word, a jump.
        let drawn = b"\x1b[2;1HEnter to confirm\x1b[2;20H\xc2\xb7 Esc to cancel";
        let screen = on_screen(drawn, 4, 40);

        assert!(
            screen.contains("Enter to confirm"),
            "the spaces have to survive: {screen:?}"
        );
    }

    #[test]
    fn borders_and_rules_are_not_words() {
        let said = last_words("────────\n╌╌╌╌╌╌\nsomething said\n", 5);

        assert_eq!(said, vec!["something said".to_owned()]);
    }

    #[test]
    fn only_the_last_few_are_wanted() {
        let frame = (1..=20).map(|n| format!("line number {n}")).collect::<Vec<_>>().join("\n");
        let said = last_words(&frame, 3);

        assert_eq!(said, vec!["line number 18", "line number 19", "line number 20"]);
    }

    #[test]
    fn the_remote_control_notice_and_the_status_words_are_not_words() {
        let frame = "\
● Remote Control not started here · another Claude Code on this machine (started 7s
ago) already has Remote Control for this conversation, so this terminal can't see
your sessions on other machines and they can't reach it · run /remote-control to
move it to this terminal
● Remote Control disconnected — another connection took over (code 4090)
● I moved the listener into the worktree and the tests pass.
● Remote Control not started here · another Claude Code on this machine (started 7s
ago) already has Remote Control for this conversation, so this terminal can't see
your sessions on other machines and they can't reach it · run /remote-control to
move it to this terminal
⏵⏵ bypass permissions on (shift+tab to cycle) · for agents ──────
effort
rc
c
onnecting…
❯ what next?
";
        let said = last_words(frame, 10);
        assert_eq!(
            said,
            vec![
                "I moved the listener into the worktree and the tests pass.".to_owned(),
                "what next?".to_owned()
            ],
            "{said:?}"
        );
    }

    #[test]
    fn a_paragraph_repainted_whole_is_said_once() {
        let frame = "first thing said\nsecond thing said\nfirst thing said\nsecond thing said\nand the end\n";
        let said = last_words(frame, 10);
        assert_eq!(said, vec!["first thing said", "second thing said", "and the end"]);
    }

    #[test]
    fn a_line_redrawn_twice_is_said_once() {
        let said = last_words("the same thing\nthe same thing\nand then this\n", 5);

        assert_eq!(said, vec!["the same thing".to_owned(), "and then this".to_owned()]);
    }
}
