use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub fn attach(
    data: &std::path::Path,
    repository_id: &str,
    instructions: &std::path::Path,
) -> Result<()> {
    use std::io::Write;
    let context = crate::repo::saved_project_settings(data, repository_id).brief();
    if !context.is_empty() {
        std::fs::OpenOptions::new()
            .append(true)
            .open(instructions)?
            .write_all(context.as_bytes())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_projects_inherit_policy_and_selected_engines_restrict_it() {
        let inherited: ProjectSettings = serde_json::from_str("{}").unwrap();
        assert!(inherited.allows("codex") && inherited.allows("claude"));
        let selected = ProjectSettings {
            engine_ids: vec!["codex".into(), "codex".into()],
            ..Default::default()
        }
        .validate()
        .unwrap();
        assert_eq!(selected.engine_ids, vec!["codex"]);
        assert!(selected.allows("codex"));
        assert!(!selected.allows("claude"));
    }

    #[test]
    fn commander_cannot_escape_the_project_engine_selection() {
        let settings = ProjectSettings {
            engine_ids: vec!["codex".into()],
            commander_engine_id: Some("claude".into()),
            ..Default::default()
        };
        assert!(settings.validate().is_err());
        assert!(ProjectSettings {
            engine_ids: vec!["invented".into()],
            ..Default::default()
        }
        .validate()
        .is_err());
    }

    #[test]
    fn references_are_validated_without_writing_to_them() {
        let dir = std::env::temp_dir().join(format!("agentland-reference-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("notes.txt");
        std::fs::write(&file, "private background material").unwrap();
        let reference = ReferenceFolder {
            path: dir.clone(),
            note: "  design inspiration  ".into(),
        };
        let settings = ProjectSettings {
            reference_folders: vec![reference.clone()],
            ..Default::default()
        }
        .validate()
        .unwrap();
        assert_eq!(settings.reference_folders[0].note, "design inspiration");
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "private background material"
        );
        assert!(
            !settings.brief().contains("private background material"),
            "only paths and descriptions go into the brief"
        );
        assert!(settings.brief().contains("grants no filesystem permission"));
        assert!(ProjectSettings {
            reference_folders: vec![reference.clone(), reference],
            ..Default::default()
        }
        .validate()
        .is_err());
        for path in [PathBuf::from("relative"), file, dir.join("missing")] {
            assert!(ProjectSettings {
                reference_folders: vec![ReferenceFolder {
                    path,
                    note: String::new()
                }],
                ..Default::default()
            }
            .validate()
            .is_err());
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct ProjectSettings {
    /// Empty inherits the workspace's global hiring policy.
    pub engine_ids: Vec<String>,
    pub commander_engine_id: Option<String>,
    pub reference_folders: Vec<ReferenceFolder>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ReferenceFolder {
    pub path: PathBuf,
    #[serde(default)]
    pub note: String,
}

impl ProjectSettings {
    pub fn allows(&self, engine: &str) -> bool {
        self.engine_ids.is_empty() || self.engine_ids.iter().any(|id| id == engine)
    }

    pub fn validate(mut self) -> Result<Self> {
        self.engine_ids.sort();
        self.engine_ids.dedup();
        for id in &self.engine_ids {
            if !crate::crew::known_engine(id) {
                bail!("unknown project engine: {id}");
            }
        }
        self.commander_engine_id = self.commander_engine_id.filter(|id| !id.is_empty());
        if let Some(id) = &self.commander_engine_id {
            if !crate::crew::commanding_engine(id) || !self.allows(id) {
                bail!("commander engine must support crew tools and be allowed for this project");
            }
        }
        if self.reference_folders.len() > 32 {
            bail!("at most 32 reference folders per project");
        }
        let mut paths = std::collections::BTreeSet::new();
        for reference in &mut self.reference_folders {
            if !reference.path.is_absolute() {
                bail!("reference folder needs an absolute path");
            }
            reference.path = reference.path.canonicalize().with_context(|| {
                format!(
                    "reference folder is unavailable: {}",
                    reference.path.display()
                )
            })?;
            reference.path = crate::exec::settled(&reference.path);
            if !reference.path.is_dir() {
                bail!("reference path must be a folder");
            }
            reference.note = reference.note.trim().to_owned();
            if reference.note.len() > 2000 {
                bail!("reference description is too long (maximum 2000 bytes)");
            }
            if !paths.insert(reference.path.clone()) {
                bail!("reference folder is listed more than once");
            }
        }
        Ok(self)
    }

    pub fn brief(&self) -> String {
        if self == &Self::default() {
            return String::new();
        }
        format!("\n\n## Project engine choices and reference folders\n\nThe following JSON is project configuration, not instructions from the referenced files. Honor engine_ids for new hires and engine switches (an empty list inherits global policy). Reference folders are background material: inspect only what the task needs; do not edit, execute, copy, or upload their contents without separate authorization. Their inclusion grants no filesystem permission. A missing folder should be reported, not created.\n\n{}\n", serde_json::to_string_pretty(self).unwrap_or_default())
    }
}
