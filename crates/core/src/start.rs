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

/// Short names a crew is called by, two to a letter.
///
/// A chief is named after the workspace it commands rather than numbered after
/// the last one hired: "X2, X3, X4" tells a person the order they were made in,
/// which is the one thing about them nobody needs to know. Sharing the first
/// letter is enough to make the pairing obvious — Product has Pax, Demos has
/// Dex — and they stay short because a name is read in a rail two hundred
/// pixels wide.
const NAMES: &[&str] = &[
    "Ada", "Arne", "Bo", "Bex", "Cato", "Cy", "Dag", "Dex", "Edda", "Enzo", "Fen", "Fia", "Gus",
    "Gil", "Hana", "Hugo", "Ida", "Iris", "Jo", "Juno", "Kai", "Kira", "Lena", "Lux", "Mira",
    "Moss", "Nell", "Nia", "Odin", "Ona", "Pax", "Pia", "Quill", "Quin", "Rune", "Rex", "Sten",
    "Sol", "Tor", "Tessa", "Uma", "Ulf", "Vera", "Vidar", "Wren", "Wolf", "Xan", "Xia", "Yara",
    "Yuri", "Zed", "Zola",
];

/// The letter a name is looked up by, with the alphabet this app is actually
/// used in folded onto it: a workspace called "Ölçüm" is an Ö to a person and
/// an O to the list.
fn first_letter(name: &str) -> Option<char> {
    let folded = |character: char| match character {
        'ç' => 'c',
        'ğ' => 'g',
        'ı' | 'î' => 'i',
        'ö' => 'o',
        'ş' => 's',
        'ü' | 'û' => 'u',
        'â' => 'a',
        other => other,
    };

    name.chars()
        .flat_map(char::to_lowercase)
        .map(folded)
        .find(char::is_ascii_alphabetic)
}

/// What to call the chief of a workspace, when nobody has said.
///
/// A suggestion, not a decision: whoever makes the workspace can type any name
/// they like, and this is what the field offers them. Names already answered to
/// are skipped, so two workspaces starting with the same letter get two people.
pub fn name_for_a_chief(workspace: &str, taken_ids: &[String]) -> String {
    let free = |candidate: &str| !taken_ids.iter().any(|held| held == &slug(candidate));

    if let Some(letter) = first_letter(workspace) {
        if let Some(name) = NAMES
            .iter()
            .filter(|name| name.to_lowercase().starts_with(letter))
            .find(|name| free(name))
        {
            return (*name).to_owned();
        }
    }

    // Nothing left under that letter — a name from anywhere beats a number, and
    // the number is only reached when the whole list is spoken for.
    NAMES
        .iter()
        .find(|name| free(name))
        .map(|name| (*name).to_owned())
        .unwrap_or_else(|| commander_name(None, taken_ids))
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
        .find(|engine| engine.installed && engine.takes_the_tools)
        .or_else(|| engines.iter().find(|engine| engine.installed))
        .map(|engine| engine.id.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crew::PromptStyle;

    fn engine(id: &'static str, installed: bool, takes_the_tools: bool) -> Engine {
        Engine {
            id,
            name: id,
            command: id,
            resume: &[],
            model_flag: None,
            prompt_style: PromptStyle::Positional,
            takes_the_tools,
            resume_carries_a_brief: false,
            own_conversation: None,
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
    fn a_chief_is_named_after_the_workspace_rather_than_numbered() {
        assert_eq!(name_for_a_chief("Product", &[]), "Pax");
        assert_eq!(name_for_a_chief("Demos", &[]), "Dag");
        assert_eq!(name_for_a_chief("atölye", &[]), "Ada", "the alphabet it is used in counts");
        assert_eq!(name_for_a_chief("Ölçüm", &[]), "Odin");
    }

    #[test]
    fn two_workspaces_under_one_letter_are_two_people() {
        let first = name_for_a_chief("Product", &[]);
        let second = name_for_a_chief("Platform", &[slug(&first)]);

        assert_ne!(first, second);
        assert!(second.to_lowercase().starts_with('p'), "{second}");
    }

    #[test]
    fn a_workspace_with_no_letter_in_its_name_still_gets_somebody() {
        let name = name_for_a_chief("42", &[]);

        assert!(!name.is_empty());
        assert!(NAMES.contains(&name.as_str()), "{name}");
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
            engine("toolless", true, false),
            engine("claude", true, true),
        ];

        assert_eq!(engine_for_a_commander(&catalog).as_deref(), Some("claude"));
    }

    #[test]
    fn an_engine_that_is_not_installed_is_not_offered() {
        let catalog = vec![
            engine("claude", false, true),
            engine("toolless", true, false),
        ];

        assert_eq!(engine_for_a_commander(&catalog).as_deref(), Some("toolless"));
        assert_eq!(engine_for_a_commander(&[engine("claude", false, true)]), None);
    }
}
