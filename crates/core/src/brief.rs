pub struct Ingredients<'a> {
    pub identity: Option<String>,
    pub base: &'a str,
    pub learned: Vec<String>,
    pub skills: Option<String>,
    pub mail: Vec<(String, String)>,
}

pub fn compose(parts: Ingredients<'_>) -> String {
    let mut brief = String::new();

    if let Some(identity) = parts.identity {
        brief.push_str(identity.trim());
        if !parts.base.trim().is_empty() {
            brief.push_str("\n\n");
        }
    }

    brief.push_str(parts.base.trim());

    if !parts.learned.is_empty() {
        brief.push_str("\n\nWhat this crew has learned:");
        for line in parts.learned {
            brief.push_str(&format!("\n- {line}"));
        }
    }

    if let Some(section) = parts.skills {
        brief.push_str(&section);
    }

    if !parts.mail.is_empty() {
        brief.push_str("\n\nMessages waiting for you:");
        for (from, text) in parts.mail {
            brief.push_str(&format!("\n- from {from}: {text}"));
        }
    }

    brief
}

pub fn spoken<'a>(brief: &'a str) -> Option<&'a str> {
    let trimmed = brief.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty(base: &str) -> Ingredients<'_> {
        Ingredients {
            identity: None,
            base,
            learned: Vec::new(),
            skills: None,
            mail: Vec::new(),
        }
    }

    #[test]
    fn an_agent_started_with_nothing_to_say_is_told_nothing() {
        assert_eq!(spoken(&compose(empty(""))), None);
        assert_eq!(spoken(&compose(empty("   \n  "))), None);
    }

    #[test]
    fn a_plain_start_still_carries_what_the_crew_knows() {
        let brief = compose(Ingredients {
            identity: None,
            base: "",
            learned: vec!["the dev server reads PORT".into()],
            skills: Some("\n\nSkills you have been given:\n\n## Code review\nread the diff".into()),
            mail: vec![("ada".into(), "the port is 4103".into())],
        });

        let told = spoken(&brief).expect("there is something to say");
        assert!(told.starts_with("What this crew has learned:"), "{told}");
        assert!(told.contains("the dev server reads PORT"));
        assert!(told.contains("Code review"));
        assert!(told.contains("from ada: the port is 4103"));
    }

    #[test]
    fn a_task_keeps_the_first_word() {
        let brief = compose(Ingredients {
            identity: None,
            base: "  Let a phone token read the skills library\n\nthe scope matrix  ",
            learned: vec!["one".into()],
            skills: None,
            mail: Vec::new(),
        });

        assert!(brief.starts_with("Let a phone token read the skills library"));
        assert!(brief.contains("the scope matrix"));
        assert!(brief.contains("\n\nWhat this crew has learned:\n- one"));
    }

    #[test]
    fn a_commander_is_told_who_it_is_before_anything_else() {
        let brief = compose(Ingredients {
            identity: Some("You are X, the commander of this crew.".into()),
            base: "ship the phone skills screen",
            learned: vec!["the scope matrix lives in auth.rs".into()],
            skills: None,
            mail: Vec::new(),
        });

        assert!(brief.starts_with("You are X, the commander of this crew."), "{brief}");
        assert!(brief.contains("ship the phone skills screen"));
    }

    #[test]
    fn an_identity_alone_is_still_worth_saying() {
        let brief = compose(Ingredients {
            identity: Some("You are X, the commander of this crew.".into()),
            base: "   ",
            learned: Vec::new(),
            skills: None,
            mail: Vec::new(),
        });

        assert_eq!(spoken(&brief), Some("You are X, the commander of this crew."));
    }

    #[test]
    fn the_sections_keep_their_order_so_the_engine_reads_the_work_first() {
        let brief = compose(Ingredients {
            identity: None,
            base: "fix the guard",
            learned: vec!["learned".into()],
            skills: Some("\n\nSkills you have been given:\nskill".into()),
            mail: vec![("kai".into(), "mail".into())],
        });

        let work = brief.find("fix the guard").expect("base");
        let learned = brief.find("What this crew has learned").expect("learned");
        let skills = brief.find("Skills you have been given").expect("skills");
        let mail = brief.find("Messages waiting for you").expect("mail");

        assert!(work < learned && learned < skills && skills < mail, "{brief}");
    }
}
