use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Entry {
    pub count: u64,
    pub last_used_unix: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Frecency {
    pub entries: HashMap<String, Entry>,
}

const HALF_LIFE_DAYS: f64 = 30.0;
const SECONDS_PER_DAY: f64 = 86_400.0;

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl Frecency {
    pub fn path() -> PathBuf {
        let mut dir = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
        dir.push("emojipick");
        dir.push("frecency.json");
        dir
    }

    pub fn load() -> Self {
        let path = Self::path();
        let contents = match std::fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(_) => return Self::default(),
        };
        match serde_json::from_str(&contents) {
            Ok(frecency) => frecency,
            Err(err) => {
                eprintln!("emojipick: corrupt frecency store ({err}); backing up");
                let _ = std::fs::rename(&path, path.with_extension("json.bak"));
                Self::default()
            }
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let contents = serde_json::to_string_pretty(self)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, contents)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    pub fn record(&mut self, emoji: &str) {
        let entry = self.entries.entry(emoji.to_string()).or_default();
        entry.count = entry.count.saturating_add(1);
        entry.last_used_unix = now_unix();
    }

    pub fn score(&self, emoji: &str) -> f64 {
        match self.entries.get(emoji) {
            Some(entry) => self.score_entry(entry),
            None => 0.0,
        }
    }

    fn score_entry(&self, entry: &Entry) -> f64 {
        if entry.count == 0 {
            return 0.0;
        }
        let now = now_unix();
        let elapsed = now.saturating_sub(entry.last_used_unix) as f64;
        let days = elapsed / SECONDS_PER_DAY;
        let decay = 0.5f64.powf(days / HALF_LIFE_DAYS);
        entry.count as f64 * decay
    }

    pub fn recent_n(&self, n: usize) -> Vec<String> {
        let mut entries: Vec<(&String, &Entry)> =
            self.entries.iter().filter(|(_, e)| e.count > 0).collect();
        entries.sort_by(|a, b| {
            b.1.last_used_unix
                .cmp(&a.1.last_used_unix)
                .then(b.1.count.cmp(&a.1.count))
                .then(a.0.cmp(b.0))
        });
        entries.into_iter().take(n).map(|(g, _)| g.clone()).collect()
    }
}
