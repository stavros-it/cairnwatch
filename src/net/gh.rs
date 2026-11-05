//! GitHub releases API client.
//!
//! Pure logic (filter application, DTO→model mapping) is unit-tested here;
//! the reqwest paths are thin and exercised by Task 10's smoke tests.

use crate::core::models::{Asset, Release, ReleaseFilters};
use serde::Deserialize;

pub const GH_API: &str = "https://api.github.com";

#[derive(Debug)]
pub enum FetchError {
    /// 404 — caller decides NoReleases vs NoStableRelease.
    NotFound,
    Auth,
    RateLimited,
    Network(String),
    Parse(String),
}

#[derive(Debug, Deserialize)]
pub struct GhAsset {
    pub name: String,
    pub size: u64,
    pub browser_download_url: String,
}

#[derive(Debug, Deserialize)]
pub struct GhRelease {
    pub tag_name: String,
    pub published_at: Option<String>,
    pub prerelease: bool,
    pub body: Option<String>,
    pub assets: Vec<GhAsset>,
}

impl GhRelease {
    pub fn to_model(&self) -> Release {
        Release {
            tag: self.tag_name.clone(),
            published_at: self.published_at.clone().unwrap_or_default(),
            prerelease: self.prerelease,
            notes: self.body.clone().unwrap_or_default(),
            assets: self
                .assets
                .iter()
                .map(|a| Asset {
                    name: a.name.clone(),
                    size: a.size,
                    url: a.browser_download_url.clone(),
                })
                .collect(),
        }
    }
}

/// Apply the repo's filters to a fixed list of releases and pick the
/// "latest": newest by published_at among those passing the filters.
/// `Ok(None)` when nothing matches.
///
/// Pure — the unit tests' target.
pub fn pick_latest(releases: &[Release], filters: &ReleaseFilters) -> Option<Release> {
    let tag_re = filters
        .tag_regex
        .as_deref()
        .and_then(|pat| regex::Regex::new(pat).ok());
    let mut candidates: Vec<&Release> = releases
        .iter()
        .filter(|r| filters.include_prereleases || !r.prerelease)
        .filter(|r| match &tag_re {
            Some(re) => re.is_match(&r.tag),
            None => true,
        })
        .collect();
    // Sort newest first; empty published_at sorts oldest.
    candidates.sort_by(|a, b| b.published_at.cmp(&a.published_at));
    candidates.first().map(|r| (*r).clone())
}

/// Fetch the latest stable release (`releases/latest`).
pub async fn fetch_latest(
    client: &reqwest::Client,
    owner: &str,
    name: &str,
    pat: Option<&str>,
) -> Result<Release, FetchError> {
    let url = format!("{GH_API}/repos/{owner}/{name}/releases/latest");
    let mut req = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "GiRustStatus");
    if let Some(pat) = pat {
        req = req.header("Authorization", format!("Bearer {pat}"));
    }
    let resp = req.send().await.map_err(|e| FetchError::Network(e.to_string()))?;
    match resp.status() {
        reqwest::StatusCode::OK => {}
        reqwest::StatusCode::NOT_FOUND => return Err(FetchError::NotFound),
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
            return Err(FetchError::Auth)
        }
        reqwest::StatusCode::TOO_MANY_REQUESTS => return Err(FetchError::RateLimited),
        s => return Err(FetchError::Network(format!("github status {s}"))),
    }
    let gh: GhRelease = resp
        .json()
        .await
        .map_err(|e| FetchError::Parse(e.to_string()))?;
    Ok(gh.to_model())
}

/// Fetch the releases list and apply filters (prerelease include, tag regex).
pub async fn fetch_releases_list(
    client: &reqwest::Client,
    owner: &str,
    name: &str,
    pat: Option<&str>,
    filters: &ReleaseFilters,
) -> Result<Option<Release>, FetchError> {
    let url = format!("{GH_API}/repos/{owner}/{name}/releases?per_page=100");
    let mut req = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "GiRustStatus");
    if let Some(pat) = pat {
        req = req.header("Authorization", format!("Bearer {pat}"));
    }
    let resp = req.send().await.map_err(|e| FetchError::Network(e.to_string()))?;
    match resp.status() {
        reqwest::StatusCode::OK => {}
        reqwest::StatusCode::NOT_FOUND => return Err(FetchError::NotFound),
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
            return Err(FetchError::Auth)
        }
        reqwest::StatusCode::TOO_MANY_REQUESTS => return Err(FetchError::RateLimited),
        s => return Err(FetchError::Network(format!("github status {s}"))),
    }
    let list: Vec<GhRelease> = resp
        .json()
        .await
        .map_err(|e| FetchError::Parse(e.to_string()))?;
    let models: Vec<Release> = list.iter().map(|r| r.to_model()).collect();
    Ok(pick_latest(&models, filters))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rel(tag: &str, at: &str, pre: bool) -> Release {
        Release {
            tag: tag.into(),
            published_at: at.into(),
            prerelease: pre,
            notes: String::new(),
            assets: vec![],
        }
    }

    fn sample() -> Vec<Release> {
        vec![
            rel("v2.0.0", "2026-11-01T00:00:00Z", false),
            rel("v2.1.0-rc1", "2026-11-02T00:00:00Z", true),
            rel("v1.9.0", "2026-10-01T00:00:00Z", false),
        ]
    }

    #[test]
    fn default_filters_pick_newest_stable() {
        let got = pick_latest(&sample(), &ReleaseFilters::default()).unwrap();
        assert_eq!(got.tag, "v2.0.0");
    }

    #[test]
    fn include_prereleases_picks_newest_prerelease() {
        let filters = ReleaseFilters {
            include_prereleases: true,
            ..Default::default()
        };
        let got = pick_latest(&sample(), &filters).unwrap();
        assert_eq!(got.tag, "v2.1.0-rc1");
    }

    #[test]
    fn tag_regex_masks_nonmatching() {
        let filters = ReleaseFilters {
            tag_regex: Some(r"^v1\.".into()),
            ..Default::default()
        };
        let got = pick_latest(&sample(), &filters).unwrap();
        assert_eq!(got.tag, "v1.9.0");
    }

    #[test]
    fn no_match_yields_none() {
        let filters = ReleaseFilters {
            tag_regex: Some(r"^nonexistent$".into()),
            ..Default::default()
        };
        assert!(pick_latest(&sample(), &filters).is_none());
    }

    #[test]
    fn prerelease_only_list_defaults_to_none() {
        let all_pre = vec![rel("v3.0.0-rc1", "2026-11-03T00:00:00Z", true)];
        assert!(pick_latest(&all_pre, &ReleaseFilters::default()).is_none());
    }

    #[test]
    fn gh_release_dto_maps_to_model() {
        let gh = GhRelease {
            tag_name: "v1.0.0".into(),
            published_at: Some("2026-01-01T00:00:00Z".into()),
            prerelease: false,
            body: Some("notes".into()),
            assets: vec![GhAsset {
                name: "app.zip".into(),
                size: 10,
                browser_download_url: "https://x/app.zip".into(),
            }],
        };
        let m = gh.to_model();
        assert_eq!(m.tag, "v1.0.0");
        assert_eq!(m.assets.len(), 1);
        assert_eq!(m.assets[0].name, "app.zip");
        assert_eq!(m.assets[0].size, 10);
    }
}
