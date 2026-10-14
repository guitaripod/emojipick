use crate::config::{Config, PasteShortcut};
use anyhow::{Context, Result};
use std::io::Write;
use std::os::unix::net::UnixDatagram;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

const YDOTOOL_UNITS: [&str; 2] = ["ydotool", "ydotoold"];
const SOCKET_POLL_ATTEMPTS: u32 = 50;
const SOCKET_POLL_INTERVAL: Duration = Duration::from_millis(100);
const SETTLE_BEFORE_PASTE: Duration = Duration::from_millis(120);
const RESTORE_AFTER_PASTE: Duration = Duration::from_millis(400);
const CLIPBOARD_READ_TIMEOUT_SECS: &str = "1";

/// Put `text` on the Wayland clipboard. It goes through stdin rather than argv so
/// a large previous clipboard can be restored without hitting the argument limit.
pub fn copy_to_clipboard(text: &str) -> Result<()> {
    let mut child = Command::new("wl-copy")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to spawn wl-copy")?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .context("failed to write to wl-copy")?;
    }
    let _ = child.wait();
    Ok(())
}

/// The current clipboard as text, or `None` when it is empty, holds no text
/// (an image, say), or the owner does not answer within a second.
fn read_text_clipboard() -> Option<String> {
    let output = Command::new("timeout")
        .args([
            CLIPBOARD_READ_TIMEOUT_SECS,
            "wl-paste",
            "--no-newline",
            "--type",
            "text",
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .filter(|text| !text.is_empty())
}

fn ydotool_socket_path() -> PathBuf {
    let uid = unsafe { libc::getuid() };
    PathBuf::from(format!("/run/user/{uid}/.ydotool_socket"))
}

/// A socket file outlives the daemon that bound it, so its existence proves
/// nothing; a datagram `connect` only succeeds while something is listening.
fn ydotoold_alive(socket: &Path) -> bool {
    UnixDatagram::unbound()
        .and_then(|probe| probe.connect(socket))
        .is_ok()
}

fn systemctl_user(args: &[&str]) -> bool {
    Command::new("systemctl")
        .arg("--user")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Start the packaged ydotool user unit, whose name differs between distros.
/// `reset-failed` first because a boot-time failure (uinput not yet writable)
/// can leave the unit stuck on its start limit.
fn start_ydotoold_unit() -> bool {
    YDOTOOL_UNITS.iter().any(|unit| {
        systemctl_user(&["reset-failed", unit]);
        systemctl_user(&["start", unit])
    })
}

/// Last resort when no unit exists. The child inherits this service's cgroup, so
/// it is reaped from a thread and still dies with the daemon.
fn spawn_ydotoold() {
    let child = Command::new("ydotoold")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    if let Ok(mut child) = child {
        thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

pub fn ensure_ydotoold() -> Result<()> {
    let socket = ydotool_socket_path();
    if ydotoold_alive(&socket) {
        return Ok(());
    }

    if !start_ydotoold_unit() {
        spawn_ydotoold();
    }

    for _ in 0..SOCKET_POLL_ATTEMPTS {
        if ydotoold_alive(&socket) {
            return Ok(());
        }
        thread::sleep(SOCKET_POLL_INTERVAL);
    }

    Err(anyhow::anyhow!("ydotoold socket did not appear"))
}

pub fn paste(shortcut: PasteShortcut) -> Result<()> {
    let status = Command::new("ydotool")
        .arg("key")
        .args(shortcut.keycodes())
        .env("YDOTOOL_SOCKET", ydotool_socket_path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("failed to run ydotool")?;
    if !status.success() {
        return Err(anyhow::anyhow!("ydotool paste exited with failure"));
    }
    Ok(())
}

/// Paste into the focused window, then put the user's previous clipboard text
/// back. A failed paste leaves the emoji on the clipboard so it can be pasted by hand.
fn auto_paste(config: &Config, previous: Option<String>) {
    if let Err(err) = ensure_ydotoold() {
        eprintln!("emojipick: auto-paste unavailable: {err:#}");
        return;
    }
    thread::sleep(SETTLE_BEFORE_PASTE);
    if let Err(err) = paste(config.paste_shortcut) {
        eprintln!("emojipick: auto-paste failed: {err:#}");
        return;
    }
    if let Some(previous) = previous {
        thread::sleep(RESTORE_AFTER_PASTE);
        if let Err(err) = copy_to_clipboard(&previous) {
            eprintln!("emojipick: could not restore the clipboard: {err:#}");
        }
    }
}

pub fn insert(emoji: &str, config: &Config, blocking: bool) -> Result<()> {
    let previous = if config.auto_paste && config.restore_clipboard {
        read_text_clipboard()
    } else {
        None
    };

    copy_to_clipboard(emoji)?;

    if config.auto_paste {
        if blocking {
            auto_paste(config, previous);
        } else {
            let config = config.clone();
            thread::spawn(move || auto_paste(&config, previous));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_socket(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("emojipick-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_file(&path);
        path
    }

    #[test]
    fn live_socket_is_alive() {
        let path = scratch_socket("live");
        let _listener = UnixDatagram::bind(&path).unwrap();
        assert!(ydotoold_alive(&path));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn stale_socket_file_is_not_alive() {
        let path = scratch_socket("stale");
        drop(UnixDatagram::bind(&path).unwrap());
        assert!(path.exists());
        assert!(!ydotoold_alive(&path));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_socket_is_not_alive() {
        assert!(!ydotoold_alive(&scratch_socket("missing")));
    }
}
