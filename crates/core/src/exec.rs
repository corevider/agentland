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
