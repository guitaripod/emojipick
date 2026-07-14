# emojipick

A fast, frecency-ranked emoji picker for **KDE Plasma on Wayland**, written in Rust + GTK4.

It runs as a resident daemon that owns a native global shortcut (via KGlobalAccel), so it shows up instantly — no per-launch startup cost. Emoji render as native color glyphs through Pango, search is fuzzy and tiered, and the emoji you actually use float to the top.

## Features

- **Instant show** — a resident daemon toggles one window over a Unix socket and a native KGlobalAccel global shortcut (default **Meta+Space**).
- **Frecency ranking** — the default view is your most-recently-used emoji first; search is tiered (exact → prefix → word-boundary → fuzzy) with a frecency tiebreak.
- **Virtualized grid** — a `GridView` recycles a handful of widgets regardless of result count, so typing stays smooth across the full Unicode set.
- **Keyboard-first** — type to filter, arrows/Home/End/PageUp-Down to move, Tab/Shift+Tab to cycle categories, Enter to insert. Full **vim-style** navigation with Ctrl.
- **Skin tones** — pick a default tone in-UI (or `Ctrl+0..5`), applied across the whole set.
- **Persistent UI scale** — grow or shrink the whole picker with `Ctrl +` / `Ctrl -`, remembered across restarts.
- **Insertion** — always copies to the clipboard (`wl-copy`); optionally auto-pastes into the focused window via `ydotool`.

## Install

### Arch Linux (AUR)

```sh
paru -S emojipick     # or: yay -S emojipick
```

### crates.io

```sh
cargo install emojipick
```

### From source

```sh
git clone https://github.com/guitaripod/emojipick
cd emojipick
cargo build --release
install -Dm755 target/release/emojipick ~/.local/bin/emojipick
```

## Setup

Enable the daemon (instant show) and bind the shortcut:

```sh
# run + enable the resident daemon
systemctl --user enable --now emojipick.service   # if installed from a package
# or, from a manual build:
emojipick --daemon &

# the daemon registers Meta+Space itself via KGlobalAccel on startup.
# apply the optional KWin rule that centers/floats the window:
emojipick install-shortcut
```

Press **Meta+Space** to toggle the picker.

By default emojipick pastes the picked emoji straight into the focused app (via `ydotool`) in addition to copying it. Install `ydotool` for that to work; to only copy to the clipboard instead, set `auto_paste = false` in the config.

## Keybindings

| Key | Action |
| --- | --- |
| type | filter (works from anywhere in the window) |
| `Enter` / click | insert the selected emoji |
| `Esc` | clear query/category, then close |
| `Tab` / `Shift+Tab` | next / previous category |
| `Down` | enter the grid from search |
| arrows, `Home`/`End`, `PageUp`/`PageDown` | move selection |
| `Ctrl+h` / `Ctrl+l` | left / right |
| `Ctrl+j` / `Ctrl+k` | down / up |
| `Ctrl+d` / `Ctrl+u` | half page down / up |
| `Ctrl+f` / `Ctrl+b` | full page down / up |
| `Ctrl+0`..`Ctrl+5` | skin tone |
| `Ctrl +` / `Ctrl -` | scale UI up / down |

## Configuration

`~/.config/emojipick/config.toml`:

```toml
auto_paste = true    # also paste (Ctrl+V via ydotool) after copying; false = copy only
skin_tone = 0        # 0 = default, 1..5 = light..dark
grid_columns = 9
scale = 1.0          # UI scale factor (0.7 – 3.0)
```

Frecency data lives in `~/.local/share/emojipick/frecency.json`.

## Requirements

- KDE Plasma 6 on Wayland (KWin) — the global shortcut uses KGlobalAccel.
- `gtk4`, `wl-clipboard` (for `wl-copy`), a color emoji font (`noto-fonts-emoji`).
- `ydotool` (+ `ydotoold`) for the default auto-paste; without it emojipick falls back to copy-only.

## Why clipboard + paste instead of typing?

Emoji have no keysym, so synthetic typing can't produce them reliably on Wayland. emojipick always copies (universal and instant) and optionally pastes via `ydotool`, which is the robust path on KWin.

## License

GPL-3.0
