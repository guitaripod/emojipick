use anyhow::{Context, Result};
use gtk::glib;
use gtk::prelude::*;
use std::thread;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedValue, Value};

const DEST: &str = "org.freedesktop.portal.Desktop";
const OBJECT: &str = "/org/freedesktop/portal/desktop";
const IFACE: &str = "org.freedesktop.portal.Settings";
const NAMESPACE: &str = "org.freedesktop.appearance";
const KEY: &str = "color-scheme";

/// Track the desktop's light/dark preference and report it as a `bool` (dark).
///
/// `on_change` is invoked once synchronously with the current value, then again
/// on the GTK main thread every time the XDG portal's
/// `org.freedesktop.appearance color-scheme` changes. It is always called at
/// least once, even when no portal is reachable, so callers can rely on it to
/// paint their initial theme. We also mirror the value into GTK's
/// `gtk-application-prefer-dark-theme` so base-themed popovers/tooltips follow.
pub fn watch<F: Fn(bool) + 'static>(on_change: F) {
    let conn = Connection::session().ok();
    let initial = conn.as_ref().and_then(read_scheme).unwrap_or(0);
    apply_prefer_dark(initial);
    on_change(prefer_dark(initial));

    let Some(conn) = conn else {
        return;
    };

    let (tx, rx) = async_channel::unbounded::<u32>();

    thread::spawn(move || {
        if let Err(err) = listen(&conn, &tx) {
            eprintln!("emojipick: color-scheme updates unavailable: {err:#}");
        }
    });

    glib::spawn_future_local(async move {
        while let Ok(scheme) = rx.recv().await {
            apply_prefer_dark(scheme);
            on_change(prefer_dark(scheme));
        }
    });
}

fn apply_prefer_dark(scheme: u32) {
    if let Some(settings) = gtk::Settings::default() {
        settings.set_property("gtk-application-prefer-dark-theme", prefer_dark(scheme));
    }
}

/// Portal semantics: 0 = no preference, 1 = prefer dark, 2 = prefer light.
fn prefer_dark(scheme: u32) -> bool {
    scheme == 1
}

fn read_scheme(conn: &Connection) -> Option<u32> {
    let proxy = Proxy::new(conn, DEST, OBJECT, IFACE).ok()?;
    let value: OwnedValue = proxy
        .call("ReadOne", &(NAMESPACE, KEY))
        .or_else(|_| proxy.call("Read", &(NAMESPACE, KEY)))
        .ok()?;
    scheme_from_value(&value)
}

fn scheme_from_value(value: &Value) -> Option<u32> {
    match value {
        Value::U32(n) => Some(*n),
        Value::U8(n) => Some(*n as u32),
        Value::I32(n) => Some(*n as u32),
        Value::Value(inner) => scheme_from_value(inner),
        _ => None,
    }
}

fn listen(conn: &Connection, tx: &async_channel::Sender<u32>) -> Result<()> {
    let proxy = Proxy::new(conn, DEST, OBJECT, IFACE).context("portal Settings proxy")?;
    let signal = proxy
        .receive_signal("SettingChanged")
        .context("subscribe to SettingChanged")?;

    for msg in signal {
        let body = msg.body();
        let Ok((namespace, key, value)) = body.deserialize::<(String, String, OwnedValue)>() else {
            continue;
        };
        if namespace != NAMESPACE || key != KEY {
            continue;
        }
        let Some(scheme) = scheme_from_value(&value) else {
            continue;
        };
        if tx.send_blocking(scheme).is_err() {
            break;
        }
    }

    Ok(())
}
