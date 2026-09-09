use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

use parking_lot::Mutex;

/// Speaking to the crew instead of typing to it.
///
/// The recording is made by a program that already exists on the machine and
/// the words are read by another one, named in the settings. Nothing is
/// bundled and nothing is sent anywhere: a microphone is not something to be
/// casual with, and a model is not something to ship by surprise.
/// How long a transcriber is taken to still be warm.
///
/// A model that has to be loaded costs about eight seconds, so anything worth
/// naming a transcriber keeps it in memory and lets go after a while. Twenty
/// minutes is under every such timeout seen so far, and being wrong here is
/// only one wasted second of somebody else's idle CPU.
const STAYS_WARM: Duration = Duration::from_secs(20 * 60);

pub struct Voice {
    holding: Mutex<Option<Held>>,
    /// When something was last read back here, so a press knows whether the
    /// transcriber is cold enough to be worth waking.
    last_read: Mutex<Option<Instant>>,
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

/// What to say when nothing here can record.
///
/// The old wording named three tools that only exist on Linux, which on Windows
/// reads as "install these" and leads nowhere. Naming the platform is the part
/// that tells somebody whether to go looking at all.
pub fn nothing_records() -> String {
    if cfg!(windows) {
        "voice needs a recorder Agentland can drive, and there is none for Windows yet".to_owned()
    } else if cfg!(target_os = "macos") {
        "no recorder here: put ffmpeg on PATH".to_owned()
    } else {
        "no recorder here: install pw-record, parec or arecord".to_owned()
    }
}

/// What to say when there is nothing to read a recording back with.
///
/// The old line was a riddle — *Settings, then House rules' neighbour, Voice* —
/// which names no action, and on a machine where nothing has been fetched yet
/// it reads as though something is broken rather than as something to set up.
/// Voice, in Settings, fetches whisper.cpp and a model for this machine and
/// writes the line itself; where nobody publishes a build for the machine, the
/// honest answer is that it has to be installed by hand.
pub fn no_transcriber() -> String {
    if crate::whisper::build_here().is_some() {
        "nothing can read a recording back yet — open Settings, Voice and pick a model: \
         whisper is fetched there, and the command is written for you"
            .to_owned()
    } else {
        format!(
            "nothing can read a recording back yet — whisper.cpp publishes no build for {} on {}, \
             so install whisper-cli yourself and name it in Settings, Voice",
            std::env::consts::ARCH,
            std::env::consts::OS
        )
    }
}

/// One second of silence, as a wav a speech model will read.
///
/// Written here rather than shipped as a file: 32 kilobytes of zeros in the
/// repository is 32 kilobytes nobody can read, and the header is eleven fields
/// long. Sixteen kilohertz, one channel, sixteen bits — what the recorders are
/// asked for, and what every model wants.
pub fn a_second_of_silence() -> Vec<u8> {
    const RATE: u32 = 16_000;
    const BITS: u16 = 16;
    const CHANNELS: u16 = 1;

    let audio = (RATE * u32::from(BITS / 8) * u32::from(CHANNELS)) as usize;
    let mut wav = Vec::with_capacity(44 + audio);

    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&((36 + audio) as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&CHANNELS.to_le_bytes());
    wav.extend_from_slice(&RATE.to_le_bytes());
    wav.extend_from_slice(&(RATE * u32::from(CHANNELS) * u32::from(BITS / 8)).to_le_bytes());
    wav.extend_from_slice(&(CHANNELS * BITS / 8).to_le_bytes());
    wav.extend_from_slice(&BITS.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(audio as u32).to_le_bytes());
    wav.resize(44 + audio, 0);

    wav
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

/// The language a transcriber is being asked to hear, when nobody has picked
/// one. It is whisper's own word for the guess, and the word every transcriber
/// here is told to expect.
pub const GUESS: &str = "auto";

/// The command that reads the words, with the recording's path put in.
///
/// `{file}` is where the recording goes, and a command without it gets the path
/// on the end, because that is what most of them expect anyway. The one put on
/// the end is quoted: the recording lives under the data directory, which on
/// Windows is under `C:\Users\<name>`, and a person whose name has a space in
/// it would otherwise hand the transcriber two arguments. A path written into
/// `{file}` is left exactly as the person wrote it, quotes and all — the line
/// Agentland writes for itself quotes it.
///
/// `{language}` is the language picked in Settings, or `auto`. It is a
/// placeholder rather than a flag because every transcriber spells the flag
/// differently, and the same value reaches the ones that read an environment
/// instead — see `run_transcriber`.
pub fn fill_in(command: &str, file: &Path, language: &str) -> String {
    let path = file.to_string_lossy();
    let language = spoken_language(language);

    let with_file = if command.contains("{file}") {
        command.replace("{file}", &path)
    } else {
        format!("{command} \"{path}\"")
    };

    with_file.replace("{language}", language)
}

/// The language, as a transcriber is told it. Nothing picked is the guess.
pub fn spoken_language(language: &str) -> &str {
    let held = language.trim();

    if held.is_empty() {
        GUESS
    } else {
        held
    }
}

/// Run the transcriber over one recording.
///
/// The language is handed over twice on purpose, because transcribers differ:
/// one takes it as a flag, and `{language}` in the command is where that goes;
/// another reads it out of its environment, and never has to be told to. Both
/// carry the same word, so neither has to be configured around the other.
fn run_transcriber(command: &str, file: &Path, language: &str) -> std::io::Result<std::process::Output> {
    crate::exec::shell_line(&fill_in(command, file, language))
        .env("AGENTLAND_VOICE_LANGUAGE", spoken_language(language))
        .output()
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
            last_read: Mutex::new(None),
            data_dir: crate::exec::settled(&data_dir),
        }
    }

    /// Something was read back just now, so whatever holds the model is awake.
    pub fn read_something(&self) {
        *self.last_read.lock() = Some(Instant::now());
    }

    /// Whether waking the transcriber now would be doing it a favour.
    pub fn is_cold(&self) -> bool {
        self.last_read
            .lock()
            .map(|when| when.elapsed() >= STAYS_WARM)
            .unwrap_or(true)
    }

    /// Load the model while the person is still speaking.
    ///
    /// A transcriber that keeps a model in memory spends about eight seconds
    /// putting it there, and it was spending them after the sentence was
    /// finished — the one moment somebody is watching. A second of silence,
    /// handed over the moment the button goes down, makes it happen while they
    /// are still talking. Nothing is done with what comes back, and a
    /// transcriber that keeps nothing in memory only reads a second of silence
    /// it was going to be given anyway.
    pub fn wake(&self, command: &str, language: &str) -> anyhow::Result<()> {
        let folder = self.data_dir.join("voice");
        std::fs::create_dir_all(&folder)?;

        let quiet = folder.join("quiet.wav");
        if !quiet.is_file() {
            std::fs::write(&quiet, a_second_of_silence())?;
        }

        self.read_something();
        let _ = run_transcriber(command, &quiet, language)?;

        Ok(())
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

        let tool = pick_recorder(on_path).ok_or_else(|| anyhow::anyhow!("{}", nothing_records()))?;

        let folder = self.data_dir.join("voice");
        std::fs::create_dir_all(&folder)?;
        let file = folder.join("said.wav");
        let _ = std::fs::remove_file(&file);

        let child = crate::exec::command(tool)
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
    pub fn stop(&self, command: Option<&str>, language: &str) -> anyhow::Result<String> {
        let held = self
            .holding
            .lock()
            .take()
            .ok_or_else(|| anyhow::anyhow!("nothing is being recorded"))?;

        let mut held = held;
        let _ = held.child.kill();
        let _ = held.child.wait();

        let Some(command) = command.map(str::trim).filter(|held| !held.is_empty()) else {
            anyhow::bail!("{}", no_transcriber());
        };

        if !held.file.exists() {
            anyhow::bail!("the recorder wrote nothing");
        }

        let spoken = run_transcriber(command, &held.file, language)?;

        if !spoken.status.success() {
            anyhow::bail!(
                "the transcriber failed: {}",
                String::from_utf8_lossy(&spoken.stderr).trim()
            );
        }

        Ok(heard(&String::from_utf8_lossy(&spoken.stdout)))
    }
}

/// Read back a recording that was made elsewhere.
///
/// A browser records webm or mp4, never a wav, so it is converted first — by
/// ffmpeg if it is here, which is also what every recorder on this machine
/// depends on. The bytes are written under the data folder and replaced by the
/// next recording rather than piling up.
pub fn read_back(
    data_dir: &Path,
    audio: &[u8],
    kind: &str,
    command: &str,
    language: &str,
) -> anyhow::Result<String> {
    let folder = data_dir.join("voice");
    std::fs::create_dir_all(&folder)?;

    let arrived = folder.join(format!("arrived.{}", extension_for(kind)));
    std::fs::write(&arrived, audio)?;

    // A recording that is already a wav is what the transcriber wants, so it
    // goes straight there. The window records one on purpose: no encoder, no
    // decoder, and no process spawned only to fail on a machine with no ffmpeg.
    if kind.contains("wav") {
        return read_aloud(&arrived, command, language);
    }

    // A name of its own: a wav that arrived is already called arrived.wav, and
    // ffmpeg asked to write its own input destroys the recording it was given.
    let wav = folder.join("heard.wav");
    let _ = std::fs::remove_file(&wav);

    let converted = crate::exec::command("ffmpeg")
        .args([
            "-loglevel", "error", "-i",
            &arrived.to_string_lossy(),
            "-ar", "16000", "-ac", "1", "-y",
            &wav.to_string_lossy(),
        ])
        .output();

    let file = match converted {
        Ok(done) if done.status.success() && wav.exists() => wav,
        Ok(done) => anyhow::bail!(
            "cannot turn that recording into audio: {}",
            String::from_utf8_lossy(&done.stderr).trim()
        ),
        Err(error) => anyhow::bail!("ffmpeg is needed to read a recording from a browser: {error}"),
    };

    read_aloud(&file, command, language)
}

/// Hand one recording to the transcriber and keep what it said.
fn read_aloud(file: &Path, command: &str, language: &str) -> anyhow::Result<String> {
    let spoken = run_transcriber(command, file, language)?;

    if !spoken.status.success() {
        anyhow::bail!(
            "the transcriber failed: {}",
            String::from_utf8_lossy(&spoken.stderr).trim()
        );
    }

    Ok(heard(&String::from_utf8_lossy(&spoken.stdout)))
}

/// What to call the file, from what the browser said it is.
pub fn extension_for(kind: &str) -> &'static str {
    let kind = kind.to_ascii_lowercase();

    if kind.contains("wav") {
        "wav"
    } else if kind.contains("ogg") {
        "ogg"
    } else if kind.contains("mp4") || kind.contains("m4a") || kind.contains("aac") {
        "m4a"
    } else {
        "webm"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_missing_is_said_in_terms_of_this_machine() {
        let says = nothing_records();
        if cfg!(windows) {
            assert!(says.contains("Windows"), "{says}");
            assert!(!says.contains("arecord"), "naming Linux tools leads nowhere here: {says}");
        } else {
            assert!(!says.contains("Windows"), "{says}");
        }
    }

    #[test]
    fn the_silence_handed_over_early_is_a_wav_something_can_read() {
        let wav = a_second_of_silence();

        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(wav.len(), 44 + 32_000, "a second at sixteen kilohertz, sixteen bits");
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 16_000);
        assert!(wav[44..].iter().all(|byte| *byte == 0), "silence");
        assert_eq!(
            u32::from_le_bytes(wav[4..8].try_into().unwrap()) as usize,
            wav.len() - 8,
            "the size in the header is the size of the file"
        );
    }

    #[test]
    fn a_transcriber_is_only_worth_waking_once_it_has_gone_cold() {
        let voice = Voice::new(std::env::temp_dir().join("agentland-voice-test"));

        assert!(voice.is_cold(), "nothing has been read back yet");

        voice.read_something();
        assert!(!voice.is_cold(), "it answered a moment ago");
    }

    #[test]
    fn the_way_to_a_transcriber_is_named_rather_than_hinted_at() {
        let says = no_transcriber();

        assert!(says.contains("Settings"), "{says}");
        assert!(says.contains("Voice"), "{says}");
        assert!(
            !says.contains("neighbour"),
            "a person told where a panel sits relative to another panel has been told nothing: {says}"
        );

        // Where whisper.cpp publishes a build, Settings fetches it; where it
        // does not, saying "pick a model" would send somebody to a button that
        // cannot work. The two answers are different on purpose.
        if crate::whisper::build_here().is_some() {
            assert!(says.contains("pick a model"), "{says}");
        } else {
            assert!(says.contains(std::env::consts::OS), "{says}");
        }
    }

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
    fn the_language_goes_in_where_the_command_asks_for_it() {
        let file = Path::new("/tmp/said.wav");

        assert_eq!(
            fill_in("whisper -l {language} -f {file}", file, "tr"),
            "whisper -l tr -f /tmp/said.wav"
        );

        // Nothing picked is the guess, in the word whisper itself uses.
        assert_eq!(
            fill_in("whisper -l {language} -f {file}", file, ""),
            "whisper -l auto -f /tmp/said.wav"
        );

        // A command written before there was a picker asks for no language, and
        // is left as it is: it gets the choice through its environment instead.
        assert_eq!(
            fill_in("my-transcriber {file}", file, "tr"),
            "my-transcriber /tmp/said.wav"
        );
    }

    #[test]
    fn nothing_picked_is_the_guess_and_a_pick_is_taken_as_it_is() {
        assert_eq!(spoken_language(""), "auto");
        assert_eq!(spoken_language("   "), "auto");
        assert_eq!(spoken_language("tr"), "tr");
        assert_eq!(spoken_language(" en "), "en");
    }

    #[test]
    fn the_recording_goes_where_the_command_says_or_on_the_end() {
        let file = Path::new("/tmp/said.wav");

        assert_eq!(
            fill_in("whisper -f {file} --model base", file, GUESS),
            "whisper -f /tmp/said.wav --model base"
        );
        assert_eq!(fill_in("my-transcriber", file, GUESS), "my-transcriber \"/tmp/said.wav\"");
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
    fn a_browser_recording_is_named_for_what_it_is() {
        assert_eq!(extension_for("audio/webm;codecs=opus"), "webm");
        assert_eq!(extension_for("audio/mp4"), "m4a");
        assert_eq!(extension_for("audio/ogg"), "ogg");
        assert_eq!(extension_for("audio/wav"), "wav");
        assert_eq!(extension_for(""), "webm", "a browser that says nothing records webm");
    }

    #[test]
    fn two_lines_of_speech_are_one_thing_said() {
        assert_eq!(heard("plan the work\nthen hand it out"), "plan the work then hand it out");
    }
}
