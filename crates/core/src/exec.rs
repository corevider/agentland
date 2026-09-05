//! Running a tool the way the machine would.
//!
//! Two things go wrong when a program spawns a tool by name. On Windows the
//! tools that come from npm — `npm`, `claude`, `codex` — are `.cmd` shims, which
//! `CreateProcess` never finds and `cmd.exe` always does; asking for `npm` said
//! it was not installed on a machine where it plainly was. And a console tool
//! started from a windowed app gets a console window of its own, so probing five
//! tools at start flashed five black boxes across the screen.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Where a tool is, the way a shell would find it.
pub fn find(tool: &str) -> Option<PathBuf> {
    let given = Path::new(tool);
    if given.components().count() > 1 {
        return given.is_file().then(|| given.to_path_buf());
    }

    let dirs = std::env::var_os("PATH")?;
    let extensions = if cfg!(windows) { windows_extensions() } else { Vec::new() };

    for dir in std::env::split_paths(&dirs) {
        if let Some(found) = find_in(&dir, tool, &extensions) {
            return Some(found);
        }
    }

    None
}

fn windows_extensions() -> Vec<String> {
    std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_owned())
        .split(';')
        .filter(|piece| !piece.is_empty())
        .map(|piece| piece.to_lowercase())
        .collect()
}

/// The tool in one folder: as named, or with one of the extensions the shell
/// would try. A name that already carries an extension is taken as it is.
pub fn find_in(dir: &Path, tool: &str, extensions: &[String]) -> Option<PathBuf> {
    let bare = dir.join(tool);
    if bare.is_file() && (extensions.is_empty() || Path::new(tool).extension().is_some()) {
        return Some(bare);
    }

    extensions
        .iter()
        .map(|extension| dir.join(format!("{tool}{extension}")))
        .find(|candidate| candidate.is_file())
}

/// The program to start and the arguments that go before the tool's own.
///
/// A `.cmd` or `.bat` is not a program; `cmd.exe /C` is what runs it.
pub fn launch_for(found: Option<PathBuf>, tool: &str, windows: bool) -> (String, Vec<String>) {
    let Some(path) = found else {
        return (tool.to_owned(), Vec::new());
    };

    let extension = path
        .extension()
        .and_then(|piece| piece.to_str())
        .map(|piece| piece.to_lowercase());

    if windows && matches!(extension.as_deref(), Some("cmd") | Some("bat")) {
        return ("cmd.exe".to_owned(), vec!["/C".to_owned(), path.to_string_lossy().into_owned()]);
    }

    (path.to_string_lossy().into_owned(), Vec::new())
}

pub fn launch(tool: &str) -> (String, Vec<String>) {
    launch_for(find(tool), tool, cfg!(windows))
}

/// A command for a tool, found the way the shell finds it, with no console
/// window of its own.
pub fn command(tool: &str) -> Command {
    let (program, leading) = launch(tool);
    let mut command = Command::new(program);
    command.args(leading);
    quiet(&mut command);
    command
}

pub fn tokio_command(tool: &str) -> tokio::process::Command {
    let (program, leading) = launch(tool);
    let mut command = tokio::process::Command::new(program);
    command.args(leading);
    quiet_tokio(&mut command);
    command
}

/// Where this person's home is, on any machine.
///
/// `HOME` is not set on Windows — the engine itself reads `USERPROFILE` there,
/// as every Node program does. Looking only at `HOME` meant Agentland could not
/// find the engine's own files on Windows: the trust answer it writes, so that
/// nobody is asked whether they trust a folder Agentland made, went nowhere and
/// every pane asked again.
pub fn home() -> Option<PathBuf> {
    home_from(|name| std::env::var_os(name).map(|value| value.to_string_lossy().into_owned()), cfg!(windows))
}

pub fn home_from(read: impl Fn(&str) -> Option<String>, windows: bool) -> Option<PathBuf> {
    let held = |name: &str| read(name).filter(|value| !value.trim().is_empty());

    if windows {
        // In the engine's order, not ours: a trust entry keyed by a different
        // home than the one it reads is a trust entry it never sees.
        if let Some(profile) = held("USERPROFILE") {
            return Some(PathBuf::from(profile));
        }

        if let (Some(drive), Some(path)) = (held("HOMEDRIVE"), held("HOMEPATH")) {
            return Some(PathBuf::from(format!("{drive}{path}")));
        }
    }

    held("HOME").map(PathBuf::from)
}

/// A command that runs one written line through a shell.
///
/// `sh -c` is not a thing on Windows, so a transcriber set in Settings never
/// ran there — the failure was the shell missing, not the transcriber. The
/// flag differs too: cmd takes `/C`, PowerShell takes `-Command`.
pub fn shell_line(line: &str) -> Command {
    if cfg!(windows) {
        let shell = default_shell();
        let flag = if shell.to_lowercase().contains("cmd") { "/C" } else { "-Command" };
        let mut command = command(&shell);
        command.arg(flag).arg(line);
        return command;
    }

    let mut command = command("sh");
    command.arg("-c").arg(line);
    command
}

/// The shell a person expects on this machine: what SHELL says, or bash;
/// PowerShell on Windows, and cmd.exe where there is none.
pub fn default_shell() -> String {
    if cfg!(windows) {
        return if find("pwsh").is_some() {
            "pwsh".to_owned()
        } else if find("powershell").is_some() {
            "powershell".to_owned()
        } else {
            std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_owned())
        };
    }

    std::env::var("SHELL")
        .ok()
        .filter(|shell| !shell.trim().is_empty())
        .unwrap_or_else(|| "bash".to_owned())
}

/// A path a tool can read back, without Windows' extended-length prefix.
///
/// `canonicalize` on Windows answers `\\?\C:\...`. Rust's own file calls take
/// that, and so does a child process's working directory — so it stays invisible
/// until a path is handed to a tool as an *argument*, where the tool has to
/// parse it itself. Git does not: cutting a worktree under the data directory
/// died with `could not create leading directories of '//?/C:/...': Invalid
/// argument`, and starting a project on Windows failed with it every time.
///
/// So a path is made plain at the moment it is settled, not at each of the
/// places it is later used. A device path that is not a drive keeps its prefix:
/// it means something there, and shortening it would name a different thing.
pub fn plain(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();

    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }

    match text.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => path,
    }
}

/// Settle a path and make it plain, leaving it as given when it cannot be read.
pub fn settled(path: &Path) -> PathBuf {
    plain(std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()))
}

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(windows)]
fn quiet(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn quiet(_command: &mut Command) {}

#[cfg(windows)]
fn quiet_tokio(command: &mut tokio::process::Command) {
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn quiet_tokio(_command: &mut tokio::process::Command) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: Vec<(&'static str, &'static str)>) -> impl Fn(&str) -> Option<String> {
        move |name| {
            pairs
                .iter()
                .find(|(held, _)| *held == name)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    #[test]
    fn on_windows_home_is_where_the_engine_looks_for_it() {
        assert_eq!(
            home_from(env(vec![("USERPROFILE", r"C:\Users\somebody")]), true),
            Some(PathBuf::from(r"C:\Users\somebody"))
        );
    }

    #[test]
    fn a_windows_machine_with_home_set_by_something_else_is_not_led_astray() {
        let both = env(vec![("USERPROFILE", r"C:\Users\somebody"), ("HOME", "/c/msys/home")]);
        assert_eq!(home_from(both, true), Some(PathBuf::from(r"C:\Users\somebody")));
    }

    #[test]
    fn a_domain_home_is_put_back_together_from_its_two_halves() {
        let split = env(vec![("HOMEDRIVE", "H:"), ("HOMEPATH", r"\somebody")]);
        assert_eq!(home_from(split, true), Some(PathBuf::from(r"H:\somebody")));
    }

    #[test]
    fn everywhere_else_home_is_home() {
        assert_eq!(
            home_from(env(vec![("HOME", "/home/somebody")]), false),
            Some(PathBuf::from("/home/somebody"))
        );
        assert_eq!(home_from(env(vec![]), false), None);
        assert_eq!(home_from(env(vec![("HOME", "  ")]), false), None, "empty is not a home");
    }

    #[test]
    fn a_written_line_goes_through_a_shell_this_machine_has() {
        let command = shell_line("whisper -f said.wav");
        let program = command.get_program().to_string_lossy().to_lowercase();
        let args: Vec<String> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();

        if cfg!(windows) {
            assert!(!program.contains("/sh"), "there is no sh here: {program}");
            assert!(args.iter().any(|arg| arg == "/C" || arg == "-Command"), "{args:?}");
        } else {
            assert!(program.ends_with("sh"), "{program}");
            assert!(args.contains(&"-c".to_owned()), "{args:?}");
        }

        assert!(args.iter().any(|arg| arg.contains("whisper")), "{args:?}");
    }

    #[test]
    fn a_windows_drive_path_loses_the_prefix_a_tool_cannot_read() {
        assert_eq!(
            plain(PathBuf::from(r"\\?\C:\Users\somebody\data\worktrees\a-project")),
            PathBuf::from(r"C:\Users\somebody\data\worktrees\a-project")
        );
    }

    #[test]
    fn a_share_comes_back_as_the_share_everybody_writes() {
        assert_eq!(
            plain(PathBuf::from(r"\\?\UNC\server\share\project")),
            PathBuf::from(r"\\server\share\project")
        );
    }

    #[test]
    fn a_device_that_is_not_a_drive_keeps_the_prefix_that_names_it() {
        let device = PathBuf::from(r"\\?\Volume{b75e2c83-0000-0000-0000-602f00000000}\a");
        assert_eq!(plain(device.clone()), device);
    }

    #[test]
    fn a_path_without_the_prefix_is_left_exactly_as_it_is() {
        for path in ["/home/somebody/data", r"C:\Users\somebody\data", "data/worktrees"] {
            assert_eq!(plain(PathBuf::from(path)), PathBuf::from(path));
        }
    }

    #[test]
    fn a_cmd_shim_runs_through_cmd_exe_on_windows() {
        let (program, leading) = launch_for(Some(PathBuf::from(r"C:\nodejs\npm.cmd")), "npm", true);
        assert_eq!(program, "cmd.exe");
        assert_eq!(leading, vec!["/C".to_owned(), r"C:\nodejs\npm.cmd".to_owned()]);
    }

    #[test]
    fn an_exe_runs_as_itself() {
        let (program, leading) = launch_for(Some(PathBuf::from(r"C:\Go\bin\go.exe")), "go", true);
        assert_eq!(program, r"C:\Go\bin\go.exe");
        assert!(leading.is_empty());
    }

    #[test]
    fn a_tool_nobody_found_is_asked_for_by_name_and_left_to_fail_honestly() {
        let (program, leading) = launch_for(None, "nothing-here", true);
        assert_eq!(program, "nothing-here");
        assert!(leading.is_empty());
    }

    #[test]
    fn extensions_are_tried_the_way_the_shell_tries_them() {
        let dir = std::env::temp_dir().join("agentland-exec-find");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("npm.cmd"), "@echo off").unwrap();

        let extensions = vec![".exe".to_owned(), ".cmd".to_owned()];
        assert_eq!(find_in(&dir, "npm", &extensions), Some(dir.join("npm.cmd")));
        assert_eq!(find_in(&dir, "npm.cmd", &extensions), Some(dir.join("npm.cmd")));
        assert_eq!(find_in(&dir, "yarn", &extensions), None);
    }

    #[cfg(unix)]
    #[test]
    fn on_unix_a_tool_is_found_as_named() {
        let found = find("sh").expect("sh is on every unix PATH");
        assert!(found.ends_with("sh"));
        assert!(find("agentland-no-such-tool").is_none());
    }
}
