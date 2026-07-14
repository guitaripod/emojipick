use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use gtk::glib;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::OwnedObjectPath;

const COMPONENT: &str = "emojipick";
const ACTION: &str = "toggle";
const FRIENDLY: &str = "Toggle emoji picker";

const KGA_SERVICE: &str = "org.kde.kglobalaccel";
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
        if let Err(err) = run(tx) {
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

fn run(tx: async_channel::Sender<()>) -> Result<()> {
    let conn = connect_with_retry()?;
    register(&conn)?;

    {
        let conn = conn.clone();
        thread::spawn(move || {
            if let Err(err) = watch_restart(&conn) {
                eprintln!("emojipick: hotkey restart watcher stopped: {err:#}");
            }
        });
    }

    listen(&conn, &tx)
}

/// Deliver Meta+Space presses forever, re-arming if the signal stream ends
/// (session bus hiccup). A dead stream that never ends is handled instead by
/// [`watch_restart`], which re-registers when kglobalacceld comes back.
fn listen(conn: &Connection, tx: &async_channel::Sender<()>) -> Result<()> {
    loop {
        let component = component_proxy(conn)?;
        let signal = component
            .receive_signal("globalShortcutPressed")
            .context("failed to subscribe to globalShortcutPressed")?;

        for _ in signal {
            if tx.send_blocking(()).is_err() {
                return Ok(());
            }
        }

        thread::sleep(Duration::from_millis(500));
        let _ = register(conn);
    }
}

/// KGlobalAccel keeps shortcut registrations in memory, so a kglobalacceld
/// restart silently drops ours. Re-register whenever the service reappears.
fn watch_restart(conn: &Connection) -> Result<()> {
    let dbus = Proxy::new(
        conn,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )?;
    let signal = dbus
        .receive_signal("NameOwnerChanged")
        .context("failed to subscribe to NameOwnerChanged")?;

    for msg in signal {
        let body = msg.body();
        let Ok((name, _old, new_owner)) = body.deserialize::<(String, String, String)>() else {
            continue;
        };
        if name == KGA_SERVICE && !new_owner.is_empty() {
            thread::sleep(Duration::from_millis(300));
            if let Err(err) = register(conn) {
                eprintln!("emojipick: re-register after kglobalaccel restart failed: {err:#}");
            }
        }
    }

    Ok(())
}

fn register(conn: &Connection) -> Result<()> {
    let kga = Proxy::new(conn, KGA_SERVICE, "/kglobalaccel", "org.kde.KGlobalAccel")?;

    let id = action_id();
    let _: () = kga
        .call("doRegister", &(id.clone(),))
        .context("doRegister failed")?;

    let keys: Vec<(Vec<i32>,)> = vec![(vec![KEY_META_SPACE, 0, 0, 0],)];
    let assigned: Vec<(Vec<i32>,)> = kga
        .call("setShortcutKeys", &(id.clone(), keys, SET_PRESENT))
        .context("setShortcutKeys failed")?;

    if assigned.iter().any(|(k,)| k.contains(&KEY_META_SPACE)) {
        eprintln!("emojipick: registered global hotkey Meta+Space");
    } else {
        eprintln!(
            "emojipick: WARNING could not bind Meta+Space (already in use?); rebind via System Settings > Shortcuts"
        );
    }

    Ok(())
}

fn component_proxy(conn: &Connection) -> Result<Proxy<'static>> {
    let kga = Proxy::new(conn, KGA_SERVICE, "/kglobalaccel", "org.kde.KGlobalAccel")?;
    let component: OwnedObjectPath = kga
        .call("getComponent", &(COMPONENT,))
        .context("getComponent failed")?;
    Proxy::new(
        conn,
        KGA_SERVICE,
        component,
        "org.kde.kglobalaccel.Component",
    )
    .map_err(Into::into)
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
