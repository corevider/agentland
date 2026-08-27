use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

const BUILTIN: &[(&str, &str)] = &[
    (
        "commanding-a-crew",
        include_str!("../../../skills/commanding-a-crew/SKILL.md"),
    ),
    (
        "systematic-debugging",
        include_str!("../../../skills/systematic-debugging/SKILL.md"),
    ),
    (
        "test-driven-development",
        include_str!("../../../skills/test-driven-development/SKILL.md"),
    ),
    ("code-review", include_str!("../../../skills/code-review/SKILL.md")),
    (
        "architecture-diagrams",
        include_str!("../../../skills/architecture-diagrams/SKILL.md"),
    ),
];

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub when_to_use: String,
    pub body: String,
    pub builtin: bool,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct State {
    #[serde(default)]
    installs: BTreeMap<String, BTreeSet<String>>,
}

pub struct SkillLibrary {
    skills: Mutex<BTreeMap<String, Skill>>,
    state: Mutex<State>,
    data_dir: PathBuf,
}

impl SkillLibrary {
    pub fn new(data_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&data_dir);
        let data_dir = fs::canonicalize(&data_dir).unwrap_or(data_dir);
        let state = crate::db::load_state(&data_dir, "skills");

        let library = Self {
            skills: Mutex::new(BTreeMap::new()),
            state: Mutex::new(state),
            data_dir,
        };

        library.reload();
        library
    }

    pub fn reload(&self) {
        let mut skills = BTreeMap::new();

        for (id, manifest) in BUILTIN {
            match parse_manifest(id, manifest, true) {
                Ok(skill) => {
                    skills.insert(skill.id.clone(), skill);
                }
                Err(error) => tracing::error!(%id, %error, "a built-in skill is malformed"),
            }
        }

        for skill in read_folder(&self.skills_dir()) {
            skills.insert(skill.id.clone(), skill);
        }

        *self.skills.lock() = skills;
    }

    pub fn skills_dir(&self) -> PathBuf {
        self.data_dir.join("skills")
    }

    pub fn list(&self) -> Vec<Skill> {
        self.skills.lock().values().cloned().collect()
    }

    pub fn get(&self, id: &str) -> Option<Skill> {
        self.skills.lock().get(id).cloned()
    }

    pub fn installed_for(&self, agent_id: &str) -> Vec<Skill> {
        let state = self.state.lock();
        let Some(ids) = state.installs.get(agent_id) else {
            return Vec::new();
        };

        let skills = self.skills.lock();
        ids.iter().filter_map(|id| skills.get(id).cloned()).collect()
    }

    pub fn install(&self, agent_id: &str, skill_id: &str) -> Result<Vec<Skill>> {
        if self.get(skill_id).is_none() {
            bail!("there is no skill called {skill_id}");
        }

        {
            let mut state = self.state.lock();
            state
                .installs
                .entry(agent_id.to_owned())
                .or_default()
                .insert(skill_id.to_owned());
            self.persist(&state);
        }

        Ok(self.installed_for(agent_id))
    }

    pub fn uninstall(&self, agent_id: &str, skill_id: &str) -> Vec<Skill> {
        {
            let mut state = self.state.lock();
            if let Some(ids) = state.installs.get_mut(agent_id) {
                ids.remove(skill_id);
                if ids.is_empty() {
                    state.installs.remove(agent_id);
                }
            }
            self.persist(&state);
        }

        self.installed_for(agent_id)
    }

    pub fn forget_agent(&self, agent_id: &str) {
        let mut state = self.state.lock();
        if state.installs.remove(agent_id).is_some() {
            self.persist(&state);
        }
    }

    pub fn brief_section(&self, agent_id: &str) -> Option<String> {
        let installed = self.installed_for(agent_id);
        if installed.is_empty() {
            return None;
        }

        let mut section = String::from("\n\nSkills you have been given:");
        for skill in installed {
            section.push_str(&format!(
                "\n\n## {}\n{}\nUse it when: {}\n\n{}",
                skill.name,
                skill.description,
                skill.when_to_use,
                skill.body.trim()
            ));
        }

        Some(section)
    }

    pub fn write(&self, id: &str, manifest: &str) -> Result<Skill> {
        let id = slug(id);
        if id.is_empty() {
            bail!("a skill needs a name");
        }
        if BUILTIN.iter().any(|(builtin, _)| *builtin == id) {
            bail!("{id} is a built-in skill; copy it under another name to change it");
        }

        let skill = parse_manifest(&id, manifest, false)?;
        let folder = self.skills_dir().join(&id);
        fs::create_dir_all(&folder)?;
        fs::write(folder.join("SKILL.md"), manifest)?;

        self.skills.lock().insert(skill.id.clone(), skill.clone());
        Ok(skill)
    }

    pub fn remove(&self, id: &str) -> Result<()> {
        let Some(skill) = self.get(id) else {
            bail!("there is no skill called {id}");
        };
        if skill.builtin {
            bail!("{id} is built in and cannot be removed");
        }

        let folder = self.skills_dir().join(id);
        if folder.exists() {
            fs::remove_dir_all(&folder)?;
        }

        self.skills.lock().remove(id);

        let mut state = self.state.lock();
        state.installs.retain(|_, ids| {
            ids.remove(id);
            !ids.is_empty()
        });
        self.persist(&state);

        Ok(())
    }

    fn persist(&self, state: &State) {
        crate::db::save_state(&self.data_dir, "skills", state);
    }
}

fn read_folder(root: &Path) -> Vec<Skill> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };

    let mut skills = Vec::new();
    for entry in entries.flatten() {
        let manifest_path = entry.path().join("SKILL.md");
        let Ok(manifest) = fs::read_to_string(&manifest_path) else {
            continue;
        };

        let id = entry.file_name().to_string_lossy().to_string();
        match parse_manifest(&slug(&id), &manifest, false) {
            Ok(skill) => skills.push(skill),
            Err(error) => {
                tracing::warn!(path = %manifest_path.display(), %error, "skipping a malformed skill")
            }
        }
    }

    skills
}

fn parse_manifest(id: &str, manifest: &str, builtin: bool) -> Result<Skill> {
    let trimmed = manifest.trim_start();
    let Some(rest) = trimmed.strip_prefix("---") else {
        bail!("a skill manifest starts with a --- header");
    };
    let Some(end) = rest.find("\n---") else {
        bail!("the --- header is never closed");
    };

    let mut fields: BTreeMap<String, String> = BTreeMap::new();
    for line in rest[..end].lines() {
        if let Some((key, value)) = line.split_once(':') {
            fields.insert(key.trim().to_lowercase(), value.trim().to_owned());
        }
    }

    let body = rest[end + 4..].trim_start_matches('\n').to_owned();
    let name = fields
        .get("name")
        .cloned()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| id.replace('-', " "));

    if body.trim().is_empty() {
        bail!("a skill with no instructions teaches nothing");
    }

    Ok(Skill {
        id: id.to_owned(),
        name,
        description: fields.get("description").cloned().unwrap_or_default(),
        when_to_use: fields.get("when_to_use").cloned().unwrap_or_default(),
        body,
        builtin,
    })
}

fn slug(value: &str) -> String {
    let mut out = String::new();
    let mut dash = false;

    for character in value.trim().chars() {
        if character.is_ascii_alphanumeric() {
            out.push(character.to_ascii_lowercase());
            dash = false;
        } else if !out.is_empty() && !dash {
            out.push('-');
            dash = true;
        }
    }

    out.trim_end_matches('-').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library(name: &str) -> SkillLibrary {
        let dir = std::env::temp_dir().join(format!("agentland-skills-{name}"));
        let _ = fs::remove_dir_all(&dir);
        SkillLibrary::new(dir)
    }

    #[test]
    fn the_built_in_skills_all_parse() {
        let library = library("builtin");
        let skills = library.list();

        assert_eq!(skills.len(), BUILTIN.len());
        for skill in skills {
            assert!(skill.builtin);
            assert!(!skill.name.is_empty(), "{} has no name", skill.id);
            assert!(!skill.description.is_empty(), "{} has no description", skill.id);
            assert!(!skill.when_to_use.is_empty(), "{} never says when to use it", skill.id);
            assert!(skill.body.len() > 200, "{} is too thin to be useful", skill.id);
        }
    }

    #[test]
    fn a_brief_carries_only_the_skills_an_agent_was_given() {
        let library = library("brief");
        library.install("a1", "code-review").expect("install");

        let brief = library.brief_section("a1").expect("a section");
        assert!(brief.contains("Code review"));
        assert!(!brief.contains("Systematic debugging"));
        assert!(brief.contains("stop at the first pass"), "the body is included");

        assert_eq!(library.brief_section("a2"), None, "another agent gets nothing");
    }

    #[test]
    fn a_skill_that_does_not_exist_cannot_be_installed() {
        let library = library("missing");
        let error = library.install("a1", "telepathy").expect_err("should refuse");
        assert!(error.to_string().contains("telepathy"));
    }

    #[test]
    fn a_written_skill_survives_a_reload_and_can_be_removed() {
        let library = library("write");
        let manifest = "---\nname: Ship checklist\ndescription: What to do before a release.\nwhen_to_use: Cutting a release.\n---\nRun the tests. Read the diff. Tag it.\n";

        let written = library.write("Ship Checklist!", manifest).expect("write");
        assert_eq!(written.id, "ship-checklist");
        assert!(!written.builtin);

        library.install("a1", "ship-checklist").expect("install");
        library.reload();
        assert!(library.get("ship-checklist").is_some(), "read back from disk");
        assert!(library.brief_section("a1").expect("section").contains("Tag it."));

        library.remove("ship-checklist").expect("remove");
        assert!(library.get("ship-checklist").is_none());
        assert_eq!(library.brief_section("a1"), None, "the install is cleaned up too");
    }

    #[test]
    fn a_built_in_skill_cannot_be_overwritten_or_removed() {
        let library = library("protected");
        let manifest = "---\nname: Hijack\n---\nDo something else entirely.\n";

        assert!(library.write("code-review", manifest).is_err());
        assert!(library.remove("code-review").is_err());
        assert!(library.get("code-review").expect("still there").body.contains("Correctness"));
    }

    #[test]
    fn a_manifest_without_a_header_or_a_body_is_refused() {
        assert!(parse_manifest("x", "just some prose", false).is_err());
        assert!(parse_manifest("x", "---\nname: Empty\n---\n\n", false).is_err());
        assert!(parse_manifest("x", "---\nname: Unclosed\n", false).is_err());
    }

    #[test]
    fn a_skill_with_no_name_falls_back_to_its_folder() {
        let skill = parse_manifest("port-hygiene", "---\ndescription: d\n---\nBody.\n", false)
            .expect("parse");
        assert_eq!(skill.name, "port hygiene");
    }
}
