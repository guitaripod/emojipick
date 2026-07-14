use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub auto_paste: bool,
    pub skin_tone: u8,
    pub grid_columns: u32,
    pub scale: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self { auto_paste: true, skin_tone: 0, grid_columns: 9, scale: 1.0 }
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
        let mut config: Self = toml::from_str(&raw).unwrap_or_default();
        config.normalize();
        config
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
