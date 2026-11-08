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

/// All settings. Secrets (VT key, PATs) are runtime-only here: they are
/// read for migration from old plaintext files but never written. They
/// live in Windows Credential Manager (`core::secrets`, user decision
/// 2026-11-06).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    #[serde(skip_serializing)]
    pub vt_api_key: String,
    /// Upload unknown files to VirusTotal for scanning (default on).
    pub vt_upload_fallback: bool,
    pub download_dir: PathBuf,
    #[serde(skip_serializing)]
    pub github_pat: Option<String>,
    #[serde(skip_serializing)]
    pub gitlab_pat: Option<String>,
    /// Parallel repo fetches during refresh.
    pub refresh_concurrency: u32,
    pub theme: ThemeChoice,
    /// Max lines kept in the in-memory log.
    pub log_ring_size: u32,
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
            log_ring_size: 500,
        }
    }
}

impl Config {
    /// Load from `path`. A missing file is `Missing` (first run — show
    /// defaults silently); anything else is `Corrupt` (warn before any
    /// settings save overwrites the file).
    pub fn load(path: &Path) -> Result<Config, LoadError> {
        let text = std::fs::read_to_string(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => LoadError::Missing,
            _ => LoadError::Corrupt(format!("read config: {e}")),
        })?;
        toml::from_str(&text).map_err(|e| LoadError::Corrupt(format!("parse config: {e}")))
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let text =
            toml::to_string_pretty(self).map_err(|e| format!("serialize config: {e}"))?;
        std::fs::write(path, text).map_err(|e| format!("write config: {e}"))
    }
}

/// Why a config load failed. `Missing` is normal first-run state;
/// `Corrupt` must be surfaced before the user saves over the file.
#[derive(Debug, PartialEq, Eq)]
pub enum LoadError {
    Missing,
    Corrupt(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_roundtrip_preserves_settings_but_drops_secrets() {
        let cfg = Config {
            vt_api_key: "vtkey-123".into(),
            vt_upload_fallback: false,
            download_dir: PathBuf::from("D:/tmp/dl"),
            github_pat: Some("ghp_x".into()),
            gitlab_pat: Some("glpat_y".into()),
            refresh_concurrency: 7,
            theme: ThemeChoice::Dark,
            log_ring_size: 1234,
        };
        let dir = std::env::temp_dir().join("girust_cfg_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        cfg.save(&path).unwrap();
        let back = Config::load(&path).unwrap();
        // Secrets never reach the file (they live in the OS keyring).
        let expected = Config {
            vt_api_key: String::new(),
            github_pat: None,
            gitlab_pat: None,
            ..cfg
        };
        assert_eq!(back, expected);
    }

    #[test]
    fn default_download_dir_is_exe_relative() {
        let cfg = Config::default();
        assert!(cfg.download_dir.starts_with(crate::core::paths::app_dir()));
    }

    #[test]
    fn missing_config_file_maps_to_missing_not_corrupt() {
        let p = std::env::temp_dir().join("girust_cfg_missing.toml");
        let _ = std::fs::remove_file(&p);
        assert_eq!(Config::load(&p), Err(LoadError::Missing));
    }

    #[test]
    fn missing_config_error_is_not_corrupt_so_no_banner() {
        // Regression: Windows reads a missing file as
        // "The system cannot find the file specified. (os error 2)"; the
        // old string-sniffing treated that as corruption and showed the
        // overwrite-warning banner on every first run.
        let p = std::env::temp_dir().join("girust_cfg_missing2.toml");
        let _ = std::fs::remove_file(&p);
        if let Err(LoadError::Corrupt(_)) = Config::load(&p) {
            panic!("missing file must not be Corrupt");
        }
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
