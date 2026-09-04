//! A screenshot taken from the tray, for a card.
//!
//! The desktop's own screenshot tool is the one people know — GNOME's area
//! picker, macOS's crosshair — so that is what is asked for, and what it
//! writes is what the card gets. Nothing here grabs pixels itself: on a
//! Wayland desktop an application is not allowed to, and on the others the
//! system tool is better than anything drawn here.

use std::path::PathBuf;
use std::process::Command;

use anyhow::{anyhow, bail, Result};

/// Ask the desktop for a screenshot and say where it was written.
///
/// Blocks until the person has picked an area or given up, so it belongs on
/// a thread of its own.
pub fn take_one() -> Result<PathBuf> {
    if cfg!(target_os = "macos") {
        return with_screencapture();
    }
    if cfg!(target_os = "linux") {
        return through_the_portal();
    }
    bail!("taking a screenshot from the tray is not available on this system yet")
}

/// macOS: the crosshair, into a file of our own.
fn with_screencapture() -> Result<PathBuf> {
    let path = std::env::temp_dir().join(format!("agentland-shot-{}.png", stamp()));
    let status = Command::new("screencapture")
        .args(["-i", "-x"])
        .arg(&path)
        .status()
        .map_err(|error| anyhow!("cannot run screencapture: {error}"))?;

    if !status.success() || !path.is_file() {
        bail!("no screenshot was taken");
    }
    Ok(path)
}

/// The portal's screenshot request, asked for in Python.
///
/// The request is answered with a signal sent only to the connection that
/// asked, so a command-line call cannot hear it; a short script on the
/// desktop's own bindings can, and every GNOME and KDE machine has them.
const PORTAL_ASK: &str = r#"
import sys
import gi
gi.require_version("Gio", "2.0")
from gi.repository import Gio, GLib

bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
sender = bus.get_unique_name()[1:].replace(".", "_")
token = "agentland%d" % GLib.random_int_range(0, 1 << 30)
request = "/org/freedesktop/portal/desktop/request/%s/%s" % (sender, token)
loop = GLib.MainLoop()
answer = {}

def heard(connection, sender_name, path, interface, signal, params):
    code, data = params.unpack()
    answer["code"] = code
    answer["uri"] = data.get("uri")
    loop.quit()

bus.signal_subscribe(
    "org.freedesktop.portal.Desktop", "org.freedesktop.portal.Request", "Response",
    request, None, Gio.DBusSignalFlags.NONE, heard,
)
bus.call_sync(
    "org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop",
    "org.freedesktop.portal.Screenshot", "Screenshot",
    GLib.Variant("(sa{sv})", ("", {
        "interactive": GLib.Variant("b", True),
        "handle_token": GLib.Variant("s", token),
    })),
    None, Gio.DBusCallFlags.NONE, -1, None,
)
GLib.timeout_add_seconds(600, loop.quit)
loop.run()

if answer.get("code") == 0 and answer.get("uri"):
    print(answer["uri"])
else:
    sys.exit(2)
"#;

/// Linux: the desktop portal, which shows the desktop's own picker.
fn through_the_portal() -> Result<PathBuf> {
    let output = Command::new("python3")
        .args(["-c", PORTAL_ASK])
        .output()
        .map_err(|error| anyhow!("cannot run python3 for the screenshot portal: {error}"))?;

    if output.status.code() == Some(2) {
        bail!("no screenshot was taken");
    }
    if !output.status.success() {
        bail!(
            "the screenshot portal did not answer: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let uri = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let path = path_of(&uri).ok_or_else(|| anyhow!("the portal answered with {uri}, which is not a file"))?;
    if !path.is_file() {
        bail!("the portal named {} and there is nothing there", path.display());
    }
    Ok(path)
}

/// The file behind a `file://` URI, with the escapes undone.
pub fn path_of(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    if !rest.starts_with('/') {
        return None;
    }

    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' && at + 2 < bytes.len() {
            if let Ok(value) = u8::from_str_radix(std::str::from_utf8(&bytes[at + 1..at + 3]).ok()?, 16) {
                out.push(value);
                at += 3;
                continue;
            }
        }
        out.push(bytes[at]);
        at += 1;
    }

    Some(PathBuf::from(String::from_utf8(out).ok()?))
}

/// A name for the file as the card should know it.
pub fn name_for(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| format!("screenshot-{}.png", stamp()))
}

fn stamp() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or_default();
    seconds.to_string()
}

/// Put the picture on the clipboard, so it pastes anywhere.
///
/// Linux: through GTK, on the main thread, from the process that stays alive
/// — a Wayland clipboard is served by whoever set it, and empties when they
/// go. macOS: the pasteboard, through the scripting bridge.
#[cfg(target_os = "linux")]
pub fn put_on_clipboard(path: &std::path::Path) -> Result<()> {
    let pixbuf = gdk_pixbuf::Pixbuf::from_file(path)
        .map_err(|error| anyhow!("cannot read {} as a picture: {error}", path.display()))?;
    let clipboard = gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD);
    clipboard.set_image(&pixbuf);
    clipboard.store();
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn put_on_clipboard(path: &std::path::Path) -> Result<()> {
    let script = format!(
        "set the clipboard to (read (POSIX file \"{}\") as «class PNGf»)",
        path.display()
    );
    let status = Command::new("osascript")
        .args(["-e", &script])
        .status()
        .map_err(|error| anyhow!("cannot run osascript: {error}"))?;
    if !status.success() {
        bail!("the pasteboard refused the picture");
    }
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn put_on_clipboard(_path: &std::path::Path) -> Result<()> {
    bail!("the clipboard is not reachable on this system yet")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_uri_becomes_a_path_with_its_spaces_back() {
        assert_eq!(
            path_of("file:///home/ege/Pictures/Screenshots/Screenshot%20From%202026-09-04%2002-10-00.png"),
            Some(PathBuf::from("/home/ege/Pictures/Screenshots/Screenshot From 2026-09-04 02-10-00.png"))
        );
        assert_eq!(path_of("file://localhost/tmp/a.png"), Some(PathBuf::from("/tmp/a.png")));
        assert_eq!(path_of("https://example.com/a.png"), None);
        assert_eq!(path_of("file://host/a.png"), None);
        assert_eq!(path_of("file:///tmp/100%25.png"), Some(PathBuf::from("/tmp/100%.png")));
    }

    #[test]
    fn the_card_gets_the_file_name() {
        assert_eq!(name_for(std::path::Path::new("/tmp/Screenshot From 2026.png")), "Screenshot From 2026.png");
        assert!(name_for(std::path::Path::new("/")).starts_with("screenshot-"));
    }
}
