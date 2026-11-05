//! App configuration, stored as `config.toml` next to the exe.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// UI theme choice. Dark-only for now (design system: light not recommended);
/// enum kept so settings UI can list it without a format break later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeChoice {
    #[default]
    Dark,
}

/// All settings. Plaintext secrets (VT key, PATs) are an accepted
/// portability trade-off, decided by the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub vt_api_key: String,
    /// Upload unknown files to VirusTotal for scanning (default on).
    pub vt_upload_fallback: bool,
    pub download_dir: PathBuf,
    pub github_pat: Option<String>,
    pub gitlab_pat: Option<String>,
    /// Parallel repo fetches during refresh.
    pub refresh_concurrency: u32,
    pub theme: ThemeChoice,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            vt_api_key: String::new(),
            vt_upload_fallback: true,
            // Exe-relative, not CWD-relative (portability — review fix).
            download_dir: crate::core::paths::default_download_dir(),
            github_pat: None,
            gitlab_pat: None,
            refresh_concurrency: 4,
            theme: ThemeChoice::Dark,
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Config, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("read config: {e}"))?;
        toml::from_str(&text).map_err(|e| format!("parse config: {e}"))
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let text =
            toml::to_string_pretty(self).map_err(|e| format!("serialize config: {e}"))?;
        std::fs::write(path, text).map_err(|e| format!("write config: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_roundtrip_preserves_every_field() {
        let cfg = Config {
            vt_api_key: "vtkey-123".into(),
            vt_upload_fallback: false,
            download_dir: PathBuf::from("D:/tmp/dl"),
            github_pat: Some("ghp_x".into()),
            gitlab_pat: Some("glpat_y".into()),
            refresh_concurrency: 7,
            theme: ThemeChoice::Dark,
        };
        let dir = std::env::temp_dir().join("girust_cfg_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        cfg.save(&path).unwrap();
        let back = Config::load(&path).unwrap();
        assert_eq!(back, cfg);
    }

    #[test]
    fn default_download_dir_is_exe_relative() {
        let cfg = Config::default();
        assert!(cfg.download_dir.starts_with(crate::core::paths::app_dir()));
    }

    #[test]
    fn missing_config_file_maps_to_error_not_panic() {
        let p = std::env::temp_dir().join("girust_cfg_missing.toml");
        let _ = std::fs::remove_file(&p);
        assert!(Config::load(&p).is_err());
    }

    #[test]
    fn partial_config_file_fills_defaults() {
        let dir = std::env::temp_dir().join("girust_cfg_partial");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "vt_api_key = \"k\"\n").unwrap();
        let back = Config::load(&path).unwrap();
        assert_eq!(back.vt_api_key, "k");
        assert_eq!(back.refresh_concurrency, 4); // default
        assert!(back.vt_upload_fallback); // default
    }
}
