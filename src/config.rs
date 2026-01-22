use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::protocol::{MAX_BRIGHTNESS, MAX_TEMPERATURE, MIN_BRIGHTNESS, MIN_TEMPERATURE};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub power: bool,
    pub brightness: u16,
    pub temperature: u16,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            power: false,
            brightness: (MIN_BRIGHTNESS + MAX_BRIGHTNESS) / 2,
            temperature: (MIN_TEMPERATURE + MAX_TEMPERATURE) / 2,
        }
    }
}

impl Config {
    fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("litra").join("config.json"))
    }

    pub fn load() -> Self {
        Self::config_path()
            .and_then(|path| fs::read_to_string(path).ok())
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(path) = Self::config_path() else {
            return;
        };

        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        if let Ok(content) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, content);
        }
    }
}
