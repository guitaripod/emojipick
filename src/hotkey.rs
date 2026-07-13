use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use gtk::glib;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::OwnedObjectPath;

const COMPONENT: &str = "emojipick";
const ACTION: &str = "toggle";
const FRIENDLY: &str = "Toggle emoji picker";

const KEY_META_SPACE: i32 = 0x1000_0000 | 0x20;
const SET_PRESENT: u32 = 2;

fn action_id() -> Vec<String> {
    vec![
        COMPONENT.to_string(),
        ACTION.to_string(),
        COMPONENT.to_string(),
        FRIENDLY.to_string(),
    ]
}

pub fn serve<F: Fn() + 'static>(on_toggle: F) -> Result<()> {
    let (tx, rx) = async_channel::unbounded::<()>();

    thread::spawn(move || {
        if let Err(err) = register_and_listen(tx) {
            eprintln!("emojipick: global hotkey unavailable: {err:#}");
        }
    });

    glib::spawn_future_local(async move {
        while rx.recv().await.is_ok() {
            on_toggle();
        }
    });

    Ok(())
}

fn register_and_listen(tx: async_channel::Sender<()>) -> Result<()> {
    let conn = connect_with_retry()?;

    let kga = Proxy::new(
        &conn,
        "org.kde.kglobalaccel",
        "/kglobalaccel",
        "org.kde.KGlobalAccel",
    )?;

    let id = action_id();
    let _: () = kga
        .call("doRegister", &(id.clone(),))
        .context("doRegister failed")?;

    let keys: Vec<(Vec<i32>,)> = vec![(vec![KEY_META_SPACE, 0, 0, 0],)];
    let _: Vec<(Vec<i32>,)> = kga
        .call("setShortcutKeys", &(id.clone(), keys, SET_PRESENT))
        .context("setShortcutKeys failed")?;

    let component: OwnedObjectPath = kga
        .call("getComponent", &(COMPONENT,))
        .context("getComponent failed")?;

    let component = Proxy::new(
        &conn,
        "org.kde.kglobalaccel",
        component,
        "org.kde.kglobalaccel.Component",
    )?;

    let signal = component
        .receive_signal("globalShortcutPressed")
        .context("failed to subscribe to globalShortcutPressed")?;

    eprintln!("emojipick: registered global hotkey Meta+Space");

    for _ in signal {
        if tx.send_blocking(()).is_err() {
            break;
        }
    }

    Ok(())
}

fn connect_with_retry() -> Result<Connection> {
    let mut last = None;
    for _ in 0..30 {
        match Connection::session() {
            Ok(conn) => return Ok(conn),
            Err(err) => {
                last = Some(err);
                thread::sleep(Duration::from_millis(500));
            }
        }
    }
    Err(last.unwrap()).context("could not reach the session bus for KGlobalAccel")
}
