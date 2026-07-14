mod config;
mod emoji;
mod emoji_object;
mod frecency;
mod hotkey;
mod inject;
mod ipc;
mod theme;
mod ui;

use crate::config::Config;
use crate::frecency::Frecency;
use crate::ui::AppState;
use gtk::gio::ApplicationFlags;
use gtk::prelude::*;
use gtk::Application;
use std::cell::RefCell;
use std::rc::Rc;

const APP_ID: &str = "org.emojipick.Emojipick";

fn main() -> anyhow::Result<()> {
    let arg = std::env::args().nth(1);
    match arg.as_deref() {
        Some("--daemon") => run_daemon(),
        Some("--toggle") => run_toggle(),
        Some("install-shortcut") => install_shortcut(),
        None => {
            if ipc::is_daemon_running() {
                run_toggle()
            } else {
                run_oneshot()
            }
        }
        Some(other) => {
            eprintln!("unknown argument: {other}");
            std::process::exit(2);
        }
    }
}

fn toggle(window: &gtk::ApplicationWindow) {
    if window.is_visible() {
        ui::hide(window);
    } else {
        ui::show(window);
    }
}

fn new_state(oneshot: bool) -> Rc<RefCell<AppState>> {
    Rc::new(RefCell::new(AppState {
        config: Config::load(),
        frecency: Frecency::load(),
        oneshot,
    }))
}

fn build_app() -> Application {
    Application::builder()
        .application_id(APP_ID)
        .flags(ApplicationFlags::NON_UNIQUE)
        .build()
}

fn run_oneshot() -> anyhow::Result<()> {
    let app = build_app();
    let state = new_state(true);
    app.connect_activate(move |app| {
        let window = ui::build_window(app, state.clone());
        ui::show(&window);
    });
    app.run_with_args::<&str>(&[]);
    Ok(())
}

fn run_daemon() -> anyhow::Result<()> {
    if ipc::is_daemon_running() {
        eprintln!("emojipick daemon already running");
        return Ok(());
    }
    let app = build_app();
    let state = new_state(false);
    let started = std::rc::Rc::new(std::cell::Cell::new(false));
    app.connect_activate(move |app| {
        if started.replace(true) {
            return;
        }
        let window = ui::build_window(app, state.clone());
        let hold = app.hold();
        std::mem::forget(hold);

        let socket_window = window.clone();
        if let Err(err) = ipc::serve(move || toggle(&socket_window)) {
            eprintln!("emojipick daemon failed to bind socket: {err:#}");
            app.quit();
            return;
        }

        let hotkey_window = window.clone();
        if let Err(err) = hotkey::serve(move || toggle(&hotkey_window)) {
            eprintln!("emojipick global hotkey unavailable: {err:#}");
        }
    });
    app.run_with_args::<&str>(&[]);
    Ok(())
}

fn run_toggle() -> anyhow::Result<()> {
    ipc::send_toggle()
}

fn install_shortcut() -> anyhow::Result<()> {
    let exe = std::env::current_exe()?;
    let exe = exe.to_string_lossy().to_string();

    install_kwin_rule();

    println!("emojipick owns its global shortcut natively (Meta+Space) via KGlobalAccel.");
    println!("The running daemon registers it on startup, so just enable the daemon:");
    println!();
    println!("    systemctl --user enable --now emojipick.service");
    println!();
    println!("Applied a KWin rule to center and float the picker window.");
    println!();
    println!("To change the key, use System Settings -> Keyboard -> Shortcuts,");
    println!("find \"{exe}\" / emojipick, and rebind \"Toggle emoji picker\".");
    Ok(())
}

const KWIN_RULE_GROUP: &str = "emojipick";

fn install_kwin_rule() {
    use std::process::Command;

    let entries = [
        ("Description", "emojipick centered palette"),
        ("wmclass", "emojipick"),
        ("wmclassmatch", "2"),
        ("wmclasscomplete", "false"),
        ("placement", "5"),
        ("placementrule", "2"),
        ("above", "true"),
        ("aboverule", "2"),
        ("skiptaskbar", "true"),
        ("skiptaskbarrule", "2"),
        ("skippager", "true"),
        ("skippagerrule", "2"),
        ("fsplevel", "0"),
        ("fsplevelrule", "2"),
    ];
    for (key, value) in entries {
        let _ = Command::new("kwriteconfig6")
            .args([
                "--file", "kwinrulesrc", "--group", KWIN_RULE_GROUP, "--key", key, value,
            ])
            .status();
    }

    register_rule_in_index();

    let _ = Command::new("qdbus6")
        .args(["org.kde.KWin", "/KWin", "reconfigure"])
        .status();
}

/// Append our rule to KWin's `[General] rules` index without clobbering the
/// user's other window rules. `rules` is the full ordered list of rule-group
/// names and `count` is its length; blindly writing `rules=emojipick`/`count=1`
/// would orphan every other rule the user has.
fn register_rule_in_index() {
    use std::process::Command;

    let existing = Command::new("kreadconfig6")
        .args(["--file", "kwinrulesrc", "--group", "General", "--key", "rules"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    let mut rules: Vec<String> = existing
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();

    if !rules.iter().any(|r| r == KWIN_RULE_GROUP) {
        rules.push(KWIN_RULE_GROUP.to_string());
    }

    let _ = Command::new("kwriteconfig6")
        .args([
            "--file", "kwinrulesrc", "--group", "General", "--key", "count",
            &rules.len().to_string(),
        ])
        .status();
    let _ = Command::new("kwriteconfig6")
        .args([
            "--file", "kwinrulesrc", "--group", "General", "--key", "rules",
            &rules.join(","),
        ])
        .status();
}
