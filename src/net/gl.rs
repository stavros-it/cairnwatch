//! GitLab releases API client (gitlab.com and self-hosted instances).

use crate::core::models::{Asset, Release};
use serde::Deserialize;

pub const GL_WEB: &str = "https://gitlab.com";

#[derive(Debug)]
pub enum GlError {
    NotFound,
    Auth,
    RateLimited,
    Network(String),
    Parse(String),
}

#[derive(Debug, Deserialize)]
pub struct GlAssetLink {
    pub name: Option<String>,
    pub url: Option<String>,
    pub direct_asset_url: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct GlAssets {
    #[serde(default)]
    pub links: Vec<GlAssetLink>,
    #[serde(default)]
    pub sources: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct GlRelease {
    pub tag_name: String,
    pub released_at: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub assets: GlAssets,
}

impl GlRelease {
    pub fn to_model(&self) -> Release {
        let mut assets: Vec<Asset> = self
            .assets
            .links
            .iter()
            .filter_map(|l| {
                let name = l.name.clone()?;
                let url = l.direct_asset_url.clone().or_else(|| l.url.clone())?;
                Some(Asset { name, size: 0, url })
            })
            .collect();
        // Generic source archives fill the list when no curated links exist.
        if assets.is_empty() {
            for src in &self.assets.sources {
                if let Some(url) = src.get("url").and_then(|v| v.as_str()) {
                    let name = url.rsplit('/').next().unwrap_or("source").to_string();
                    assets.push(Asset {
                        name,
                        size: 0,
                        url: url.to_string(),
                    });
                }
            }
        }
        Release {
            tag: self.tag_name.clone(),
            published_at: self.released_at.clone().unwrap_or_default(),
            prerelease: false, // GitLab has no prerelease flag
            notes: self.description.clone().unwrap_or_default(),
            assets,
        }
    }
}

fn api_base(host: &str) -> String {
    if host == "gitlab.com" {
        format!("{GL_WEB}/api/v4")
    } else {
        format!("https://{host}/api/v4")
    }
}

/// Resolve `owner/name` (full group path) → numeric project id.
pub async fn resolve_project_id(
    client: &reqwest::Client,
    host: &str,
    owner_slash_name: &str,
    pat: Option<&str>,
) -> Result<u64, GlError> {
    let encoded = urlencoding_simple(owner_slash_name);
    let url = format!("{}/projects/{}", api_base(host), encoded);
    let mut req = client.get(&url).header("User-Agent", "GiRustStatus");
    if let Some(pat) = pat {
        req = req.header("PRIVATE-TOKEN", pat);
    }
    let resp = req.send().await.map_err(|e| GlError::Network(e.to_string()))?;
    match resp.status() {
        reqwest::StatusCode::OK => {}
        reqwest::StatusCode::NOT_FOUND => return Err(GlError::NotFound),
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
            return Err(GlError::Auth)
        }
        reqwest::StatusCode::TOO_MANY_REQUESTS => return Err(GlError::RateLimited),
        s => return Err(GlError::Network(format!("gitlab status {s}"))),
    }
    #[derive(Deserialize)]
    struct Proj {
        id: u64,
    }
    let p: Proj = resp
        .json()
        .await
        .map_err(|e| GlError::Parse(e.to_string()))?;
    Ok(p.id)
}

/// Percent-encode a project path (`grp/sub/proj` → `grp%2Fsub%2Fproj`).
/// Only `/` needs encoding for a path; GitLab requires it single-encoded.
fn urlencoding_simple(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'/' => "%2F".to_string(),
            c if c.is_ascii_alphanumeric()
                || matches!(c, b'-' | b'_' | b'.' | b'~') =>
            {
                (c as char).to_string()
            }
            c => format!("%{c:02X}"),
        })
        .collect()
}

/// Fetch the newest release (GitLab `/releases` returns newest first).
pub async fn fetch_latest(
    client: &reqwest::Client,
    host: &str,
    project_id: u64,
    pat: Option<&str>,
) -> Result<Option<Release>, GlError> {
    let url = format!("{}/projects/{}/releases?per_page=1", api_base(host), project_id);
    let mut req = client.get(&url).header("User-Agent", "GiRustStatus");
    if let Some(pat) = pat {
        req = req.header("PRIVATE-TOKEN", pat);
    }
    let resp = req.send().await.map_err(|e| GlError::Network(e.to_string()))?;
    match resp.status() {
        reqwest::StatusCode::OK => {}
        reqwest::StatusCode::NOT_FOUND => return Err(GlError::NotFound),
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
            return Err(GlError::Auth)
        }
        reqwest::StatusCode::TOO_MANY_REQUESTS => return Err(GlError::RateLimited),
        s => return Err(GlError::Network(format!("gitlab status {s}"))),
    }
    let list: Vec<GlRelease> = resp
        .json()
        .await
        .map_err(|e| GlError::Parse(e.to_string()))?;
    Ok(list.first().map(|r| r.to_model()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_path_encoding() {
        assert_eq!(
            urlencoding_simple("grp/sub/proj"),
            "grp%2Fsub%2Fproj"
        );
        assert_eq!(urlencoding_simple("acme/widget"), "acme%2Fwidget");
    }

    #[test]
    fn gl_release_maps_links_with_direct_url_preference() {
        let json = serde_json::json!({
            "tag_name": "v17.6.1",
            "released_at": "2026-10-29T00:00:00Z",
            "description": "rel notes",
            "assets": {
                "links": [
                    {
                        "name": "runner.exe",
                        "url": "https://x/runner.exe",
                        "direct_asset_url": "https://y/runner.exe"
                    }
                ],
                "sources": []
            }
        });
        let gl: GlRelease = serde_json::from_value(json).unwrap();
        let m = gl.to_model();
        assert_eq!(m.tag, "v17.6.1");
        assert_eq!(m.assets.len(), 1);
        assert_eq!(m.assets[0].url, "https://y/runner.exe"); // direct preferred
        assert_eq!(m.assets[0].name, "runner.exe");
    }

    #[test]
    fn gl_release_falls_back_to_sources_when_no_links() {
        let json = serde_json::json!({
            "tag_name": "v1.0.0",
            "released_at": "2026-01-01T00:00:00Z",
            "assets": {
                "links": [],
                "sources": [
                    {"url": "https://x/proj-v1.0.0.tar.gz"}
                ]
            }
        });
        let gl: GlRelease = serde_json::from_value(json).unwrap();
        let m = gl.to_model();
        assert_eq!(m.assets.len(), 1);
        assert_eq!(m.assets[0].name, "proj-v1.0.0.tar.gz");
    }

    #[test]
    fn gl_release_empty_assets_is_empty_vec() {
        let json = serde_json::json!({"tag_name": "v1.0.0"});
        let gl: GlRelease = serde_json::from_value(json).unwrap();
        let m = gl.to_model();
        assert!(m.assets.is_empty());
        assert!(!m.prerelease); // GitLab releases are never flagged prerelease
    }
}
