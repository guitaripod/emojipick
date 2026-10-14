use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The key chord `ydotool` sends to paste the picked emoji. Terminals reserve
/// plain Ctrl+V, so the right chord depends on where the emoji is going.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PasteShortcut {
    #[default]
    CtrlV,
    CtrlShiftV,
    ShiftInsert,
}

impl PasteShortcut {
    /// Linux input keycodes in `ydotool key` syntax (`code:1` press, `code:0` release).
    pub fn keycodes(self) -> &'static [&'static str] {
        match self {
            Self::CtrlV => &["29:1", "47:1", "47:0", "29:0"],
            Self::CtrlShiftV => &["29:1", "42:1", "47:1", "47:0", "42:0", "29:0"],
            Self::ShiftInsert => &["42:1", "110:1", "110:0", "42:0"],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub auto_paste: bool,
    pub paste_shortcut: PasteShortcut,
    pub restore_clipboard: bool,
    pub skin_tone: u8,
    pub grid_columns: u32,
    pub scale: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            auto_paste: true,
            paste_shortcut: PasteShortcut::default(),
            restore_clipboard: false,
            skin_tone: 0,
            grid_columns: 9,
            scale: 1.0,
        }
    }
}

pub const SCALE_MIN: f32 = 0.7;
pub const SCALE_MAX: f32 = 3.0;
pub const SCALE_STEP: f32 = 0.1;

/// Snap a scale factor to one decimal so repeated `Ctrl +/-` steps don't drift
/// into values like `1.9000002`.
pub fn round_scale(scale: f32) -> f32 {
    (scale * 10.0).round() / 10.0
}

impl Config {
    pub fn path() -> PathBuf {
        let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        base.join("emojipick").join("config.toml")
    }

    pub fn load() -> Self {
        let path = Self::path();
        let raw = match std::fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(_) => return Self::default(),
        };
        let mut config = Self::parse(&raw).unwrap_or_else(|err| {
            Self::back_up_unreadable(&path, &err.to_string());
            Self::default()
        });
        config.normalize();
        config
    }

    fn parse(raw: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(raw)
    }

    /// Move a config that fails to parse aside before the next save would
    /// overwrite it with defaults, so a typo never silently destroys the user's file.
    fn back_up_unreadable(path: &std::path::Path, reason: &str) {
        let backup = path.with_extension("toml.bak");
        eprintln!(
            "emojipick: unreadable config ({reason}); using defaults, original kept at {}",
            backup.display()
        );
        let _ = std::fs::rename(path, backup);
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let raw = toml::to_string_pretty(self)?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, raw)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    fn normalize(&mut self) {
        if self.skin_tone > 5 {
            self.skin_tone = 5;
        }
        if self.grid_columns == 0 {
            self.grid_columns = 1;
        }
        if !self.scale.is_finite() {
            self.scale = 1.0;
        }
        self.scale = round_scale(self.scale.clamp(SCALE_MIN, SCALE_MAX));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_file_yields_defaults() {
        let config = Config::parse("").unwrap();
        assert!(config.auto_paste);
        assert!(!config.restore_clipboard);
        assert_eq!(config.paste_shortcut, PasteShortcut::CtrlV);
    }

    #[test]
    fn paste_shortcut_reads_kebab_case() {
        let config = Config::parse("paste_shortcut = \"ctrl-shift-v\"").unwrap();
        assert_eq!(config.paste_shortcut, PasteShortcut::CtrlShiftV);
        let config = Config::parse("paste_shortcut = \"shift-insert\"").unwrap();
        assert_eq!(config.paste_shortcut, PasteShortcut::ShiftInsert);
    }

    #[test]
    fn unknown_paste_shortcut_is_an_error() {
        assert!(Config::parse("paste_shortcut = \"ctrl-p\"").is_err());
    }

    #[test]
    fn every_shortcut_releases_what_it_presses() {
        for shortcut in [
            PasteShortcut::CtrlV,
            PasteShortcut::CtrlShiftV,
            PasteShortcut::ShiftInsert,
        ] {
            let codes = shortcut.keycodes();
            let pressed: Vec<&str> = codes.iter().filter_map(|c| c.strip_suffix(":1")).collect();
            let mut released: Vec<&str> =
                codes.iter().filter_map(|c| c.strip_suffix(":0")).collect();
            released.reverse();
            assert_eq!(
                pressed, released,
                "{shortcut:?} must release in reverse order"
            );
        }
    }

    #[test]
    fn normalize_clamps_out_of_range_values() {
        let mut config = Config::parse("skin_tone = 9\ngrid_columns = 0\nscale = 99.0").unwrap();
        config.normalize();
        assert_eq!(config.skin_tone, 5);
        assert_eq!(config.grid_columns, 1);
        assert_eq!(config.scale, SCALE_MAX);
    }

    #[test]
    fn unreadable_config_is_moved_aside_not_deleted() {
        let dir =
            std::env::temp_dir().join(format!("emojipick-config-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "auto_paste = maybe").unwrap();

        Config::back_up_unreadable(&path, "test");

        assert!(!path.exists());
        assert_eq!(
            std::fs::read_to_string(dir.join("config.toml.bak")).unwrap(),
            "auto_paste = maybe"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
