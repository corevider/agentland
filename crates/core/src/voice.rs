use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use parking_lot::Mutex;

/// Speaking to the crew instead of typing to it.
///
/// The recording is made by a program that already exists on the machine and
/// the words are read by another one, named in the settings. Nothing is
/// bundled and nothing is sent anywhere: a microphone is not something to be
/// casual with, and a model is not something to ship by surprise.
pub struct Voice {
    holding: Mutex<Option<Held>>,
    data_dir: PathBuf,
}

struct Held {
    child: Child,
    file: PathBuf,
}

/// The recorders worth trying, in the order they are worth trying.
const RECORDERS: &[&str] = &["pw-record", "parec", "arecord", "ffmpeg"];

/// How each of them is asked for sixteen-kilohertz mono, which is what every
/// speech model wants and the smallest thing worth recording.
pub fn recorder_argv(tool: &str, into: &Path) -> Vec<String> {
    let file = into.to_string_lossy().into_owned();

    match tool {
        "pw-record" => vec!["--rate=16000".into(), "--channels=1".into(), file],
        "parec" => vec![
            "--rate=16000".into(),
            "--channels=1".into(),
            "--file-format=wav".into(),
            file,
        ],
        "arecord" => vec![
            "-f".into(),
            "S16_LE".into(),
            "-r".into(),
            "16000".into(),
            "-c".into(),
            "1".into(),
            file,
        ],
        "ffmpeg" => vec![
            "-f".into(),
            "pulse".into(),
            "-i".into(),
            "default".into(),
            "-ar".into(),
            "16000".into(),
            "-ac".into(),
            "1".into(),
            "-y".into(),
            file,
        ],
        _ => vec![file],
    }
}

/// The first recorder on this machine, or nothing.
pub fn pick_recorder(here: impl Fn(&str) -> bool) -> Option<&'static str> {
    RECORDERS.iter().copied().find(|tool| here(tool))
}

pub fn on_path(tool: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| dir.join(tool).is_file())
        })
        .unwrap_or(false)
}

/// The command that reads the words, with the recording's path put in.
///
/// `{file}` is where the recording goes. A command without it gets the path on
/// the end, because that is what most of them expect anyway.
pub fn fill_in(command: &str, file: &Path) -> String {
    let path = file.to_string_lossy();

    if command.contains("{file}") {
        command.replace("{file}", &path)
    } else {
        format!("{command} {path}")
    }
}

/// What a transcriber printed, tidied.
///
/// Models print leading blank lines, trailing newlines, and sometimes bracketed
/// noise like "[BLANK_AUDIO]" for a recording with nothing in it. None of that
/// is what somebody said.
pub fn heard(output: &str) -> String {
    let said: String = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !(line.starts_with('[') && line.ends_with(']')))
        .collect::<Vec<_>>()
        .join(" ");

    said.trim().to_owned()
}

impl Voice {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            holding: Mutex::new(None),
            data_dir,
        }
    }

    /// Whether anything here can record at all.
    pub fn recorder(&self) -> Option<&'static str> {
        pick_recorder(on_path)
    }

    pub fn listening(&self) -> bool {
        self.holding.lock().is_some()
    }

    /// Start recording. Refuses to start a second one: two recorders on one
    /// microphone is two half-recordings.
    pub fn start(&self) -> anyhow::Result<()> {
        let mut holding = self.holding.lock();
        if holding.is_some() {
            anyhow::bail!("already listening");
        }

        let tool = pick_recorder(on_path)
            .ok_or_else(|| anyhow::anyhow!("no recorder here: install pw-record, parec or arecord"))?;

        let folder = self.data_dir.join("voice");
        std::fs::create_dir_all(&folder)?;
        let file = folder.join("said.wav");
        let _ = std::fs::remove_file(&file);

        let child = Command::new(tool)
            .args(recorder_argv(tool, &file))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        *holding = Some(Held { child, file });
        Ok(())
    }

    /// Stop, and say what was said. An empty answer is not an error: somebody
    /// pressed the key and thought better of it.
    pub fn stop(&self, command: Option<&str>) -> anyhow::Result<String> {
        let held = self
            .holding
            .lock()
            .take()
            .ok_or_else(|| anyhow::anyhow!("nothing is being recorded"))?;

        let mut held = held;
        let _ = held.child.kill();
        let _ = held.child.wait();

        let Some(command) = command.map(str::trim).filter(|held| !held.is_empty()) else {
            anyhow::bail!(
                "no transcriber set — put a command in Settings, such as \
                 `whisper-cli -m models/ggml-base.en.bin -nt -f {{file}}`"
            );
        };

        if !held.file.exists() {
            anyhow::bail!("the recorder wrote nothing");
        }

        let spoken = Command::new("sh")
            .arg("-c")
            .arg(fill_in(command, &held.file))
            .output()?;

        if !spoken.status.success() {
            anyhow::bail!(
                "the transcriber failed: {}",
                String::from_utf8_lossy(&spoken.stderr).trim()
            );
        }

        Ok(heard(&String::from_utf8_lossy(&spoken.stdout)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_recorder_is_asked_for_the_same_thing_in_its_own_words() {
        let file = Path::new("/tmp/said.wav");

        assert!(recorder_argv("pw-record", file).contains(&"--rate=16000".to_owned()));
        assert!(recorder_argv("arecord", file).contains(&"16000".to_owned()));
        assert!(recorder_argv("ffmpeg", file).contains(&"-ac".to_owned()));

        for tool in ["pw-record", "parec", "arecord", "ffmpeg"] {
            assert!(
                recorder_argv(tool, file).contains(&"/tmp/said.wav".to_owned()),
                "{tool} must be told where to write"
            );
        }
    }

    #[test]
    fn the_first_recorder_that_is_here_is_the_one_used() {
        assert_eq!(pick_recorder(|tool| tool == "arecord"), Some("arecord"));
        assert_eq!(pick_recorder(|_| true), Some("pw-record"), "in order");
        assert_eq!(pick_recorder(|_| false), None);
    }

    #[test]
    fn the_recording_goes_where_the_command_says_or_on_the_end() {
        let file = Path::new("/tmp/said.wav");

        assert_eq!(
            fill_in("whisper -f {file} --model base", file),
            "whisper -f /tmp/said.wav --model base"
        );
        assert_eq!(fill_in("my-transcriber", file), "my-transcriber /tmp/said.wav");
    }

    #[test]
    fn what_a_model_prints_around_the_words_is_not_the_words() {
        assert_eq!(heard("\n\n  hello there \n\n"), "hello there");
        assert_eq!(heard("[BLANK_AUDIO]"), "");
        assert_eq!(
            heard("[00:00.000 --> 00:02.000]\n take the metrics work \n"),
            "take the metrics work"
        );
    }

    #[test]
    fn two_lines_of_speech_are_one_thing_said() {
        assert_eq!(heard("plan the work\nthen hand it out"), "plan the work then hand it out");
    }
}
