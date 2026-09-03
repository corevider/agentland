use serde::Deserialize;

/// What the command line was asked for.
#[derive(Clone, Debug, PartialEq)]
pub enum Wanted {
    /// Where the core is and who is running.
    Status,
    /// Put a pane on this terminal.
    Attach(String),
    /// Say something to an agent without watching it.
    Say { who: String, words: String },
    Help,
}

pub fn read_args(args: &[String]) -> Wanted {
    let Some(first) = args.first().map(String::as_str) else {
        return Wanted::Status;
    };

    match first {
        "status" | "ls" => Wanted::Status,
        "attach" | "watch" => args
            .get(1)
            .map(|who| Wanted::Attach(who.clone()))
            .unwrap_or(Wanted::Help),
        "say" | "tell" => match (args.get(1), args.len() > 2) {
            (Some(who), true) => Wanted::Say {
                who: who.clone(),
                words: args[2..].join(" "),
            },
            _ => Wanted::Help,
        },
        _ => Wanted::Help,
    }
}

/// Just enough of an agent to find its pane.
#[derive(Clone, Debug, Deserialize)]
pub struct Hired {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub presence: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
}

/// The pane somebody means by a word.
///
/// An id, a name, or the pane itself — whichever they had to hand. Case does
/// not matter, because "X" is written both ways in the same breath.
pub fn pane_of(crew: &[Hired], wanted: &str) -> Option<String> {
    let wanted = wanted.trim();

    if wanted.starts_with("pane-") {
        return Some(wanted.to_owned());
    }

    crew.iter()
        .find(|held| {
            held.id.eq_ignore_ascii_case(wanted) || held.name.eq_ignore_ascii_case(wanted)
        })
        .and_then(|held| held.session_id.clone())
}

pub const HELP: &str = "\
agentland — the crew, from a terminal

  agentland status            where the core is, and who is running
  agentland attach <agent>    put that agent's pane on this terminal
  agentland say <agent> ...   send a line to an agent and leave it working

Attached, ctrl-] lets go of the pane without stopping anything.
The core keeps running whether or not anybody is watching.
";

#[cfg(test)]
mod tests {
    use super::*;

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn nothing_asked_for_is_a_look_at_the_crew() {
        assert_eq!(read_args(&[]), Wanted::Status);
        assert_eq!(read_args(&args("status")), Wanted::Status);
    }

    #[test]
    fn attaching_needs_somebody_to_attach_to() {
        assert_eq!(read_args(&args("attach ada")), Wanted::Attach("ada".into()));
        assert_eq!(read_args(&args("attach")), Wanted::Help);
    }

    #[test]
    fn what_is_said_keeps_its_spaces() {
        assert_eq!(
            read_args(&args("say x carry on with the metrics work")),
            Wanted::Say {
                who: "x".into(),
                words: "carry on with the metrics work".into()
            }
        );
    }

    #[test]
    fn saying_nothing_is_not_saying() {
        assert_eq!(read_args(&args("say x")), Wanted::Help);
    }

    fn crew() -> Vec<Hired> {
        vec![
            Hired {
                id: "x".into(),
                name: "X".into(),
                session_id: Some("pane-1".into()),
                presence: Some("waiting".into()),
                role: Some("commander".into()),
            },
            Hired {
                id: "ada".into(),
                name: "Ada".into(),
                session_id: None,
                presence: Some("idle".into()),
                role: Some("implementer".into()),
            },
        ]
    }

    #[test]
    fn an_id_a_name_or_the_pane_itself_all_find_it() {
        assert_eq!(pane_of(&crew(), "x"), Some("pane-1".into()));
        assert_eq!(pane_of(&crew(), "X"), Some("pane-1".into()));
        assert_eq!(pane_of(&crew(), "pane-1"), Some("pane-1".into()));
    }

    #[test]
    fn an_agent_with_no_pane_has_no_pane_to_offer() {
        assert_eq!(pane_of(&crew(), "ada"), None);
        assert_eq!(pane_of(&crew(), "nobody"), None);
    }
}
