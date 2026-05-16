use std::net::SocketAddr;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Persisted LUCHS configuration. Lives at
/// `dirs::config_dir()/luchs/config.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LuchsConfig {
    #[serde(default = "default_osc_endpoints")]
    pub osc_endpoints: Vec<String>,
    #[serde(default = "default_phrase_address")]
    pub phrase_address: String,
    #[serde(default = "default_beat_address")]
    pub beat_address: String,
    #[serde(default)]
    pub forward_all_decks: bool,
}

fn default_osc_endpoints() -> Vec<String> {
    Vec::new()
}

fn default_phrase_address() -> String {
    "/luchs/phrase".to_string()
}

fn default_beat_address() -> String {
    "/luchs/beat".to_string()
}

impl Default for LuchsConfig {
    fn default() -> Self {
        Self {
            osc_endpoints: Vec::new(),
            phrase_address: default_phrase_address(),
            beat_address: default_beat_address(),
            forward_all_decks: false,
        }
    }
}

impl LuchsConfig {
    pub fn config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("luchs")
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.json")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                log::warn!("luchs config parse failed ({}); using defaults", e);
                LuchsConfig::default()
            }),
            Err(_) => LuchsConfig::default(),
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let dir = Self::config_dir();
        std::fs::create_dir_all(&dir)?;
        let path = Self::config_path();
        let text = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, text)
    }

    /// Parse endpoint strings as `SocketAddr`. Ignores malformed entries.
    pub fn parsed_endpoints(&self) -> Vec<SocketAddr> {
        self.osc_endpoints
            .iter()
            .filter_map(|s| s.parse().ok())
            .collect()
    }
}
