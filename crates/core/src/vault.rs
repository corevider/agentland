use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};

/// Where a note belongs.
///
/// A crew works across projects, and knowledge does not all live at the same
/// level: a port contract belongs to one repository, "the reviewer prefers small
/// commits" belongs to the workspace, and "how we write notes" is shared by
/// everything. The scope is the folder, so the vault reads as a place rather
/// than a heap: `shared/`, `<workspace>/`, `<workspace>/<project>/`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub enum Scope {
    #[default]
    Shared,
    Workspace(String),
    Project { workspace: String, project: String },
}

impl Scope {
    /// Read a scope the way an agent writes it: "shared", "workspace:atolye",
    /// "project:atolye/svc-demo", or a bare project id when the workspace is
    /// obvious from the repository.
    pub fn parse(text: &str, workspace: Option<&str>) -> Self {
        let text = text.trim();

        if text.is_empty() || text.eq_ignore_ascii_case("shared") {
            return Scope::Shared;
        }

        if let Some(rest) = text.strip_prefix("workspace:") {
            return Scope::Workspace(slug_for(rest));
        }

        let rest = text.strip_prefix("project:").unwrap_or(text);
        match rest.split_once('/') {
            Some((workspace, project)) => Scope::Project {
                workspace: slug_for(workspace),
                project: slug_for(project),
            },
            None => Scope::Project {
                workspace: slug_for(workspace.unwrap_or("workspace")),
                project: slug_for(rest),
            },
        }
    }

    /// The folder it lives in, relative to the vault.
    pub fn folder(&self) -> String {
        match self {
            Scope::Shared => "shared".to_owned(),
            Scope::Workspace(workspace) => workspace.clone(),
            Scope::Project { workspace, project } => format!("{workspace}/{project}"),
        }
    }

    pub fn title(&self) -> String {
        match self {
            Scope::Shared => "Shared".to_owned(),
            Scope::Workspace(workspace) => workspace.replace('-', " "),
            Scope::Project { project, .. } => project.replace('-', " "),
        }
    }
}

/// A vault of notes the crew keeps.
///
/// Not a database: a folder of markdown files with front matter and `[[links]]`,
/// which is what Obsidian and every other note tool already reads. The human can
/// open the folder, edit a note by hand, and the crew reads the edit; nothing
/// here is a format only Agentland understands.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Note {
    /// The note's path in the vault without the extension, e.g.
    /// `atolye/svc-demo/the-port-contract`. Links match on the last segment, so
    /// a note can be pointed at from anywhere without knowing where it lives.
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub written_by: String,
    #[serde(default)]
    pub written_at: u64,
    #[serde(default)]
    pub body: String,
    /// Whether this note may be told to an agent, and whether the human has
    /// said yes yet.
    ///
    /// `None` is an ordinary note: nothing reads it unless an agent goes
    /// looking. `Some(false)` is a memory waiting on the human. `Some(true)` is
    /// a memory that may be folded into a brief — which is why it needs a yes
    /// at all: a brief is read whether the agent asked for it or not.
    #[serde(default)]
    pub approved: Option<bool>,
    /// The memory this one replaces, by slug. Written down rather than said in
    /// prose, so a person can act on it with a click instead of reading every
    /// other memory to work out which one is meant.
    #[serde(default)]
    pub supersedes: Option<String>,
    /// Whether a person took this out of the crew's brief after having approved
    /// it. Not the same as never having been approved: one is waiting on a
    /// decision, the other is the decision — and showing them in the same list
    /// asks the same question twice.
    #[serde(default)]
    pub retired: bool,
    /// The notes this one points at, and the ones that point back.
    #[serde(default)]
    pub links: Vec<String>,
    #[serde(default)]
    pub backlinks: Vec<String>,
}

/// A title becomes a file name the way a person would write it: lower case,
/// spaces to dashes, nothing that needs escaping in a path.
pub fn slug_for(title: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = true;

    for character in title.trim().chars() {
        if character.is_alphanumeric() {
            for lower in character.to_lowercase() {
                slug.push(lower);
            }
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }

    while slug.ends_with('-') {
        slug.pop();
    }

    slug
}

/// The notes a body points at. A link is `[[a title]]`, and it is matched by the
/// slug of what is inside, so `[[The Port Contract]]` and `[[the-port-contract]]`
/// reach the same note.
/// The notes a note points at.
///
/// Code is not prose: a fenced block is quoted for reading, and a JS array of
/// pairs is not a link to anything. X hit this writing down a `[[path, method]]`
/// table and the vault turned it into a link to a note nobody will ever write —
/// so fences are skipped, and inline code with them.
pub fn links_in(body: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut fenced = false;

    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }

        if fenced {
            continue;
        }

        for piece in outside_inline_code(line) {
            collect_links(piece, &mut found);
        }
    }

    found
}

/// A line split into the parts that are not inside backticks.
fn outside_inline_code(line: &str) -> Vec<&str> {
    line.split('`')
        .enumerate()
        .filter(|(index, _)| index % 2 == 0)
        .map(|(_, piece)| piece)
        .collect()
}

fn collect_links(text: &str, found: &mut Vec<String>) {
    let mut rest = text;

    while let Some(start) = rest.find("[[") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("]]") else {
            break;
        };

        let inside = &after[..end];
        let target = inside.split('|').next().unwrap_or(inside);
        let slug = slug_for(target);
        if !slug.is_empty() && !found.contains(&slug) {
            found.push(slug);
        }

        rest = &after[end + 2..];
    }
}

/// Who points at whom, across the whole vault.
/// The name a link uses: the last part of a path, so `[[Worktree ports]]` finds
/// `atolye/svc-demo/worktree-ports` from anywhere in the vault.
pub fn leaf_of(slug: &str) -> &str {
    slug.rsplit('/').next().unwrap_or(slug)
}

pub fn backlinks(notes: &[Note]) -> BTreeMap<String, Vec<String>> {
    let mut pointing: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let by_leaf: BTreeMap<&str, &str> = notes
        .iter()
        .map(|note| (leaf_of(&note.slug), note.slug.as_str()))
        .collect();

    for note in notes {
        for target in links_in(&note.body) {
            let full = by_leaf.get(target.as_str()).copied().unwrap_or(target.as_str());
            pointing
                .entry(full.to_owned())
                .or_default()
                .insert(note.slug.clone());
        }
    }

    pointing
        .into_iter()
        .map(|(slug, sources)| (slug, sources.into_iter().collect()))
        .collect()
}

/// A note as it sits on disk: front matter a person can read, then the body.
/// The first few words of a fact, for a title and a file name.
fn first_words(text: &str, most: usize) -> String {
    let flattened = text.split_whitespace().collect::<Vec<_>>();
    let taken = flattened
        .iter()
        .take(most)
        .copied()
        .collect::<Vec<_>>()
        .join(" ");

    taken.trim_end_matches(|c: char| c.is_ascii_punctuation()).to_owned()
}

pub fn render(note: &Note) -> String {
    let tags = note.tags.join(", ");
    let approved = match note.approved {
        Some(true) => "approved: true\n".to_owned(),
        Some(false) => "approved: false\n".to_owned(),
        None => String::new(),
    };
    let retired = if note.retired { "retired: true\n" } else { "" };
    let supersedes = note
        .supersedes
        .as_deref()
        .filter(|slug| !slug.trim().is_empty())
        .map(|slug| format!("supersedes: {slug}\n"))
        .unwrap_or_default();

    format!(
        "---\ntitle: {}\ntags: [{}]\nwritten_by: {}\nwritten_at: {}\n{approved}{retired}{supersedes}---\n\n{}\n",
        note.title.trim(),
        tags,
        note.written_by.trim(),
        note.written_at,
        note.body.trim(),
    )
}

/// Read a note back. A file with no front matter is still a note — the human may
/// have dropped it in — and its first heading, or its file name, is the title.
pub fn parse(slug: &str, text: &str) -> Note {
    let mut note = Note {
        slug: slug.to_owned(),
        title: slug.replace('-', " "),
        ..Note::default()
    };

    let rest = if let Some(stripped) = text.strip_prefix("---\n") {
        match stripped.find("\n---") {
            Some(end) => {
                for line in stripped[..end].lines() {
                    let Some((key, value)) = line.split_once(':') else {
                        continue;
                    };
                    let value = value.trim();

                    match key.trim() {
                        "title" if !value.is_empty() => note.title = value.to_owned(),
                        "written_by" => note.written_by = value.to_owned(),
                        "written_at" => note.written_at = value.parse().unwrap_or(0),
                        // Anything but a plain "true" is treated as not yet
                        // approved: a half-written line in someone's editor
                        // must never read as a yes.
                        "approved" => note.approved = Some(value.eq_ignore_ascii_case("true")),
                        "supersedes" if !value.is_empty() => {
                            note.supersedes = Some(value.to_owned())
                        }
                        "retired" => note.retired = value.eq_ignore_ascii_case("true"),
                        "tags" => {
                            note.tags = value
                                .trim_start_matches('[')
                                .trim_end_matches(']')
                                .split(',')
                                .map(|tag| tag.trim().to_owned())
                                .filter(|tag| !tag.is_empty())
                                .collect();
                        }
                        _ => {}
                    }
                }

                stripped[end + 4..].trim_start_matches('\n')
            }
            None => text,
        }
    } else {
        text
    };

    note.body = rest.trim().to_owned();

    if note.title == slug.replace('-', " ") {
        if let Some(heading) = note.body.lines().find_map(|line| line.strip_prefix("# ")) {
            note.title = heading.trim().to_owned();
        }
    }

    note.links = links_in(&note.body);
    note
}

/// How well a note answers a question, by the words in it. The title counts for
/// more than the body, because a note named for the thing is usually the note.
pub fn score(note: &Note, query: &str) -> usize {
    let wanted: Vec<String> = query
        .split_whitespace()
        .map(|word| word.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .filter(|word| word.len() > 2)
        .collect();

    if wanted.is_empty() {
        return 0;
    }

    let title = note.title.to_lowercase();
    let body = note.body.to_lowercase();
    let tags = note.tags.join(" ").to_lowercase();

    wanted
        .iter()
        .map(|word| {
            let mut points = 0;
            if title.contains(word) {
                points += 3;
            }
            if tags.contains(word) {
                points += 2;
            }
            if body.contains(word) {
                points += 1;
            }
            points
        })
        .sum()
}

/// Where the vault lives.
///
/// A notes folder belongs where a person keeps notes, not buried in an app's
/// data directory: the point of markdown with `[[links]]` is that Obsidian or
/// anything else opens it. So the default is `~/Documents/Agentland`,
/// `AGENTLAND_VAULT_DIR` overrides it, and a machine with no home falls back to
/// the data directory rather than failing to start.
pub fn vault_root(data_dir: &Path, home: Option<&Path>, wanted: Option<&str>) -> PathBuf {
    if let Some(chosen) = wanted.map(str::trim).filter(|value| !value.is_empty()) {
        let path = Path::new(chosen);
        return match (path.strip_prefix("~"), home) {
            (Ok(rest), Some(home)) => home.join(rest),
            _ => path.to_path_buf(),
        };
    }

    match home {
        Some(home) => home.join("Documents").join("Agentland"),
        None => data_dir.join("vault"),
    }
}

fn notes_in(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut folders = vec![root.to_path_buf()];

    while let Some(folder) = folders.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };

        for path in entries.flatten().map(|entry| entry.path()) {
            let name = path.file_name().map(|name| name.to_string_lossy().to_string());
            if path.is_dir() {
                // A note tool keeps its own settings in the vault; that is not
                // the crew's knowledge.
                if !name.map(|name| name.starts_with('.')).unwrap_or(false) {
                    folders.push(path);
                }
            } else if path.extension().map(|kind| kind == "md").unwrap_or(false) {
                found.push(path);
            }
        }
    }

    found
}

/// Move notes to where the vault now lives, but never over the top of notes that
/// are already there: a folder someone is using is not a place to spill into.
pub fn move_notes(from: &Path, to: &Path) -> Result<usize> {
    if from == to || !from.is_dir() || !notes_in(to).is_empty() {
        return Ok(0);
    }

    std::fs::create_dir_all(to)?;
    let mut moved = 0;

    for path in notes_in(from) {
        let Some(name) = path.file_name() else {
            continue;
        };

        std::fs::rename(&path, to.join(name)).or_else(|_| {
            // A rename across filesystems fails; copying still moves the note.
            std::fs::copy(&path, to.join(name)).and_then(|_| std::fs::remove_file(&path))
        })?;
        moved += 1;
    }

    Ok(moved)
}

pub struct Vault {
    root: PathBuf,
}

impl Vault {
    /// Open the vault where it belongs, bringing any notes from where it used to
    /// live with it.
    pub fn open(data_dir: &Path) -> Result<Self> {
        let home = crate::exec::home();
        let wanted = std::env::var("AGENTLAND_VAULT_DIR").ok();
        let root = vault_root(data_dir, home.as_deref(), wanted.as_deref());

        std::fs::create_dir_all(&root)?;

        let older = data_dir.join("vault");
        match move_notes(&older, &root) {
            Ok(moved) if moved > 0 => {
                tracing::info!(?older, ?root, moved, "moved the vault to where notes belong");
            }
            Err(error) => tracing::warn!(%error, "could not move the old vault"),
            _ => {}
        }

        Ok(Self { root })
    }

    pub fn open_at(root: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn path_of(&self, slug: &str) -> PathBuf {
        self.root.join(format!("{slug}.md"))
    }

    fn slug_of(&self, path: &Path) -> Option<String> {
        let relative = path.strip_prefix(&self.root).ok()?;
        let text = relative.to_string_lossy().to_string();
        Some(text.trim_end_matches(".md").replace('\\', "/"))
    }

    pub fn list(&self) -> Vec<Note> {
        let mut notes: Vec<Note> = notes_in(&self.root)
            .into_iter()
            .filter_map(|path| {
                let slug = self.slug_of(&path)?;
                let text = std::fs::read_to_string(&path).ok()?;
                Some(parse(&slug, &text))
            })
            .collect();

        let pointing = backlinks(&notes);
        for note in &mut notes {
            note.backlinks = pointing.get(&note.slug).cloned().unwrap_or_default();
        }

        notes.sort_by(|first, second| first.slug.cmp(&second.slug));
        notes
    }

    pub fn get(&self, slug: &str) -> Option<Note> {
        let text = std::fs::read_to_string(self.path_of(slug)).ok()?;
        let mut note = parse(slug, &text);
        note.backlinks = backlinks(&self.list())
            .get(slug)
            .cloned()
            .unwrap_or_default();
        Some(note)
    }

    /// Write a note. A title with no letters or digits has no file name, and a
    /// note nobody can find again is not worth writing.
    pub fn write(
        &self,
        scope: &Scope,
        title: &str,
        body: &str,
        tags: Vec<String>,
        by: &str,
        now: u64,
    ) -> Result<Note> {
        let leaf = slug_for(title);
        if leaf.is_empty() {
            bail!("a note needs a title with letters or digits in it");
        }

        let folder = scope.folder();
        let slug = format!("{folder}/{leaf}");
        std::fs::create_dir_all(self.root.join(&folder))?;

        let note = Note {
            slug: slug.clone(),
            title: title.trim().to_owned(),
            tags,
            written_by: by.to_owned(),
            written_at: now,
            body: body.trim().to_owned(),
            approved: None,
            retired: false,
            supersedes: None,
            links: links_in(body),
            backlinks: Vec::new(),
        };

        std::fs::write(self.path_of(&slug), render(&note))?;
        Ok(self.get(&slug).unwrap_or(note))
    }

    /// The line below which the map is Agentland's to write. Anything a person
    /// or the commander writes above it is kept, because a map with a sentence
    /// of judgement on top is worth more than a list of links.
    const MAP_MARK: &str = "<!-- agentland keeps the list below this line -->";

    /// Write an index in every folder that holds notes, and one at the root that
    /// points at the folders. The list is regenerated; the words above it are
    /// left exactly as they were.
    /// The folder a scope keeps its memories in.
    ///
    /// A memory is a note, but a one-line fact and a page of prose do not read
    /// well in the same list, so memories sit in a `memory/` folder under their
    /// scope. Obsidian shows them beside everything else; the crew's own tools
    /// tell them apart by the `approved` line, not by the path.
    fn memory_folder(scope: &Scope) -> String {
        let folder = scope.folder();
        if folder.is_empty() {
            "memory".to_owned()
        } else {
            format!("{folder}/memory")
        }
    }

    /// Write down something the crew should be told, once a person says yes.
    ///
    /// The title is the fact itself, trimmed to something a file name can carry,
    /// so the vault reads as sentences rather than as `m17.md`.
    pub fn write_memory(
        &self,
        scope: &Scope,
        text: &str,
        by: &str,
        supersedes: Option<&str>,
        now: u64,
    ) -> Result<Note> {
        let text = text.trim();
        if text.is_empty() {
            bail!("a memory needs something to remember");
        }

        let leaf = slug_for(&first_words(text, 9));
        if leaf.is_empty() {
            bail!("a memory needs letters or digits in it");
        }

        let folder = Self::memory_folder(scope);
        std::fs::create_dir_all(self.root.join(&folder))?;
        let slug = format!("{folder}/{leaf}");

        let note = Note {
            slug: slug.clone(),
            title: first_words(text, 9),
            tags: vec!["memory".to_owned()],
            written_by: by.to_owned(),
            written_at: now,
            body: text.to_owned(),
            approved: Some(false),
            retired: false,
            supersedes: supersedes
                .map(str::trim)
                .filter(|slug| !slug.is_empty())
                .map(str::to_owned),
            links: links_in(text),
            backlinks: Vec::new(),
        };

        std::fs::write(self.path_of(&slug), render(&note))?;
        Ok(self.get(&slug).unwrap_or(note))
    }

    /// Every note that is a memory: proposed, or approved and in use.
    pub fn memories(&self) -> Vec<Note> {
        self.list()
            .into_iter()
            .filter(|note| note.approved.is_some())
            .collect()
    }

    /// Say yes or no to a memory. Saying no leaves the note where it is — the
    /// crew wrote it down, and forgetting is a separate act.
    pub fn set_approved(&self, slug: &str, approved: bool) -> Result<Note> {
        let mut note = self
            .get(slug)
            .ok_or_else(|| anyhow!("no note called {slug}"))?;

        // Taking back something that was in force is a decision and reads as
        // one; a memory that was never approved is still a question. Approving
        // again clears the mark, because it is in force once more.
        note.retired = match (note.approved, approved) {
            (Some(true), false) => true,
            (_, true) => false,
            _ => note.retired,
        };
        note.approved = Some(approved);

        std::fs::write(self.path_of(slug), render(&note))?;
        Ok(note)
    }

    pub fn reindex(&self, now: u64) -> Result<usize> {
        let notes = self.list();
        let mut folders: BTreeMap<String, Vec<Note>> = BTreeMap::new();

        for note in notes {
            if leaf_of(&note.slug) == "index" {
                continue;
            }

            let folder = match note.slug.rsplit_once('/') {
                Some((folder, _)) => folder.to_owned(),
                None => String::new(),
            };
            folders.entry(folder).or_default().push(note);
        }

        // A workspace holding only projects still deserves a map, and so does the
        // root: every folder on the way to a note gets one.
        for folder in folders.keys().cloned().collect::<Vec<_>>() {
            let mut parts: Vec<&str> = folder.split('/').collect();
            while !parts.is_empty() {
                parts.pop();
                folders.entry(parts.join("/")).or_default();
            }
        }
        folders.entry(String::new()).or_default();

        let mut written = 0;

        for (folder, mut held) in folders.clone() {
            held.sort_by(|first, second| first.title.cmp(&second.title));
            let mut lines = Vec::new();

            if !held.is_empty() {
                lines.push(format!("## Notes here ({})", held.len()));
            }

            for note in &held {
                let tags = if note.tags.is_empty() {
                    String::new()
                } else {
                    format!(" — *{}*", note.tags.join(", "))
                };
                lines.push(format!("- [[{}]]{tags}", note.title));
            }

            // Only the places directly below this one: a map that lists every
            // descendant is the heap it was meant to replace.
            let under = if folder.is_empty() {
                String::new()
            } else {
                format!("{folder}/")
            };

            let mut children: Vec<String> = folders
                .keys()
                .filter(|other| !other.is_empty() && other.starts_with(&under) && **other != folder)
                .filter_map(|other| {
                    let rest = &other[under.len()..];
                    rest.split('/').next().map(|name| format!("{under}{name}"))
                })
                .collect();
            children.sort();
            children.dedup();

            if !children.is_empty() {
                lines.push(String::new());
                lines.push("## Places".to_owned());
                for child in children {
                    let name = child.rsplit('/').next().unwrap_or(&child).replace('-', " ");
                    lines.push(format!("- [[{child}/index|{name}]]"));
                }
            }

            self.write_index(&folder, &lines.join("\n"), now)?;
            written += 1;
        }

        Ok(written)
    }

    fn write_index(&self, folder: &str, list: &str, now: u64) -> Result<()> {
        let slug = if folder.is_empty() {
            "index".to_owned()
        } else {
            format!("{folder}/index")
        };

        let path = self.path_of(&slug);
        let held = std::fs::read_to_string(&path).unwrap_or_default();

        let above = match held.split_once(Self::MAP_MARK) {
            Some((above, _)) => above.trim_end().to_owned(),
            None => {
                let title = if folder.is_empty() {
                    "The crew's vault".to_owned()
                } else {
                    folder.rsplit('/').next().unwrap_or(folder).replace('-', " ")
                };

                let note = Note {
                    slug: slug.clone(),
                    title: format!("{title} — index"),
                    tags: vec!["index".to_owned()],
                    written_by: "agentland".to_owned(),
                    written_at: now,
                    body: String::new(),
                    approved: None,
                    retired: false,
                    supersedes: None,
                    links: Vec::new(),
                    backlinks: Vec::new(),
                };

                render(&note).trim_end().to_owned()
            }
        };

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(path, format!("{above}\n\n{}\n\n{list}\n", Self::MAP_MARK))?;
        Ok(())
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<Note> {
        let mut hits: Vec<(usize, Note)> = self
            .list()
            .into_iter()
            .map(|note| (score(&note, query), note))
            .filter(|(points, _)| *points > 0)
            .collect();

        hits.sort_by(|first, second| second.0.cmp(&first.0).then(first.1.slug.cmp(&second.1.slug)));
        hits.into_iter().take(limit).map(|(_, note)| note).collect()
    }

    pub fn forget(&self, slug: &str) -> Result<()> {
        let path = self.path_of(slug);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn code_is_quoted_not_linked() {
        let body = [
            "It points at [[The port contract]].",
            "",
            "```js",
            "for (const [path, method] of [[\"/version\", \"POST\"]]) {}",
            "```",
            "",
            "And inline `[[not a link]]` either.",
        ]
        .join("\n");

        assert_eq!(super::links_in(&body), vec!["the-port-contract".to_owned()]);
    }

    #[test]
    fn a_fence_that_is_never_closed_does_not_swallow_the_rest() {
        let body = "See [[Worktree ports]].\n\n```\nleft open";

        assert_eq!(super::links_in(body), vec!["worktree-ports".to_owned()]);
    }

    use super::*;

    fn note(slug: &str, title: &str, body: &str) -> Note {
        Note {
            slug: slug.to_owned(),
            title: title.to_owned(),
            body: body.to_owned(),
            links: links_in(body),
            ..Note::default()
        }
    }

    #[test]
    fn a_title_becomes_a_file_name_a_person_would_recognise() {
        assert_eq!(slug_for("The port contract"), "the-port-contract");
        assert_eq!(slug_for("  /health, proved!  "), "health-proved");
        assert_eq!(slug_for("!!!"), "");
    }

    #[test]
    fn a_body_names_the_notes_it_points_at() {
        let body = "PORT is read from the env, see [[The port contract]] and [[worktree ports|ports]].";
        assert_eq!(links_in(body), vec!["the-port-contract", "worktree-ports"]);
    }

    #[test]
    fn an_unfinished_link_does_not_swallow_the_rest_of_the_note() {
        assert_eq!(links_in("half a link [[open"), Vec::<String>::new());
        assert_eq!(links_in("[[one]] then [[open"), vec!["one"]);
    }

    #[test]
    fn the_same_note_named_twice_is_still_one_link() {
        assert_eq!(links_in("[[a note]] and [[A Note]]"), vec!["a-note"]);
    }

    #[test]
    fn who_points_at_whom() {
        let notes = vec![
            note("ports", "Ports", "see [[the port contract]]"),
            note("health", "Health", "also [[the port contract]]"),
            note("the-port-contract", "The port contract", "no links here"),
        ];

        let pointing = backlinks(&notes);
        assert_eq!(pointing.get("the-port-contract").unwrap(), &vec!["health".to_owned(), "ports".to_owned()]);
        assert!(pointing.get("ports").is_none());
    }

    #[test]
    fn a_note_survives_a_round_trip_through_the_file() {
        let written = Note {
            approved: None,
            retired: false,
            supersedes: None,
            slug: "the-port-contract".into(),
            title: "The port contract".into(),
            tags: vec!["ports".into(), "svc-demo".into()],
            written_by: "ada".into(),
            written_at: 1_700_000_000,
            body: "The dev server reads PORT from the env. See [[worktree ports]].".into(),
            links: vec!["worktree-ports".into()],
            backlinks: Vec::new(),
        };

        let read = parse("the-port-contract", &render(&written));
        assert_eq!(read.title, written.title);
        assert_eq!(read.tags, written.tags);
        assert_eq!(read.written_by, "ada");
        assert_eq!(read.written_at, 1_700_000_000);
        assert_eq!(read.body, written.body);
        assert_eq!(read.links, vec!["worktree-ports"]);
    }

    #[test]
    fn a_file_a_human_dropped_in_is_still_a_note() {
        let read = parse("stray-thought", "# What the reviewer wants\n\nSmall commits.");
        assert_eq!(read.title, "What the reviewer wants");
        assert_eq!(read.body, "# What the reviewer wants\n\nSmall commits.");
        assert!(read.tags.is_empty());
    }

    #[test]
    fn a_note_named_for_the_thing_beats_one_that_merely_mentions_it() {
        let named = note("ports", "Worktree ports", "each worktree gets a listener");
        let mentions = note("other", "Morning sweep", "check the ports if a worktree fails");

        assert!(score(&named, "worktree ports") > score(&mentions, "worktree ports"));
        assert_eq!(score(&named, "a of the"), 0, "short words are noise");
    }

    #[test]
    fn a_vault_keeps_what_it_is_given_and_finds_it_again() {
        let home = std::env::temp_dir().join(format!("agentland-vault-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let vault = Vault::open_at(home.clone()).expect("a vault");

        let project = Scope::Project {
            workspace: "atolye".into(),
            project: "svc-demo".into(),
        };

        vault
            .write(
                &project,
                "The port contract",
                "PORT comes from the env. See [[worktree ports]].",
                vec!["ports".into()],
                "ada",
                1_700_000_000,
            )
            .expect("written");
        vault
            .write(
                &project,
                "Worktree ports",
                "One listener per worktree.",
                vec![],
                "x",
                1_700_000_001,
            )
            .expect("written");

        let found = vault.search("port contract", 5);
        assert_eq!(
            found.first().map(|note| note.slug.as_str()),
            Some("atolye/svc-demo/the-port-contract"),
        );

        let pointed_at = vault.get("atolye/svc-demo/worktree-ports").expect("the note");
        assert_eq!(
            pointed_at.backlinks,
            vec!["atolye/svc-demo/the-port-contract".to_owned()],
            "a link finds its target across folders",
        );

        vault.forget("atolye/svc-demo/worktree-ports").expect("forgotten");
        assert!(vault.get("atolye/svc-demo/worktree-ports").is_none());

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn the_vault_lives_where_a_person_keeps_notes() {
        let data = Path::new("/var/app/data");
        let home = Path::new("/home/someone");

        assert_eq!(
            vault_root(data, Some(home), None),
            Path::new("/home/someone/Documents/Agentland"),
        );
        assert_eq!(
            vault_root(data, Some(home), Some("~/Notes/Crew")),
            Path::new("/home/someone/Notes/Crew"),
        );
        assert_eq!(
            vault_root(data, Some(home), Some("/mnt/shared/vault")),
            Path::new("/mnt/shared/vault"),
        );
        assert_eq!(
            vault_root(data, None, None),
            Path::new("/var/app/data/vault"),
            "a machine with no home still starts",
        );
        assert_eq!(
            vault_root(data, Some(home), Some("   ")),
            Path::new("/home/someone/Documents/Agentland"),
            "an empty setting is not a path",
        );
    }

    #[test]
    fn notes_follow_the_vault_when_it_moves_but_never_spill_into_a_folder_in_use() {
        let base = std::env::temp_dir().join(format!("agentland-move-{}", std::process::id()));
        let older = base.join("old");
        let newer = base.join("new");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&older).expect("old");

        std::fs::write(older.join("a-note.md"), "---\ntitle: A note\n---\n\nbody").expect("write");
        std::fs::write(older.join("notes.txt"), "not a note").expect("write");

        assert_eq!(move_notes(&older, &newer).expect("moved"), 1);
        assert!(newer.join("a-note.md").exists());
        assert!(!older.join("a-note.md").exists(), "the note moved, it was not copied");
        assert!(older.join("notes.txt").exists(), "only notes move");

        std::fs::write(older.join("second.md"), "another").expect("write");
        assert_eq!(
            move_notes(&older, &newer).expect("second attempt"),
            0,
            "a folder with notes in it is left alone",
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_note_with_no_name_is_refused_rather_than_written_somewhere_odd() {
        let home = std::env::temp_dir().join(format!("agentland-vault-empty-{}", std::process::id()));
        let vault = Vault::open_at(home.clone()).expect("a vault");
        assert!(vault.write(&Scope::Shared, "   ", "body", vec![], "x", 0).is_err());
        let _ = std::fs::remove_dir_all(&home);
    }
}

#[cfg(test)]
mod scope_tests {
    use super::*;

    #[test]
    fn a_scope_is_read_the_way_an_agent_writes_it() {
        assert_eq!(Scope::parse("shared", None), Scope::Shared);
        assert_eq!(Scope::parse("", None), Scope::Shared);
        assert_eq!(
            Scope::parse("workspace:Atölye", None),
            Scope::Workspace("atölye".into()),
            "a letter is a letter, whatever alphabet it comes from",
        );
        assert_eq!(
            Scope::parse("project:atolye/svc demo", None),
            Scope::Project { workspace: "atolye".into(), project: "svc-demo".into() },
        );
        assert_eq!(
            Scope::parse("agentland-svc-demo", Some("atolye")),
            Scope::Project { workspace: "atolye".into(), project: "agentland-svc-demo".into() },
            "a bare project id lands in the workspace being looked at",
        );
    }

    #[test]
    fn every_scope_has_a_folder_of_its_own() {
        assert_eq!(Scope::Shared.folder(), "shared");
        assert_eq!(Scope::Workspace("atolye".into()).folder(), "atolye");
        assert_eq!(
            Scope::Project { workspace: "atolye".into(), project: "svc-demo".into() }.folder(),
            "atolye/svc-demo",
        );
    }

    #[test]
    fn the_index_lists_what_is_there_and_keeps_what_a_person_wrote() {
        let home = std::env::temp_dir().join(format!("agentland-index-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let vault = Vault::open_at(home.clone()).expect("a vault");

        let project = Scope::Project { workspace: "atolye".into(), project: "svc-demo".into() };
        vault.write(&project, "The port contract", "PORT from the env", vec!["ports".into()], "ada", 10).expect("write");
        vault.write(&Scope::Shared, "How we write notes", "short, linked", vec![], "x", 11).expect("write");

        vault.reindex(12).expect("reindex");

        let map = vault.get("atolye/svc-demo/index").expect("the project's map");
        assert!(map.body.contains("[[The port contract]]"), "it lists what is there: {}", map.body);
        assert!(map.body.contains("*ports*"), "with what the note is about");

        let root = vault.get("index").expect("the root map");
        assert!(root.body.contains("[[atolye/index"), "the root points at its places: {}", root.body);
        assert!(root.body.contains("[[shared/index"));

        // A person writes a sentence at the top; the list is rebuilt under it.
        let path = home.join("atolye/svc-demo/index.md");
        let held = std::fs::read_to_string(&path).expect("read");
        let above = held.split(Vault::MAP_MARK).next().unwrap().to_owned();
        std::fs::write(&path, format!("{above}Start with the port contract.\n\n{}\n\nold list\n", Vault::MAP_MARK)).expect("write");

        vault.write(&project, "Health endpoint", "answers 200", vec![], "x", 13).expect("write");
        vault.reindex(14).expect("reindex");

        let again = std::fs::read_to_string(&path).expect("read");
        assert!(again.contains("Start with the port contract."), "the words above the line survive");
        assert!(again.contains("[[Health endpoint]]"), "the list below it is rebuilt");
        assert!(!again.contains("old list"), "and the old list is gone");

        let _ = std::fs::remove_dir_all(&home);
    }
}
