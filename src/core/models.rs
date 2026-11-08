//! Data models shared across the app.

use serde::{Deserialize, Serialize};

/// Which service a tracked repo lives on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepoHost {
    GitHub,
    /// `host` is the GitLab instance base host, e.g. `gitlab.com` or
    /// a self-hosted `gitlab.example.com`.
    GitLab { host: String },
}

/// Per-repo release filters, applied during fetch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ReleaseFilters {
    /// Include prereleases when picking the "latest" release.
    /// Default: false (GitHub `releases/latest` semantics).
    #[serde(default)]
    pub include_prereleases: bool,
    /// Only consider releases whose tag matches this regex.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag_regex: Option<String>,
    /// On the detail view, assets matching this glob sort first and get a
    /// "match" mark; others stay listed, de-emphasized.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_pattern: Option<String>,
}

/// One downloadable file attached to a release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    pub name: String,
    pub size: u64,
    pub url: String,
}

/// The latest release of a tracked repo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Release {
    pub tag: String,
    /// RFC3339 timestamp.
    pub published_at: String,
    pub prerelease: bool,
    /// Release notes body (markdown source, rendered as styled text).
    pub notes: String,
    pub assets: Vec<Asset>,
}

/// Per-repo fetch status, shown as a status pill.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepoStatus {
    #[default]
    Ok,
    /// Only prereleases exist; enable the prerelease filter to track them.
    NoStableRelease,
    NoReleases,
    Error(String),
    RateLimited,
}

/// VirusTotal verdict for one file hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VtVerdict {
    pub sha256: String,
    pub malicious: u32,
    pub suspicious: u32,
    pub undetected: u32,
    pub harmless: u32,
    pub timeout: u32,
    /// True when this app uploaded the file to get the verdict.
    pub scanned_by_upload: bool,
    /// RFC3339 timestamp of when the verdict was retrieved.
    pub retrieved_at: String,
}

impl VtVerdict {
    /// True when any engine flagged the file.
    pub fn is_flagged(&self) -> bool {
        self.malicious + self.suspicious > 0
    }

    /// "N/M engines flagged" — flagged count over total engines reporting.
    pub fn flagged_ratio(&self) -> (u32, u32) {
        let flagged = self.malicious + self.suspicious;
        let total = flagged + self.undetected + self.harmless + self.timeout;
        (flagged, total)
    }
}

/// A tracked repository with its last-known release state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repo {
    /// Stable id: `{host}/{owner}/{name}` (self-hosted keeps its host).
    pub id: String,
    pub host: RepoHost,
    pub owner: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub filters: ReleaseFilters,
    /// GitLab numeric project id, resolved at add time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gl_project_id: Option<u64>,
    /// Last release tag the user has seen (NEW pill clears against this).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen_tag: Option<String>,
    /// Release tag the user marked as installed locally. When set and
    /// different from `latest.tag`, the UI shows "update available".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed_tag: Option<String>,
    /// Latest release snapshot from the last successful fetch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest: Option<Release>,
    #[serde(default)]
    pub status: RepoStatus,
    /// True when `latest` is newer than `last_seen_tag`.
    #[serde(default)]
    pub new_release: bool,
}

impl Repo {
    /// `owner/name` for display.
    #[allow(dead_code)] // used by Task 6 UI
    pub fn display_name(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }

    /// True when the user marked an installed version and the latest
    /// release tag differs from it.
    pub fn update_available(&self) -> bool {
        match (&self.installed_tag, &self.latest) {
            (Some(inst), Some(rel)) => inst != &rel.tag,
            _ => false,
        }
    }

    /// True when the installed version equals the latest release.
    pub fn up_to_date(&self) -> bool {
        match (&self.installed_tag, &self.latest) {
            (Some(inst), Some(rel)) => inst == &rel.tag,
            _ => false,
        }
    }
}

/// Cumulative counters for the statistics view.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stats {
    pub downloads_started: u64,
    pub downloads_completed: u64,
    pub vt_scans: u64,
    pub vt_flagged_seen: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_filters_exclude_prereleases() {
        let f = ReleaseFilters::default();
        assert!(!f.include_prereleases);
        assert!(f.tag_regex.is_none());
        assert!(f.asset_pattern.is_none());
    }

    #[test]
    fn is_flagged_true_when_malicious() {
        let v = VtVerdict {
            sha256: "abc".into(),
            malicious: 3,
            suspicious: 0,
            undetected: 60,
            harmless: 8,
            timeout: 1,
            scanned_by_upload: false,
            retrieved_at: "2026-11-05T00:00:00Z".into(),
        };
        assert!(v.is_flagged());
        assert_eq!(v.flagged_ratio(), (3, 72));
    }

    #[test]
    fn is_flagged_false_when_clean() {
        let v = VtVerdict {
            sha256: "abc".into(),
            malicious: 0,
            suspicious: 0,
            undetected: 65,
            harmless: 7,
            timeout: 0,
            scanned_by_upload: false,
            retrieved_at: "2026-11-05T00:00:00Z".into(),
        };
        assert!(!v.is_flagged());
        assert_eq!(v.flagged_ratio(), (0, 72));
    }

    #[test]
    fn repo_serde_roundtrip_preserves_fields() {
        let repo = Repo {
            id: "gitlab.com/grp/sub/proj".into(),
            host: RepoHost::GitLab {
                host: "gitlab.com".into(),
            },
            owner: "grp/sub".into(),
            name: "proj".into(),
            category: Some("tools".into()),
            pinned: true,
            filters: ReleaseFilters {
                include_prereleases: true,
                tag_regex: Some("^v\\d+".into()),
                asset_pattern: Some("*-linux.zip".into()),
            },
            gl_project_id: Some(42),
            last_seen_tag: Some("v1.0.0".into()),
            installed_tag: None,
            latest: Some(Release {
                tag: "v1.1.0".into(),
                published_at: "2026-11-01T12:00:00Z".into(),
                prerelease: false,
                notes: "notes".into(),
                assets: vec![Asset {
                    name: "proj-linux.zip".into(),
                    size: 1234,
                    url: "https://example.com/x.zip".into(),
                }],
            }),
            status: RepoStatus::Ok,
            new_release: true,
        };
        let json = serde_json::to_string(&repo).unwrap();
        let back: Repo = serde_json::from_str(&json).unwrap();
        assert_eq!(back, repo);
    }

    #[test]
    fn update_available_compares_installed_to_latest() {
        let mut repo: Repo = serde_json::from_str(
            r#"{"id":"github.com/o/n","host":"GitHub","owner":"o","name":"n",
                "latest":{"tag":"v2","published_at":"","prerelease":false,"notes":"","assets":[]}}"#,
        )
        .unwrap();
        // Old repos.json without installed_tag loads fine; no update shown.
        assert!(repo.installed_tag.is_none());
        assert!(!repo.update_available());
        assert!(!repo.up_to_date());
        repo.installed_tag = Some("v1".into());
        assert!(repo.update_available());
        repo.installed_tag = Some("v2".into());
        assert!(!repo.update_available());
        assert!(repo.up_to_date());
    }
}
