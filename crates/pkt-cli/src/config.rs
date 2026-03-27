// v19.6 — PKT CLI config: ~/.pkt/cli.toml

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConfig {
    pub node_url: String,
}

impl Default for CliConfig {
    fn default() -> Self {
        Self { node_url: "https://oceif.com".to_string() }
    }
}

pub fn config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".pkt").join("cli.toml")
}

pub fn load_config() -> CliConfig {
    let path = config_path();
    if path.exists() {
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    } else {
        CliConfig::default()
    }
}

pub fn save_config(cfg: &CliConfig) -> std::io::Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let s = toml::to_string_pretty(cfg).expect("toml serialize");
    std::fs::write(path, s)
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_path_ends_correctly() {
        let p = config_path();
        assert!(p.ends_with(".pkt/cli.toml"), "path = {p:?}");
    }

    #[test]
    fn default_node_url() {
        let cfg = CliConfig::default();
        assert_eq!(cfg.node_url, "https://oceif.com");
    }

    #[test]
    fn serialize_deserialize_roundtrip() {
        let original = CliConfig { node_url: "http://localhost:3000".to_string() };
        let s = toml::to_string_pretty(&original).unwrap();
        let parsed: CliConfig = toml::from_str(&s).unwrap();
        assert_eq!(parsed.node_url, original.node_url);
    }

    #[test]
    fn deserialize_missing_falls_back_to_default() {
        let result: Result<CliConfig, _> = toml::from_str("bad = 123");
        // missing node_url field → deserialization lỗi → unwrap_or_default → default URL
        let cfg = result.unwrap_or_default();
        assert_eq!(cfg.node_url, "https://oceif.com");
    }
}
