use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::vault::{leaf_of, links_in, Note};

/// What is wrong with the vault, in a form somebody can act on.
///
/// A vault of markdown grows the way a garden does: a note is renamed and the
/// links to it stop reaching, a memory is proposed and nobody ever says yes, two
/// agents write down the same fact in different words. None of that is caught by
/// writing a note and none of it announces itself, so it is looked for on
/// purpose. Everything here is decided by reading the notes — no engine is
/// asked, so a check costs nothing and says the same thing twice running.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Trouble {
    pub kind: Kind,
    /// The note it is about, by slug, so it can be opened.
    pub slug: String,
    /// One line, addressed to whoever has to do something about it.
    pub says: String,
    /// The other note in it, where there is one.
    #[serde(default)]
    pub about: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Two memories in force, one of which was written to replace the other.
    /// The worst of these: the crew is being told both sides of a correction.
    Overruled,
    /// A `[[link]]` to a note nobody has written.
    DeadLink,
    /// A memory proposed long enough ago that forgetting to answer it is the
    /// likeliest reason it is still waiting.
    Unanswered,
    /// Two memories in force in one scope that say nearly the same thing.
    SaidTwice,
    /// A note nothing points at but the map it is listed on.
    Adrift,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Health {
    pub notes: usize,
    pub memories: usize,
    pub waiting: usize,
    pub trouble: Vec<Trouble>,
}

/// A proposal older than this has not been missed — it has been forgotten.
pub const LONG_ENOUGH: u64 = 7 * 24 * 60 * 60;

/// How much of two memories has to be the same words before saying so is worth
/// a person's time. High: a pair of memories about the same file share plenty.
const TOO_ALIKE: f32 = 0.7;

fn is_index(note: &Note) -> bool {
    leaf_of(&note.slug) == "index"
}

fn is_memory(note: &Note) -> bool {
    note.tags.iter().any(|tag| tag == "memory") || note.slug.contains("memory/")
}

fn folder_of(slug: &str) -> &str {
    match slug.rsplit_once('/') {
        Some((folder, _)) => folder,
        None => "",
    }
}

fn words_of(text: &str) -> BTreeSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.len() > 2)
        .map(str::to_lowercase)
        .collect()
}

fn alikeness(left: &BTreeSet<String>, right: &BTreeSet<String>) -> f32 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }

    let shared = left.intersection(right).count() as f32;
    let between = left.union(right).count() as f32;
    shared / between
}

/// Read the whole vault and say what wants attention.
pub fn check(notes: &[Note], now: u64) -> Health {
    let mut trouble = Vec::new();

    let by_slug: BTreeMap<&str, &Note> = notes.iter().map(|note| (note.slug.as_str(), note)).collect();
    let by_leaf: BTreeSet<&str> = notes.iter().map(|note| leaf_of(&note.slug)).collect();

    // A map is written by the machine and redrawn whole, so a link on one is
    // never a mistake somebody has to fix — it is a map waiting to be redrawn.
    let written: Vec<&Note> = notes.iter().filter(|note| !is_index(note)).collect();

    let mut pointed_at: BTreeSet<&str> = BTreeSet::new();

    for note in &written {
        for target in links_in(&note.body) {
            if by_leaf.contains(target.as_str()) || by_slug.contains_key(target.as_str()) {
                pointed_at.insert(
                    notes
                        .iter()
                        .find(|held| leaf_of(&held.slug) == target || held.slug == target)
                        .map(|held| held.slug.as_str())
                        .unwrap_or_default(),
                );
                continue;
            }

            trouble.push(Trouble {
                kind: Kind::DeadLink,
                slug: note.slug.clone(),
                says: format!("points at [[{target}]], which nobody has written"),
                about: Some(target),
            });
        }
    }

    for note in &written {
        // A memory is told, not looked up: nothing linking to it is how it is
        // meant to be. A note is the other way round.
        if is_memory(note) || pointed_at.contains(note.slug.as_str()) {
            continue;
        }

        trouble.push(Trouble {
            kind: Kind::Adrift,
            slug: note.slug.clone(),
            says: "nothing points at this but the map it is listed on".to_owned(),
            about: None,
        });
    }

    let memories: Vec<&Note> = written.iter().copied().filter(|note| is_memory(note)).collect();

    for memory in &memories {
        if memory.approved == Some(false) && !memory.retired && now.saturating_sub(memory.written_at) > LONG_ENOUGH {
            trouble.push(Trouble {
                kind: Kind::Unanswered,
                slug: memory.slug.clone(),
                says: format!("proposed by {} and never answered", memory.written_by),
                about: None,
            });
        }

        let Some(older) = memory.supersedes.as_deref() else {
            continue;
        };

        if memory.approved != Some(true) {
            continue;
        }

        if by_slug.get(older).is_some_and(|held| held.approved == Some(true)) {
            trouble.push(Trouble {
                kind: Kind::Overruled,
                slug: memory.slug.clone(),
                says: format!("this replaced [[{}]], and both are still being told to the crew", leaf_of(older)),
                about: Some(older.to_owned()),
            });
        }
    }

    let told: Vec<&&Note> = memories.iter().filter(|note| note.approved == Some(true)).collect();

    for (place, memory) in told.iter().enumerate() {
        let mine = words_of(&memory.body);

        for other in told.iter().skip(place + 1) {
            if folder_of(&memory.slug) != folder_of(&other.slug) {
                continue;
            }

            if alikeness(&mine, &words_of(&other.body)) < TOO_ALIKE {
                continue;
            }

            trouble.push(Trouble {
                kind: Kind::SaidTwice,
                slug: memory.slug.clone(),
                says: format!("says nearly what [[{}]] says", leaf_of(&other.slug)),
                about: Some(other.slug.clone()),
            });
        }
    }

    trouble.sort_by(|first, second| {
        first
            .kind
            .cmp(&second.kind)
            .then_with(|| first.slug.cmp(&second.slug))
            .then_with(|| first.about.cmp(&second.about))
    });

    Health {
        notes: written.len() - memories.len(),
        memories: memories.len(),
        waiting: memories
            .iter()
            .filter(|note| note.approved == Some(false) && !note.retired)
            .count(),
        trouble,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(slug: &str, body: &str) -> Note {
        Note {
            slug: slug.to_owned(),
            title: slug.replace('-', " "),
            body: body.to_owned(),
            written_by: "ada".to_owned(),
            written_at: 100,
            ..Note::default()
        }
    }

    fn memory(slug: &str, body: &str, approved: bool) -> Note {
        Note {
            tags: vec!["memory".to_owned()],
            approved: Some(approved),
            ..note(slug, body)
        }
    }

    #[test]
    fn a_link_to_a_note_nobody_wrote_is_named_with_the_note_holding_it() {
        let vault = vec![
            note("shared/ports", "the allocation lives in [[worktree ports]]"),
            note("shared/index", "- [[ports]]"),
        ];

        let found = check(&vault, 1_000);
        let dead: Vec<&Trouble> = found.trouble.iter().filter(|one| one.kind == Kind::DeadLink).collect();

        assert_eq!(dead.len(), 1, "{:?}", found.trouble);
        assert_eq!(dead[0].slug, "shared/ports");
        assert_eq!(dead[0].about.as_deref(), Some("worktree-ports"));
    }

    #[test]
    fn a_link_written_as_a_title_reaches_the_note_it_names() {
        let vault = vec![
            note("shared/ports", "see [[The Port Contract]]"),
            note("shared/the-port-contract", "one port per worktree"),
        ];

        let found = check(&vault, 1_000);

        assert!(
            !found.trouble.iter().any(|one| one.kind == Kind::DeadLink),
            "{:?}",
            found.trouble
        );
    }

    #[test]
    fn a_map_is_not_what_saves_a_note_from_being_adrift() {
        let vault = vec![
            note("shared/ports", "one port per worktree"),
            note("shared/index", "- [[ports]]"),
        ];

        let found = check(&vault, 1_000);
        let adrift: Vec<&Trouble> = found.trouble.iter().filter(|one| one.kind == Kind::Adrift).collect();

        assert_eq!(adrift.len(), 1, "{:?}", found.trouble);
        assert_eq!(adrift[0].slug, "shared/ports");
    }

    #[test]
    fn a_note_another_note_points_at_is_not_adrift() {
        let vault = vec![
            note("shared/ports", "one port per worktree"),
            note("shared/how-we-deploy", "the port comes from [[ports]]"),
        ];

        let found = check(&vault, 1_000);
        let adrift: Vec<&Trouble> = found.trouble.iter().filter(|one| one.kind == Kind::Adrift).collect();

        assert_eq!(adrift.len(), 1, "only the note nothing points at: {:?}", adrift);
        assert_eq!(adrift[0].slug, "shared/how-we-deploy");
    }

    #[test]
    fn a_memory_is_told_rather_than_looked_up_so_it_is_never_adrift() {
        let vault = vec![memory("shared/memory/the-dev-server-reads-port", "the dev server reads PORT", true)];

        let found = check(&vault, 1_000);

        assert_eq!(found.memories, 1);
        assert_eq!(found.notes, 0);
        assert!(found.trouble.is_empty(), "{:?}", found.trouble);
    }

    #[test]
    fn a_proposal_nobody_answered_is_only_raised_once_it_has_been_forgotten() {
        let waiting = memory("shared/memory/the-port-is-4103", "the port is 4103", false);
        let vault = vec![waiting];

        let soon = check(&vault, 100 + LONG_ENOUGH - 1);
        assert!(!soon.trouble.iter().any(|one| one.kind == Kind::Unanswered));
        assert_eq!(soon.waiting, 1, "it is still counted while it waits");

        let later = check(&vault, 100 + LONG_ENOUGH + 1);
        let unanswered: Vec<&Trouble> = later
            .trouble
            .iter()
            .filter(|one| one.kind == Kind::Unanswered)
            .collect();

        assert_eq!(unanswered.len(), 1);
        assert!(unanswered[0].says.contains("ada"), "{}", unanswered[0].says);
    }

    #[test]
    fn a_correction_that_left_the_old_memory_in_force_is_the_first_thing_said() {
        let older = memory("shared/memory/the-port-is-4103", "the port is 4103", true);
        let newer = Note {
            supersedes: Some("shared/memory/the-port-is-4103".to_owned()),
            ..memory("shared/memory/the-port-is-4200", "the port is 4200", true)
        };

        let found = check(&vec![older, newer], 1_000);

        assert_eq!(found.trouble[0].kind, Kind::Overruled, "{:?}", found.trouble);
        assert_eq!(found.trouble[0].slug, "shared/memory/the-port-is-4200");
        assert_eq!(
            found.trouble[0].about.as_deref(),
            Some("shared/memory/the-port-is-4103")
        );
    }

    #[test]
    fn a_correction_that_took_the_old_one_out_of_force_is_nothing_to_report() {
        let older = memory("shared/memory/the-port-is-4103", "the port is 4103", false);
        let newer = Note {
            supersedes: Some("shared/memory/the-port-is-4103".to_owned()),
            ..memory("shared/memory/the-port-is-4200", "the port is 4200", true)
        };

        let found = check(&vec![older, newer], 1_000);

        assert!(
            !found.trouble.iter().any(|one| one.kind == Kind::Overruled),
            "{:?}",
            found.trouble
        );
    }

    #[test]
    fn two_agents_writing_down_the_same_fact_are_told_about_each_other() {
        let vault = vec![
            memory("shared/memory/one", "the dev server reads PORT from the environment", true),
            memory("shared/memory/two", "the dev server reads the PORT environment variable", true),
        ];

        let found = check(&vault, 1_000);
        let twice: Vec<&Trouble> = found.trouble.iter().filter(|one| one.kind == Kind::SaidTwice).collect();

        assert_eq!(twice.len(), 1, "said once, not once per direction: {:?}", found.trouble);
        assert_eq!(twice[0].about.as_deref(), Some("shared/memory/two"));
    }

    #[test]
    fn the_same_fact_in_two_scopes_is_left_alone() {
        let vault = vec![
            memory("shared/memory/one", "the dev server reads PORT from the environment", true),
            memory(
                "atolye/svc-demo/memory/two",
                "the dev server reads PORT from the environment",
                true,
            ),
        ];

        let found = check(&vault, 1_000);

        assert!(
            !found.trouble.iter().any(|one| one.kind == Kind::SaidTwice),
            "one is the crew's and one is this project's: {:?}",
            found.trouble
        );
    }

    #[test]
    fn a_proposal_is_not_compared_with_what_is_in_force() {
        let vault = vec![
            memory("shared/memory/one", "the dev server reads PORT from the environment", true),
            memory("shared/memory/two", "the dev server reads PORT from the environment", false),
        ];

        let found = check(&vault, 1_000);

        assert!(
            !found.trouble.iter().any(|one| one.kind == Kind::SaidTwice),
            "answering it is what settles this: {:?}",
            found.trouble
        );
    }

    #[test]
    fn an_empty_vault_is_healthy() {
        let found = check(&[], 1_000);

        assert_eq!(found.notes, 0);
        assert_eq!(found.memories, 0);
        assert!(found.trouble.is_empty());
    }
}
