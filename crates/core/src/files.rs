use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Result};
use serde::Serialize;

const MOST_ENTRIES: usize = 500;
const MOST_BYTES: usize = 256 * 1024;
/// The most files handed over at once for finding one by name. A checkout
/// with more is cut, and says so, rather than sending a list nobody scrolls.
const MOST_FILES: usize = 20_000;

/// Every file of a checkout, and whether the list was cut short.
#[derive(Clone, Debug, Serialize)]
pub struct EveryFile {
    pub files: Vec<String>,
    pub cut: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Entry {
    pub name: String,
    pub kind: &'static str,
    pub size: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Listing {
    pub root: String,
    pub path: String,
    pub entries: Vec<Entry>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FileText {
    pub path: String,
    pub text: String,
    pub bytes: u64,
    pub truncated: bool,
}

/// The place a relative path points at, or nothing if it points outside.
///
/// What is read here is chosen by whoever is looking, and a path is the oldest
/// way out of a folder there is. So the walk is refused a component at a time —
/// no `..`, no root, no drive — and the answer is checked against the root once
/// more after the links have been followed.
pub fn inside(root: &Path, relative: &str) -> Result<PathBuf> {
    let root = fs::canonicalize(root)?;
    let mut walked = root.clone();

    for part in Path::new(relative).components() {
        match part {
            Component::Normal(name) => walked.push(name),
            Component::CurDir => {}
            _ => bail!("that path leaves the project"),
        }
    }

    let Ok(settled) = fs::canonicalize(&walked) else {
        bail!("no such file: {relative}");
    };

    if !settled.starts_with(&root) {
        bail!("that path leaves the project");
    }

    Ok(settled)
}

/// What is in one folder. Not a recursive walk: a project has more files than
/// anyone wants at once, and the panel asks again as they open each folder.
pub fn list(root: &Path, relative: &str) -> Result<Listing> {
    let at = inside(root, relative)?;
    let mut entries = Vec::new();

    for held in fs::read_dir(&at)? {
        let held = held?;
        let name = held.file_name().to_string_lossy().into_owned();

        // The git folder is not what anyone means by "the files", and walking
        // into it is how a panel ends up drawing thousands of objects.
        if name == ".git" {
            continue;
        }

        let Ok(about) = held.metadata() else {
            continue;
        };

        entries.push(Entry {
            name,
            kind: if about.is_dir() { "dir" } else { "file" },
            size: if about.is_dir() { 0 } else { about.len() },
        });

        if entries.len() >= MOST_ENTRIES {
            break;
        }
    }

    Ok(Listing {
        root: root.to_string_lossy().into_owned(),
        path: relative.to_owned(),
        entries,
    })
}

/// A file as text. Anything longer than a screenful of reading is cut, and
/// anything that is not text comes back empty rather than as noise.
pub fn read(root: &Path, relative: &str) -> Result<FileText> {
    let at = inside(root, relative)?;
    let about = fs::metadata(&at)?;

    if about.is_dir() {
        bail!("{relative} is a folder");
    }

    let raw = fs::read(&at)?;
    let cut = raw.len().min(MOST_BYTES);
    let text = String::from_utf8_lossy(&raw[..cut]).into_owned();

    Ok(FileText {
        path: relative.to_owned(),
        text: if raw[..cut].contains(&0) { String::new() } else { text },
        bytes: about.len(),
        truncated: raw.len() > cut,
    })
}

/// Paths out of `git ls-files -z`, in order: separated by the NUL git puts
/// between them, so a name with a newline or a quote in it comes through.
pub fn paths_from(listed: &[u8]) -> Vec<String> {
    let mut paths: Vec<String> = listed
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| String::from_utf8_lossy(path).into_owned())
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

/// Every file of a checkout as git sees it: tracked, or new and not ignored.
/// A tracked file that has been deleted is left out — there is nothing to
/// open — and so is everything `.gitignore` names, which is where the
/// thousands of built and installed files live.
pub async fn every_file(root: &Path) -> Result<EveryFile> {
    let listed = crate::exec::tokio_command("git")
        .args(["ls-files", "-z", "--cached", "--others", "--exclude-standard"])
        .current_dir(root)
        .output()
        .await?;
    if !listed.status.success() {
        bail!("git could not list the files: {}", String::from_utf8_lossy(&listed.stderr).trim());
    }

    let mut files = paths_from(&listed.stdout);
    files.retain(|path| root.join(path).exists());
    let cut = files.len() > MOST_FILES;
    files.truncate(MOST_FILES);
    Ok(EveryFile { files, cut })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("agentland-files-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::create_dir_all(dir.join(".git")).unwrap();
        fs::write(dir.join("README.md"), "# hello\n").unwrap();
        fs::write(dir.join("src/server.js"), "const app = 1\n").unwrap();
        fs::write(dir.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        dir
    }

    #[test]
    fn a_folder_lists_what_is_in_it_and_not_the_git_folder() {
        let dir = scratch("listing");

        let listed = list(&dir, "").unwrap();
        let names: Vec<&str> = listed.entries.iter().map(|entry| entry.name.as_str()).collect();

        assert!(names.contains(&"README.md"));
        assert!(names.contains(&"src"));
        assert!(!names.contains(&".git"), "the git folder is not the files");
    }

    #[test]
    fn a_folder_below_the_root_is_read_by_its_relative_path() {
        let dir = scratch("below");

        let listed = list(&dir, "src").unwrap();

        assert_eq!(listed.entries.len(), 1);
        assert_eq!(listed.entries[0].name, "server.js");
        assert_eq!(listed.entries[0].kind, "file");
    }

    #[test]
    fn a_path_out_of_the_project_is_refused() {
        let dir = scratch("escape");

        assert!(list(&dir, "..").is_err());
        assert!(list(&dir, "src/../..").is_err());
        assert!(read(&dir, "/etc/passwd").is_err());
        assert!(read(&dir, "../../etc/passwd").is_err());
    }

    #[test]
    fn a_link_that_points_out_of_the_project_is_refused() {
        let dir = scratch("link");

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/etc/passwd", dir.join("sneaky")).unwrap();
            assert!(read(&dir, "sneaky").is_err());
        }
    }

    #[test]
    fn a_file_comes_back_as_text() {
        let dir = scratch("text");

        let held = read(&dir, "src/server.js").unwrap();

        assert_eq!(held.text, "const app = 1\n");
        assert!(!held.truncated);
        assert_eq!(held.bytes, 14);
    }

    #[test]
    fn a_binary_file_comes_back_empty_rather_than_as_noise() {
        let dir = scratch("binary");
        fs::write(dir.join("logo.png"), [0x89, 0x50, 0x00, 0x01]).unwrap();

        let held = read(&dir, "logo.png").unwrap();

        assert_eq!(held.text, "");
        assert_eq!(held.bytes, 4);
    }

    #[test]
    fn paths_come_apart_at_the_nul_git_puts_between_them() {
        let listed = b"src/app.ts\0odd\nname.txt\0README.md\0src/app.ts\0\0";

        assert_eq!(paths_from(listed), vec!["README.md", "odd\nname.txt", "src/app.ts"]);
    }

    #[tokio::test]
    async fn every_file_is_what_git_would_show() {
        let dir = std::env::temp_dir().join(format!("agentland-every-file-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        let git = |args: &[&str]| {
            std::process::Command::new("git").args(args).current_dir(&dir).output().unwrap()
        };

        git(&["init", "-q"]);
        fs::write(dir.join("src/app.ts"), "a").unwrap();
        fs::write(dir.join("gone.txt"), "b").unwrap();
        fs::write(dir.join(".gitignore"), "build/\n").unwrap();
        git(&["add", "-A"]);
        git(&["-c", "user.name=check", "-c", "user.email=check@localhost", "commit", "-qm", "first"]);
        fs::remove_file(dir.join("gone.txt")).unwrap();
        fs::write(dir.join("new.md"), "c").unwrap();
        fs::create_dir_all(dir.join("build")).unwrap();
        fs::write(dir.join("build/out.js"), "d").unwrap();

        let every = every_file(&dir).await.unwrap();

        assert_eq!(every.files, vec![".gitignore", "new.md", "src/app.ts"]);
        assert!(!every.cut);
        let _ = fs::remove_dir_all(&dir);
    }
}
