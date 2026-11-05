//! App state: repo list, VT verdict cache, statistics.
//! Persisted as `repos.json` next to the exe, written atomically.

use super::models::{Repo, VtVerdict};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppState {
    #[serde(default)]
    pub repos: Vec<Repo>,
    /// Cached verdicts keyed by sha256.
    #[serde(default)]
    pub vt_cache: HashMap<String, VtVerdict>,
    /// Asset → sha256 mapping from completed downloads (VT badge lookup).
    #[serde(default)]
    pub asset_shas: HashMap<String, String>,
    #[serde(default)]
    pub stats: super::models::Stats,
}

impl AppState {
    /// Load from `path`. A missing file is a fresh state; a corrupt file is
    /// an error the UI shows as a banner (defaults are NOT silently used —
    /// the user must know their repo list failed to load).
    pub fn load(path: &Path) -> Result<AppState, String> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(AppState::default())
            }
            Err(e) => return Err(format!("read state: {e}")),
        };
        serde_json::from_str(&text).map_err(|e| format!("parse state (file may be corrupt): {e}"))
    }

    /// Atomic save: write to `<path>.tmp`, then rename over the target.
    /// A crash mid-write leaves the old file intact.
    pub fn save_atomic(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("serialize state: {e}"))?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| format!("write temp state: {e}"))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("rename state: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{Asset, Release, RepoHost, RepoStatus};

    fn sample_state() -> AppState {
        let mut vt_cache = HashMap::new();
        vt_cache.insert(
            "deadbeef".to_string(),
            VtVerdict {
                sha256: "deadbeef".into(),
                malicious: 0,
                suspicious: 0,
                undetected: 70,
                harmless: 2,
                timeout: 0,
                scanned_by_upload: true,
                retrieved_at: "2026-11-05T00:00:00Z".into(),
            },
        );
        AppState {
            repos: vec![Repo {
                id: "github.com/rust-lang/rust".into(),
                host: RepoHost::GitHub,
                owner: "rust-lang".into(),
                name: "rust".into(),
                category: None,
                pinned: false,
                filters: Default::default(),
                gl_project_id: None,
                last_seen_tag: Some("1.81.0".into()),
                latest: Some(Release {
                    tag: "1.82.0".into(),
                    published_at: "2026-10-31T00:00:00Z".into(),
                    prerelease: false,
                    notes: String::new(),
                    assets: vec![Asset {
                        name: "rust.exe".into(),
                        size: 1,
                        url: "https://example.com/rust.exe".into(),
                    }],
                }),
                status: RepoStatus::Ok,
                new_release: true,
            }],
            vt_cache,
            asset_shas: Default::default(),
            stats: super::super::models::Stats {
                downloads_started: 3,
                downloads_completed: 2,
                vt_scans: 5,
                vt_flagged_seen: 1,
            },
        }
    }

    #[test]
    fn state_roundtrip_preserves_everything() {
        let st = sample_state();
        let dir = std::env::temp_dir().join("girust_state_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("repos.json");
        st.save_atomic(&path).unwrap();
        let back = AppState::load(&path).unwrap();
        assert_eq!(back, st);
    }

    #[test]
    fn save_atomic_leaves_no_tmp_file() {
        let dir = std::env::temp_dir().join("girust_state_tmp");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("repos.json");
        sample_state().save_atomic(&path).unwrap();
        let tmp = path.with_extension("json.tmp");
        assert!(!tmp.exists(), ".tmp file must be gone after rename");
        assert!(path.exists());
    }

    #[test]
    fn missing_state_file_yields_fresh_default() {
        let dir = std::env::temp_dir().join("girust_state_missing");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("repos.json");
        let _ = std::fs::remove_file(&path);
        let st = AppState::load(&path).unwrap();
        assert_eq!(st, AppState::default());
    }

    #[test]
    fn corrupt_state_file_is_error_not_default() {
        let dir = std::env::temp_dir().join("girust_state_corrupt");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("repos.json");
        std::fs::write(&path, "{{{").unwrap();
        let res = AppState::load(&path);
        assert!(res.is_err(), "corrupt file must Err so UI can show a banner");
        let msg = res.unwrap_err();
        assert!(msg.contains("corrupt"), "error should mention corruption: {msg}");
    }
}
