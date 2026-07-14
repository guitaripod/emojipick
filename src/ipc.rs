use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use anyhow::Context;
use gtk::glib;

pub const TOGGLE: &str = "toggle";

pub fn socket_path() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    dir.join("emojipick.sock")
}

pub fn send_toggle() -> anyhow::Result<()> {
    let mut stream = UnixStream::connect(socket_path())
        .context("no emojipick daemon listening on socket")?;
    stream.write_all(TOGGLE.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    Ok(())
}

pub fn is_daemon_running() -> bool {
    let path = socket_path();
    match UnixStream::connect(&path) {
        Ok(_) => true,
        Err(err) => {
            use std::io::ErrorKind;
            if matches!(err.kind(), ErrorKind::ConnectionRefused | ErrorKind::NotFound) {
                remove_stale_socket(&path);
            }
            false
        }
    }
}

fn remove_stale_socket(path: &PathBuf) {
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }
}

pub fn serve<F: Fn() + 'static>(on_toggle: F) -> anyhow::Result<()> {
    let path = socket_path();
    remove_stale_socket(&path);

    let listener = UnixListener::bind(&path)
        .with_context(|| format!("failed to bind emojipick socket at {}", path.display()))?;

    let (tx, rx) = async_channel::unbounded::<()>();

    thread::spawn(move || {
        for stream in listener.incoming() {
            let stream = match stream {
                Ok(s) => s,
                Err(_) => continue,
            };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            if reader.read_line(&mut line).is_ok()
                && line.trim() == TOGGLE
                && tx.send_blocking(()).is_err()
            {
                break;
            }
        }
    });

    glib::spawn_future_local(async move {
        while rx.recv().await.is_ok() {
            on_toggle();
        }
    });

    Ok(())
}
