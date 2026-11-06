//! Secrets (VirusTotal key, GitHub/GitLab tokens) in the OS keyring —
//! Windows Credential Manager — never in `config.toml`.
//!
//! User decision 2026-11-06 ("keyring only"): this is the one deliberate
//! exception to "no writes outside the portable folder". Consequences:
//! - Secrets do not travel with the folder. Moving PCs = re-enter them,
//!   or use the *encrypted* export, which carries them.
//! - Portable copies on the same Windows account share one set of
//!   secrets (same service name).
//! - Non-Windows builds have no native backend enabled (keyring's mock
//!   store): secrets last for the session only. Windows-first by design.
//!
//! `Config` keeps the secret fields for runtime use but never serializes
//! them (`#[serde(skip_serializing)]`); they are filled from here at start.

use crate::core::config::Config;

/// Credential Manager "service". Entries appear as `<account>.GiRustStatus`.
pub const SERVICE: &str = "GiRustStatus";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Secret {
    VtApiKey,
    GithubPat,
    GitlabPat,
}

impl Secret {
    pub const ALL: [Secret; 3] = [Secret::VtApiKey, Secret::GithubPat, Secret::GitlabPat];

    pub fn account(self) -> &'static str {
        match self {
            Secret::VtApiKey => "vt_api_key",
            Secret::GithubPat => "github_pat",
            Secret::GitlabPat => "gitlab_pat",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Secret::VtApiKey => "VirusTotal API key",
            Secret::GithubPat => "GitHub token",
            Secret::GitlabPat => "GitLab token",
        }
    }

    /// Current value in `cfg` ("" = unset).
    pub fn get(self, cfg: &Config) -> &str {
        match self {
            Secret::VtApiKey => &cfg.vt_api_key,
            Secret::GithubPat => cfg.github_pat.as_deref().unwrap_or(""),
            Secret::GitlabPat => cfg.gitlab_pat.as_deref().unwrap_or(""),
        }
    }

    pub fn put(self, cfg: &mut Config, value: &str) {
        let opt = (!value.is_empty()).then(|| value.to_string());
        match self {
            Secret::VtApiKey => cfg.vt_api_key = value.to_string(),
            Secret::GithubPat => cfg.github_pat = opt,
            Secret::GitlabPat => cfg.gitlab_pat = opt,
        }
    }
}

/// Backend abstraction so the logic is testable without touching the real
/// Credential Manager.
pub trait SecretStore {
    /// `Ok(None)` = no entry.
    fn get(&self, s: Secret) -> Result<Option<String>, String>;
    /// `""` deletes the entry (deleting a missing entry is not an error).
    fn set(&self, s: Secret, value: &str) -> Result<(), String>;
}

/// The real store: Windows Credential Manager via the `keyring` crate.
pub struct OsKeyring {
    pub service: &'static str,
}

impl Default for OsKeyring {
    fn default() -> Self {
        Self { service: SERVICE }
    }
}

impl SecretStore for OsKeyring {
    fn get(&self, s: Secret) -> Result<Option<String>, String> {
        let entry = keyring::Entry::new(self.service, s.account())
            .map_err(|e| format!("{}: {e}", s.label()))?;
        match entry.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(format!("{}: {e}", s.label())),
        }
    }

    fn set(&self, s: Secret, value: &str) -> Result<(), String> {
        let entry = keyring::Entry::new(self.service, s.account())
            .map_err(|e| format!("{}: {e}", s.label()))?;
        let res = if value.is_empty() {
            match entry.delete_credential() {
                Err(keyring::Error::NoEntry) => Ok(()),
                other => other,
            }
        } else {
            entry.set_password(value)
        };
        res.map_err(|e| format!("{}: {e}", s.label()))
    }
}

/// What happened while loading secrets at startup.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct LoadReport {
    /// Secrets found in plaintext `config.toml` and moved into the store.
    pub migrated: Vec<Secret>,
    /// A plaintext secret could NOT be moved into the store.
    pub migration_failed: bool,
    pub errors: Vec<String>,
}

impl LoadReport {
    /// Rewrite `config.toml` (which drops the plaintext) only when every
    /// plaintext secret is safely in the store — otherwise a key is lost.
    pub fn should_rewrite_config(&self) -> bool {
        !self.migrated.is_empty() && !self.migration_failed
    }
}

/// Fill `cfg`'s secrets from the store. Plaintext values that came from an
/// older `config.toml` are migrated into the store first. A value that
/// fails to migrate stays in `cfg` (in memory) and is NOT reported as
/// migrated, so the caller keeps the plaintext file instead of losing it.
pub fn load_into(cfg: &mut Config, store: &dyn SecretStore) -> LoadReport {
    let mut report = LoadReport::default();
    for s in Secret::ALL {
        let plaintext = s.get(cfg).to_string();
        if !plaintext.is_empty() {
            match store.set(s, &plaintext) {
                Ok(()) => report.migrated.push(s),
                Err(e) => {
                    report.migration_failed = true;
                    report.errors.push(e);
                }
            }
            continue;
        }
        match store.get(s) {
            Ok(Some(v)) => s.put(cfg, &v),
            Ok(None) => {}
            Err(e) => report.errors.push(e),
        }
    }
    report
}

/// Persist every secret that differs between `old` and `new`.
pub fn sync(old: &Config, new: &Config, store: &dyn SecretStore) -> Result<(), String> {
    let mut errors = Vec::new();
    for s in Secret::ALL {
        if s.get(old) != s.get(new) {
            if let Err(e) = store.set(s, s.get(new)) {
                errors.push(e);
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

/// Import rule: a bundle without a secret (plain export, or one made
/// before secrets existed) must not wipe the secret already stored.
pub fn keep_missing(imported: &mut Config, current: &Config) {
    for s in Secret::ALL {
        if s.get(imported).is_empty() && !s.get(current).is_empty() {
            s.put(imported, s.get(current));
        }
    }
}

/// In-memory store: unit tests, and debug devshots runs (which must never
/// read or write the user's real Credential Manager entries). Compiled out
/// of release builds.
#[cfg(any(test, debug_assertions))]
#[derive(Default)]
pub struct MemStore {
    pub map: std::cell::RefCell<std::collections::HashMap<Secret, String>>,
    pub fail: bool,
}

#[cfg(any(test, debug_assertions))]
impl SecretStore for MemStore {
    fn get(&self, s: Secret) -> Result<Option<String>, String> {
        if self.fail {
            return Err("store unavailable".into());
        }
        Ok(self.map.borrow().get(&s).cloned())
    }

    fn set(&self, s: Secret, value: &str) -> Result<(), String> {
        if self.fail {
            return Err("store unavailable".into());
        }
        if value.is_empty() {
            self.map.borrow_mut().remove(&s);
        } else {
            self.map.borrow_mut().insert(s, value.to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg_with(vt: &str, gh: Option<&str>) -> Config {
        Config {
            vt_api_key: vt.into(),
            github_pat: gh.map(Into::into),
            ..Config::default()
        }
    }

    #[test]
    fn load_fills_from_store() {
        let store = MemStore::default();
        store.set(Secret::VtApiKey, "vt-1").unwrap();
        store.set(Secret::GitlabPat, "gl-1").unwrap();
        let mut cfg = Config::default();
        let r = load_into(&mut cfg, &store);
        assert_eq!(r, LoadReport::default());
        assert_eq!(cfg.vt_api_key, "vt-1");
        assert_eq!(cfg.gitlab_pat.as_deref(), Some("gl-1"));
        assert_eq!(cfg.github_pat, None);
    }

    #[test]
    fn plaintext_config_migrates_into_store() {
        let store = MemStore::default();
        store.set(Secret::VtApiKey, "stale").unwrap();
        let mut cfg = cfg_with("from-toml", Some("ghp"));
        let r = load_into(&mut cfg, &store);
        assert_eq!(r.migrated, vec![Secret::VtApiKey, Secret::GithubPat]);
        assert!(r.should_rewrite_config());
        // Plaintext wins over a stale stored value (it's what the user had).
        assert_eq!(store.get(Secret::VtApiKey).unwrap().as_deref(), Some("from-toml"));
        assert_eq!(store.get(Secret::GithubPat).unwrap().as_deref(), Some("ghp"));
        assert_eq!(cfg.vt_api_key, "from-toml");
    }

    #[test]
    fn failed_migration_is_not_reported_as_migrated() {
        // Caller rewrites config.toml only for migrated secrets; a failure
        // must keep the plaintext file so the key is not lost.
        let store = MemStore { fail: true, ..Default::default() };
        let mut cfg = cfg_with("k", None);
        let r = load_into(&mut cfg, &store);
        assert!(r.migrated.is_empty());
        assert!(r.migration_failed);
        assert!(!r.should_rewrite_config());
        assert_eq!(r.errors.len(), 3); // 1 failed migration + 2 failed reads
        assert_eq!(cfg.vt_api_key, "k"); // still usable this session
    }

    #[test]
    fn sync_writes_only_changes_and_empty_deletes() {
        let store = MemStore::default();
        store.set(Secret::GithubPat, "old").unwrap();
        let old = cfg_with("", Some("old"));
        let new = cfg_with("vt-new", None);
        sync(&old, &new, &store).unwrap();
        assert_eq!(store.get(Secret::VtApiKey).unwrap().as_deref(), Some("vt-new"));
        assert_eq!(store.get(Secret::GithubPat).unwrap(), None);
    }

    #[test]
    fn keep_missing_preserves_current_secrets_on_import() {
        let current = cfg_with("mine", Some("gh-mine"));
        let mut imported = cfg_with("", Some("gh-imported"));
        keep_missing(&mut imported, &current);
        assert_eq!(imported.vt_api_key, "mine");
        assert_eq!(imported.github_pat.as_deref(), Some("gh-imported"));
    }

    /// Real Credential Manager round-trip on a throwaway service name
    /// (never the app's own entries). Proves the native backend is
    /// compiled in: keyring's mock store would lose the value between
    /// `Entry` instances.
    #[cfg(windows)]
    #[test]
    fn windows_credential_manager_roundtrip() {
        let store = OsKeyring { service: "GiRustStatus-selftest" };
        let s = Secret::VtApiKey;
        store.set(s, "").unwrap(); // clean slate
        assert_eq!(store.get(s).unwrap(), None);
        store.set(s, "selftest-value").unwrap();
        assert_eq!(store.get(s).unwrap().as_deref(), Some("selftest-value"));
        store.set(s, "").unwrap(); // delete
        assert_eq!(store.get(s).unwrap(), None);
        store.set(s, "").unwrap(); // deleting a missing entry is fine
    }

    #[test]
    fn config_toml_never_contains_secrets() {
        let cfg = cfg_with("SECRET-VT", Some("SECRET-GH"));
        let text = toml::to_string_pretty(&cfg).unwrap();
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("vt_api_key"));
    }

    #[test]
    fn old_plaintext_config_still_parses_for_migration() {
        let cfg: Config = toml::from_str("vt_api_key = \"k\"\ngithub_pat = \"g\"\n").unwrap();
        assert_eq!(cfg.vt_api_key, "k");
        assert_eq!(cfg.github_pat.as_deref(), Some("g"));
    }
}
