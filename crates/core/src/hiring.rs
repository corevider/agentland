//! What the crew may hire onto: which engines, and which logins on them.
//!
//! A person decides it once, in the settings, and the chief and the commanders
//! read it every time they hire — with what the person wrote about each engine
//! and how much of each login's week is left. It is kept as what is closed
//! rather than what is open: an engine or a login nobody has said anything
//! about is open, so a new install or a new login is usable until somebody
//! closes it.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Rules {
    /// Engines the crew may not hire onto.
    #[serde(default)]
    pub closed_engines: BTreeSet<String>,
    /// Logins the crew may not spend from: `engine/label` for one added beside
    /// the machine's own, the engine's id alone for the machine's own.
    #[serde(default)]
    pub closed_logins: BTreeSet<String>,
    /// What each engine is for, in the words of the person who opened it.
    #[serde(default)]
    pub notes: BTreeMap<String, String>,
}

impl Rules {
    pub fn engine_open(&self, engine_id: &str) -> bool {
        !self.closed_engines.contains(engine_id)
    }

    /// A login is open when its engine is and nobody closed the login itself.
    pub fn login_open(&self, engine_id: &str, account: Option<&str>) -> bool {
        self.engine_open(engine_id)
            && !self
                .closed_logins
                .contains(&crate::budget::identity_of(engine_id, account))
    }

    pub fn note(&self, engine_id: &str) -> Option<&str> {
        self.notes
            .get(engine_id)
            .map(|note| note.trim())
            .filter(|note| !note.is_empty())
    }

    /// Refuse what the person closed, saying what is open instead, so whoever
    /// is hiring can choose again rather than stop.
    pub fn allow(&self, engine_id: &str, account: Option<&str>, open_engines: &[&str]) -> Result<()> {
        if !self.engine_open(engine_id) {
            let open = if open_engines.is_empty() {
                "nothing".to_owned()
            } else {
                open_engines.join(", ")
            };
            bail!(
                "{engine_id} is closed to the crew — it may hire onto {open}; ask the person if the work needs {engine_id}"
            );
        }

        if !self.login_open(engine_id, account) {
            let which = account
                .map(str::trim)
                .filter(|label| !label.is_empty())
                .map(|label| format!("the {label} login"))
                .unwrap_or_else(|| "this machine's own login".to_owned());
            bail!(
                "{which} on {engine_id} is closed to the crew — hire onto another login from crew_engines, or ask the person"
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn closed(engines: &[&str], logins: &[&str]) -> Rules {
        Rules {
            closed_engines: engines.iter().map(|id| (*id).to_owned()).collect(),
            closed_logins: logins.iter().map(|id| (*id).to_owned()).collect(),
            notes: BTreeMap::new(),
        }
    }

    #[test]
    fn nothing_is_closed_until_somebody_closes_it() {
        let rules: Rules = serde_json::from_str("{}").expect("an empty rule book");

        assert!(rules.engine_open("codex"));
        assert!(rules.login_open("claude", Some("work")));
        assert!(rules.allow("gemini", None, &[]).is_ok());
    }

    #[test]
    fn a_closed_engine_closes_every_login_on_it() {
        let rules = closed(&["gemini"], &[]);

        assert!(!rules.login_open("gemini", None));
        assert!(!rules.login_open("gemini", Some("second")));
        assert!(rules.login_open("codex", None));
    }

    #[test]
    fn the_machines_own_login_and_a_second_one_are_closed_apart() {
        let rules = closed(&[], &["claude"]);
        assert!(!rules.login_open("claude", None));
        assert!(
            rules.login_open("claude", Some("work")),
            "closing the machine's login leaves the second one open"
        );

        let rules = closed(&[], &["claude/work"]);
        assert!(rules.login_open("claude", None));
        assert!(!rules.login_open("claude", Some("work")));
    }

    #[test]
    fn a_refusal_says_what_is_open_instead() {
        let rules = closed(&["gemini"], &["codex/second"]);

        let refused = rules
            .allow("gemini", None, &["claude", "codex"])
            .expect_err("gemini is closed");
        assert!(refused.to_string().contains("claude, codex"), "{refused}");

        let refused = rules
            .allow("codex", Some("second"), &["claude", "codex"])
            .expect_err("that login is closed");
        assert!(refused.to_string().contains("the second login on codex"), "{refused}");
    }

    #[test]
    fn a_blank_note_says_nothing() {
        let mut rules = Rules::default();
        rules.notes.insert("codex".into(), "  implementers and tests ".into());
        rules.notes.insert("claude".into(), "   ".into());

        assert_eq!(rules.note("codex"), Some("implementers and tests"));
        assert_eq!(rules.note("claude"), None);
    }
}
