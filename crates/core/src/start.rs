use crate::crew::Engine;

const WORDS_IN_A_NAME: usize = 4;
const LONGEST_NAME: usize = 24;
const FALLBACK_NAME: &str = "start";
const FIRST_COMMANDER: &str = "X";
const MOST_TRIES: u32 = 99;

/// A name a git branch and a folder can both carry.
pub fn slug(value: &str) -> String {
    let mut out = String::with_capacity(value.len());

    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            out.push(character.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }

    out.trim_end_matches('-').to_owned()
}

fn shorten(name: &str) -> String {
    if name.len() <= LONGEST_NAME {
        return name.to_owned();
    }

    let cut = &name[..LONGEST_NAME];
    match cut.rfind('-') {
        Some(at) if at > 0 => cut[..at].to_owned(),
        _ => cut.to_owned(),
    }
}

fn unused(base: &str, taken: &[String], join: impl Fn(&str, u32) -> String) -> String {
    let free = |candidate: &str| !taken.iter().any(|held| held == candidate);

    if free(base) {
        return base.to_owned();
    }

    for number in 2..=MOST_TRIES {
        let candidate = join(base, number);
        if free(&candidate) {
            return candidate;
        }
    }

    join(base, MOST_TRIES)
}

/// What a project's first worktree is called, taken from the goal it was opened for.
///
/// The name becomes a branch — `agent/<name>` — and a folder on disk, so it is
/// cut from the first few words rather than from the whole sentence, and a goal
/// with nothing nameable in it still gets a worktree instead of an error.
pub fn worktree_name(goal: &str, taken: &[String]) -> String {
    let words: Vec<&str> = goal.split_whitespace().take(WORDS_IN_A_NAME).collect();
    let mut name = shorten(&slug(&words.join(" ")));

    if name.is_empty() {
        name = FALLBACK_NAME.to_owned();
    }

    unused(&name, taken, |base, number| format!("{base}-{number}"))
}

/// A name for the commander that nobody in the crew answers to yet.
///
/// The crew is keyed by the slug of a name, so the check is against ids: hiring
/// a second `X` fails at the registry, and failing there in the middle of
/// starting a project leaves a worktree with nobody in it.
pub fn commander_name(wanted: Option<&str>, taken_ids: &[String]) -> String {
    let base = wanted
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(FIRST_COMMANDER);

    let slugs: Vec<String> = taken_ids.to_vec();
    let free = |candidate: &str| !slugs.iter().any(|held| held == &slug(candidate));

    if free(base) {
        return base.to_owned();
    }

    for number in 2..=MOST_TRIES {
        let candidate = format!("{base}{number}");
        if free(&candidate) {
            return candidate;
        }
    }

    format!("{base}{MOST_TRIES}")
}

/// The engine a new project's commander runs on when nobody has said which.
///
/// A commander is worth nothing without `plan_create`, and an engine only has
/// that tool if the crew's `.mcp.json` can be handed to it. So an engine that
/// takes the tools beats whichever one happens to be first on PATH, and the
/// fallback is only reached when no installed engine takes them at all.
pub fn engine_for_a_commander(engines: &[Engine]) -> Option<String> {
    engines
        .iter()
        .find(|engine| engine.installed && !engine.mcp_flags.is_empty())
        .or_else(|| engines.iter().find(|engine| engine.installed))
        .map(|engine| engine.id.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crew::PromptStyle;

    fn engine(id: &'static str, installed: bool, mcp_flags: &'static [&'static str]) -> Engine {
        Engine {
            id,
            name: id,
            command: id,
            resume_flag: None,
            model_flag: None,
            permission_flag: None,
            mcp_flags,
            prompt_style: PromptStyle::Positional,
            installed,
            version: None,
        }
    }

    #[test]
    fn a_goal_becomes_a_name_a_branch_can_carry() {
        let name = worktree_name("Widen the scope matrix so a phone can read skills", &[]);

        assert_eq!(name, "widen-the-scope-matrix");
        assert!(name.chars().all(|character| character.is_ascii_alphanumeric() || character == '-'));
    }

    #[test]
    fn a_goal_written_in_any_alphabet_still_names_a_folder() {
        let name = worktree_name("Önizleme panelini düzelt", &[]);

        assert!(!name.is_empty(), "a goal must always name something");
        assert!(name.chars().all(|character| character.is_ascii_alphanumeric() || character == '-'), "{name}");
    }

    #[test]
    fn a_goal_with_nothing_nameable_in_it_still_gets_a_worktree() {
        assert_eq!(worktree_name("!!! ???", &[]), FALLBACK_NAME);
        assert_eq!(worktree_name("   ", &[]), FALLBACK_NAME);
    }

    #[test]
    fn the_same_goal_twice_does_not_ask_for_the_same_worktree() {
        let taken = vec!["fix-the-guard".to_owned()];
        let second = worktree_name("fix the guard", &taken);

        assert_eq!(second, "fix-the-guard-2");
        assert_eq!(
            worktree_name("fix the guard", &[taken[0].clone(), second]),
            "fix-the-guard-3"
        );
    }

    #[test]
    fn the_commander_gets_a_name_nobody_in_the_crew_is_using() {
        assert_eq!(commander_name(None, &[]), "X");
        assert_eq!(commander_name(None, &["x".to_owned()]), "X2");
        assert_eq!(commander_name(Some("  "), &["x".to_owned(), "x2".to_owned()]), "X3");
        assert_eq!(commander_name(Some("Ada"), &["x".to_owned()]), "Ada");
    }

    #[test]
    fn a_project_starts_on_an_engine_that_takes_the_crews_tools() {
        let catalog = vec![
            engine("toolless", true, &[]),
            engine("claude", true, &["--mcp-config"]),
        ];

        assert_eq!(engine_for_a_commander(&catalog).as_deref(), Some("claude"));
    }

    #[test]
    fn an_engine_that_is_not_installed_is_not_offered() {
        let catalog = vec![
            engine("claude", false, &["--mcp-config"]),
            engine("toolless", true, &[]),
        ];

        assert_eq!(engine_for_a_commander(&catalog).as_deref(), Some("toolless"));
        assert_eq!(engine_for_a_commander(&[engine("claude", false, &["--mcp-config"])]), None);
    }
}
