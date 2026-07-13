use crate::config::Config;
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

pub fn copy_to_clipboard(text: &str) -> Result<()> {
    let mut child = Command::new("wl-copy")
        .arg("--")
        .arg(text)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to spawn wl-copy")?;
    let _ = child.wait();
    Ok(())
}

fn ydotool_socket_path() -> PathBuf {
    let uid = unsafe { libc::getuid() };
    PathBuf::from(format!("/run/user/{uid}/.ydotool_socket"))
}

pub fn ensure_ydotoold() -> Result<()> {
    let socket = ydotool_socket_path();
    if socket.exists() {
        return Ok(());
    }

    let started = Command::new("systemctl")
        .args(["--user", "start", "ydotoold"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !started {
        let _ = Command::new("ydotoold")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }

    for _ in 0..50 {
        if socket.exists() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }

    Err(anyhow::anyhow!("ydotoold socket did not appear"))
}

pub fn paste() -> Result<()> {
    let socket = ydotool_socket_path();
    let status = Command::new("ydotool")
        .args(["key", "29:1", "47:1", "47:0", "29:0"])
        .env("YDOTOOL_SOCKET", socket)
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

pub fn insert(emoji: &str, config: &Config, blocking: bool) -> Result<()> {
    copy_to_clipboard(emoji)?;

    if config.auto_paste {
        if blocking {
            if let Err(err) = ensure_ydotoold() {
                eprintln!("emojipick: auto-paste unavailable: {err:#}");
            } else {
                thread::sleep(Duration::from_millis(120));
                let _ = paste();
            }
        } else {
            thread::spawn(|| {
                if let Err(err) = ensure_ydotoold() {
                    eprintln!("emojipick: auto-paste unavailable: {err:#}");
                    return;
                }
                thread::sleep(Duration::from_millis(120));
                let _ = paste();
            });
        }
    }

    Ok(())
}
