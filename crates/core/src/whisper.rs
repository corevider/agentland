//! Whisper, fetched rather than installed.
//!
//! Reading speech back needs a model, and a model is a download somebody has to
//! make. The choice was to ask a person to install whisper.cpp themselves and
//! then find the right incantation, or to make the download the app's job. This
//! is the second: the build for this machine and the model come from their own
//! releases, land under the data directory, and the transcriber line is written
//! for them. Nothing is put on PATH and nothing outside the data directory is
//! touched, so removing the folder undoes all of it.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

/// The whisper.cpp release the builds are taken from.
///
/// Pinned rather than "latest": a release that changed its layout under us
/// would break the download for everybody at once, and a version somebody
/// already has is the version they keep.
const RELEASE: &str = "v1.9.2";
const RELEASES: &str = "https://github.com/ggml-org/whisper.cpp/releases/download";
const MODELS: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

/// A model somebody can choose, smallest first.
///
/// `base` is the one that answers in about a second on a laptop and is enough
/// for a sentence of dictation; `small` is the one worth its download for a
/// language other than English, which is why it is the default here.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Model {
    pub id: &'static str,
    pub file: &'static str,
    /// Roughly, in megabytes — what it costs to fetch, said before it is.
    pub megabytes: u32,
    pub says: &'static str,
}

pub const MODELS_ON_OFFER: &[Model] = &[
    Model {
        id: "base",
        file: "ggml-base.bin",
        megabytes: 148,
        says: "fastest, and enough for a sentence in English",
    },
    Model {
        id: "small",
        file: "ggml-small.bin",
        megabytes: 488,
        says: "slower, and the first one worth trusting in another language",
    },
    Model {
        id: "medium",
        file: "ggml-medium.bin",
        megabytes: 1530,
        says: "slowest, for dictation you do not want to correct",
    },
];

pub const BY_DEFAULT: &str = "small";

pub fn model_named(id: &str) -> Option<Model> {
    MODELS_ON_OFFER.iter().copied().find(|model| model.id == id)
}

/// The build of whisper.cpp for this machine, or nothing where none is published.
///
/// Apple silicon is the gap: whisper.cpp ships an xcframework rather than a
/// command-line build, so a mac says what to install instead of pretending.
pub fn build_for(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("windows", "x86_64") => Some("whisper-bin-x64.zip"),
        ("linux", "x86_64") => Some("whisper-bin-ubuntu-x64.tar.gz"),
        ("linux", "aarch64") => Some("whisper-bin-ubuntu-arm64.tar.gz"),
        _ => None,
    }
}

pub fn build_here() -> Option<&'static str> {
    build_for(std::env::consts::OS, std::env::consts::ARCH)
}

pub fn build_url(asset: &str) -> String {
    format!("{RELEASES}/{RELEASE}/{asset}")
}

pub fn model_url(model: &Model) -> String {
    format!("{MODELS}/{}", model.file)
}

/// Everything fetched lives here, under the data directory and nowhere else.
pub fn folder(data_dir: &Path) -> PathBuf {
    data_dir.join("whisper")
}

/// The command-line tool, wherever this platform's archive put it.
///
/// The Windows zip unpacks into `Release/`, the Linux tarball into a folder
/// named after itself, so the tool is looked for rather than assumed.
pub fn tool_in(folder: &Path) -> Option<PathBuf> {
    let name = if cfg!(windows) { "whisper-cli.exe" } else { "whisper-cli" };

    let direct = folder.join(name);
    if direct.is_file() {
        return Some(direct);
    }

    let entries = std::fs::read_dir(folder).ok()?;
    for entry in entries.flatten() {
        let inside = entry.path().join(name);
        if inside.is_file() {
            return Some(inside);
        }
    }

    None
}

/// How the transcriber is asked for what was said, and nothing else.
///
/// `-nt` drops the timestamps, `-np` everything that is not the words, and
/// `auto` lets it hear which language it is being spoken to in — a person who
/// dictates in two languages should not have to say which each time. Quoted
/// because a data directory has a person's name in it, and names have spaces.
pub fn transcriber_line(tool: &Path, model: &Path) -> String {
    format!(
        "\"{}\" -m \"{}\" -l auto -nt -np -f {{file}}",
        tool.display(),
        model.display()
    )
}

/// What is on disk already: the tool, the model, and whether both are here.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Standing {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub ready: bool,
}

pub fn standing(data_dir: &Path, model_id: &str) -> Standing {
    let here = folder(data_dir);
    let tool = tool_in(&here);
    let model = model_named(model_id)
        .map(|model| here.join(model.file))
        .filter(|file| file.is_file());

    Standing {
        ready: tool.is_some() && model.is_some(),
        tool: tool.map(|path| path.display().to_string()),
        model: model.map(|path| path.display().to_string()),
    }
}

/// Unpack an archive with what the machine already has.
///
/// No archive crate: Windows has had `Expand-Archive` since PowerShell 5 and
/// every other machine has `tar`, and a dependency that unpacks two formats is
/// a dependency to keep patched for the life of the app.
fn unpack(archive: &Path, into: &Path) -> Result<()> {
    std::fs::create_dir_all(into)?;

    let done = if archive.extension().and_then(|piece| piece.to_str()) == Some("zip") {
        if cfg!(windows) {
            crate::exec::shell_line(&format!(
                "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
                archive.display(),
                into.display()
            ))
            .output()
        } else {
            crate::exec::command("unzip")
                .args(["-o", &archive.to_string_lossy(), "-d", &into.to_string_lossy()])
                .output()
        }
    } else {
        crate::exec::command("tar")
            .args(["-xzf", &archive.to_string_lossy(), "-C", &into.to_string_lossy()])
            .output()
    };

    match done {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => bail!(
            "could not unpack {}: {}",
            archive.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ),
        Err(error) => bail!("could not unpack {}: {error}", archive.display()),
    }
}

/// Whatever is still missing, fetched. Reports each step as it starts.
pub async fn fetch(data_dir: &Path, model_id: &str, mut say: impl FnMut(String)) -> Result<Standing> {
    let model = model_named(model_id)
        .ok_or_else(|| anyhow::anyhow!("no model called {model_id}"))?;
    let here = folder(data_dir);
    std::fs::create_dir_all(&here)?;

    if tool_in(&here).is_none() {
        let asset = build_here().ok_or_else(|| {
            anyhow::anyhow!(
                "whisper.cpp publishes no build for {} on {} — install whisper-cli yourself and name it in Settings",
                std::env::consts::ARCH,
                std::env::consts::OS
            )
        })?;

        say(format!("fetching whisper.cpp {RELEASE}"));
        let archive = here.join(asset);
        download(&build_url(asset), &archive).await?;
        unpack(&archive, &here)?;
        let _ = std::fs::remove_file(&archive);

        // A downloaded binary is not executable until it is said to be.
        if let Some(tool) = tool_in(&here) {
            make_runnable(&tool);
        }
    }

    let file = here.join(model.file);
    if !file.is_file() {
        say(format!("fetching the {} model, about {} MB", model.id, model.megabytes));
        download(&model_url(&model), &file).await?;
    }

    let standing = standing(data_dir, model_id);
    if !standing.ready {
        bail!("the download finished but whisper is not here — try again");
    }

    Ok(standing)
}

#[cfg(unix)]
fn make_runnable(tool: &Path) {
    use std::os::unix::fs::PermissionsExt;

    if let Ok(held) = std::fs::metadata(tool) {
        let mut mode = held.permissions();
        mode.set_mode(mode.mode() | 0o755);
        let _ = std::fs::set_permissions(tool, mode);
    }

    // The tool loads its own shared libraries from beside it, and those arrive
    // as plain files too.
    if let Some(beside) = tool.parent() {
        for entry in std::fs::read_dir(beside).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.extension().and_then(|piece| piece.to_str()) != Some("so") {
                continue;
            }
            if let Ok(held) = std::fs::metadata(&path) {
                let mut mode = held.permissions();
                mode.set_mode(mode.mode() | 0o755);
                let _ = std::fs::set_permissions(&path, mode);
            }
        }
    }
}

#[cfg(not(unix))]
fn make_runnable(_tool: &Path) {}

/// One file, written as it arrives.
///
/// Into a part file first: a download interrupted halfway is otherwise a model
/// that is there, is the wrong size, and fails at the first sentence with
/// nothing saying why.
async fn download(url: &str, into: &Path) -> Result<()> {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    let part = into.with_extension("part");
    let response = reqwest::Client::builder()
        .build()?
        .get(url)
        .send()
        .await
        .with_context(|| format!("could not reach {url}"))?
        .error_for_status()
        .with_context(|| format!("could not fetch {url}"))?;

    let mut file = tokio::fs::File::create(&part).await?;
    let mut stream = response.bytes_stream();

    while let Some(piece) = stream.next().await {
        file.write_all(&piece?).await?;
    }

    file.flush().await?;
    drop(file);

    std::fs::rename(&part, into)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_machine_with_a_published_build_is_told_which_one() {
        assert_eq!(build_for("windows", "x86_64"), Some("whisper-bin-x64.zip"));
        assert_eq!(build_for("linux", "x86_64"), Some("whisper-bin-ubuntu-x64.tar.gz"));
        assert_eq!(build_for("linux", "aarch64"), Some("whisper-bin-ubuntu-arm64.tar.gz"));
    }

    #[test]
    fn a_machine_with_none_says_so_rather_than_guessing_at_one() {
        assert_eq!(build_for("macos", "aarch64"), None);
        assert_eq!(build_for("windows", "aarch64"), None);
    }

    #[test]
    fn every_url_points_at_the_release_that_was_pinned() {
        assert_eq!(
            build_url("whisper-bin-x64.zip"),
            "https://github.com/ggml-org/whisper.cpp/releases/download/v1.9.2/whisper-bin-x64.zip"
        );
        assert!(model_url(&model_named("small").unwrap()).ends_with("/ggml-small.bin"));
    }

    #[test]
    fn the_model_offered_by_default_is_one_of_the_ones_on_offer() {
        assert!(model_named(BY_DEFAULT).is_some());
        assert!(model_named("enormous").is_none());
    }

    #[test]
    fn the_models_are_offered_smallest_first_with_what_each_costs() {
        let sizes: Vec<u32> = MODELS_ON_OFFER.iter().map(|model| model.megabytes).collect();
        let mut sorted = sizes.clone();
        sorted.sort_unstable();
        assert_eq!(sizes, sorted);
        assert!(MODELS_ON_OFFER.iter().all(|model| !model.says.is_empty()));
    }

    #[test]
    fn the_transcriber_line_quotes_paths_and_asks_only_for_the_words() {
        let line = transcriber_line(
            Path::new("/home/a person/data/whisper/whisper-cli"),
            Path::new("/home/a person/data/whisper/ggml-small.bin"),
        );

        assert!(line.contains("\"/home/a person/data/whisper/whisper-cli\""), "{line}");
        assert!(line.contains("-nt"), "no timestamps: {line}");
        assert!(line.contains("-np"), "nothing but the words: {line}");
        assert!(line.contains("-l auto"), "whichever language it is spoken in: {line}");
        assert!(line.ends_with("-f {file}"), "{line}");
    }

    #[test]
    fn the_tool_is_found_whether_the_archive_nested_it_or_not() {
        let here = std::env::temp_dir().join("agentland-whisper-found");
        let _ = std::fs::remove_dir_all(&here);
        let name = if cfg!(windows) { "whisper-cli.exe" } else { "whisper-cli" };

        std::fs::create_dir_all(here.join("Release")).unwrap();
        assert_eq!(tool_in(&here), None, "nothing is unpacked yet");

        std::fs::write(here.join("Release").join(name), b"x").unwrap();
        assert_eq!(tool_in(&here), Some(here.join("Release").join(name)));

        std::fs::write(here.join(name), b"x").unwrap();
        assert_eq!(tool_in(&here), Some(here.join(name)), "beside beats nested");

        let _ = std::fs::remove_dir_all(&here);
    }

    #[test]
    fn nothing_is_ready_until_both_halves_are_on_disk() {
        let data = std::env::temp_dir().join("agentland-whisper-standing");
        let _ = std::fs::remove_dir_all(&data);
        std::fs::create_dir_all(folder(&data)).unwrap();

        assert!(!standing(&data, "small").ready);

        std::fs::write(folder(&data).join("ggml-small.bin"), b"x").unwrap();
        assert!(!standing(&data, "small").ready, "a model with no tool reads nothing");

        let name = if cfg!(windows) { "whisper-cli.exe" } else { "whisper-cli" };
        std::fs::write(folder(&data).join(name), b"x").unwrap();

        let read = standing(&data, "small");
        assert!(read.ready);
        assert!(read.model.unwrap().ends_with("ggml-small.bin"));

        assert!(!standing(&data, "medium").ready, "another model is another download");

        let _ = std::fs::remove_dir_all(&data);
    }
}
