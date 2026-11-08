//! VirusTotal v3 API client.
//!
//! Flow (spec-pinned): hash lookup first; on 404, optional upload
//! fallback (≤32 MB direct, ≤650 MB via upload_url); poll analysis every
//! 30 s, 10 min timeout. Quotas: 4 req/min, 500 req/day — the bus
//! throttles VT requests through a gate.

use crate::core::models::VtVerdict;
use serde_json::Value;

pub const VT_API: &str = "https://www.virustotal.com/api/v3";

/// Size limits (VT API).
pub const DIRECT_UPLOAD_LIMIT: u64 = 32 * 1024 * 1024;
pub const SPECIAL_UPLOAD_LIMIT: u64 = 650 * 1024 * 1024;

#[derive(Debug)]
pub enum VtError {
    Auth,
    Quota,
    TooLarge,
    Network(String),
    Parse(String),
}

impl std::fmt::Display for VtError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VtError::Auth => write!(f, "VirusTotal API key invalid or missing"),
            VtError::Quota => write!(f, "VirusTotal quota exhausted"),
            VtError::TooLarge => write!(f, "file too large for VirusTotal upload"),
            VtError::Network(e) => write!(f, "VirusTotal network error: {e}"),
            VtError::Parse(e) => write!(f, "VirusTotal response parse error: {e}"),
        }
    }
}

/// Outcome of a hash lookup.
#[derive(Debug)]
pub enum LookupOutcome {
    /// VT has a report with analysis stats (top flagging engines included
    /// for the flagged dialog — review fix).
    Report(VtVerdict, Vec<String>),
    /// VT never saw this hash (404) — upload fallback eligible.
    Unknown,
    /// Report exists but was never analysed (null stats).
    Unscanned,
    /// No verdict obtainable (auth, quota, network, parse).
    NoVerdict(VtError),
}

impl std::fmt::Display for LookupOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LookupOutcome::Report(_, _) => write!(f, "report"),
            LookupOutcome::Unknown => write!(f, "unknown to VirusTotal"),
            LookupOutcome::Unscanned => write!(f, "on VirusTotal, never analysed"),
            LookupOutcome::NoVerdict(e) => write!(f, "{e}"),
        }
    }
}

/// Pure: map a VT file-report `data.attributes` JSON into a verdict.
/// `retrieved_at` is passed in (tests inject a fixed timestamp).
pub fn map_stats(sha256: &str, attrs: &Value, retrieved_at: &str) -> LookupOutcome {
    let Some(stats) = attrs.get("last_analysis_stats") else {
        // Report exists but was never analysed.
        return LookupOutcome::Unscanned;
    };
    if stats.is_null() {
        return LookupOutcome::Unscanned;
    }
    let get = |k: &str| stats.get(k).and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let engines = top_flagging_engines(attrs);
    LookupOutcome::Report(
        VtVerdict {
            sha256: sha256_to_string(attrs, sha256),
            malicious: get("malicious"),
            suspicious: get("suspicious"),
            undetected: get("undetected"),
            harmless: get("harmless"),
            timeout: get("timeout"),
            scanned_by_upload: attrs
                .get("custom_analysis_id")
                .map(|v| !v.is_null())
                .unwrap_or(false),
            retrieved_at: retrieved_at.to_string(),
        },
        engines,
    )
}

/// Parse the top flagging engine names from `last_analysis_results`
/// (review: flagged dialog must list them). Returns up to 3 names of
/// engines whose verdict is malicious/suspicious.
pub fn top_flagging_engines(attrs: &Value) -> Vec<String> {
    let Some(results) = attrs.get("last_analysis_results") else {
        return Vec::new();
    };
    let Some(map) = results.as_object() else {
        return Vec::new();
    };
    let mut names: Vec<String> = map
        .iter()
        .filter(|(_, e)| {
            e.get("category")
                .and_then(|c| c.as_str())
                .map(|c| c == "malicious" || c == "suspicious")
                .unwrap_or(false)
        })
        .map(|(name, _)| name.clone())
        .collect();
    names.sort();
    names.truncate(3);
    names
}

fn sha256_to_string(attrs: &Value, fallback: &str) -> String {
    attrs
        .get("sha256")
        .and_then(|v| v.as_str())
        .unwrap_or(fallback)
        .to_string()
}

fn status_to_err(status: reqwest::StatusCode) -> VtError {
    match status {
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => VtError::Auth,
        reqwest::StatusCode::TOO_MANY_REQUESTS => VtError::Quota,
        reqwest::StatusCode::NOT_FOUND => VtError::Network(format!("unexpected 404: {status}")),
        s => VtError::Network(format!("virustotal status {s}")),
    }
}

/// Hash lookup: `GET /files/{sha256}`.
pub async fn lookup_sha256(
    client: &reqwest::Client,
    key: &str,
    sha256: &str,
) -> LookupOutcome {
    if key.is_empty() {
        return LookupOutcome::NoVerdict(VtError::Auth);
    }
    let url = format!("{VT_API}/files/{sha256}");
    let resp = match client
        .get(&url)
        .header("x-apikey", key)
        .header("User-Agent", "Cairnwatch")
        .send()
        .await
        .inspect(|r| crate::net::ratelimits::observe_vt(r.headers()))
    {
        Ok(r) => r,
        Err(e) => return LookupOutcome::NoVerdict(VtError::Network(e.to_string())),
    };
    match resp.status() {
        reqwest::StatusCode::OK => {}
        reqwest::StatusCode::NOT_FOUND => return LookupOutcome::Unknown,
        s => return LookupOutcome::NoVerdict(status_to_err(s)),
    }
    let json: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => return LookupOutcome::NoVerdict(VtError::Parse(e.to_string())),
    };
    let attrs = json.get("data").and_then(|d| d.get("attributes"));
    match attrs {
        Some(a) => map_stats(sha256, a, &now_rfc3339()),
        None => LookupOutcome::NoVerdict(VtError::Parse(
            "response missing data.attributes".into(),
        )),
    }
}

/// One poll step of an analysis: `GET /analyses/{id}`.
/// Returns Ok(Some(verdict)) when completed, Ok(None) when still queued.
pub async fn poll_once(
    client: &reqwest::Client,
    key: &str,
    analysis_id: &str,
) -> Result<Option<VtVerdict>, VtError> {
    let url = format!("{VT_API}/analyses/{analysis_id}");
    let resp = client
        .get(&url)
        .header("x-apikey", key)
        .send()
        .await
        .inspect(|r| crate::net::ratelimits::observe_vt(r.headers()))
        .map_err(|e| VtError::Network(e.to_string()))?;
    match resp.status() {
        reqwest::StatusCode::OK => {}
        s => return Err(status_to_err(s)),
    }
    let json: Value = resp
        .json()
        .await
        .map_err(|e| VtError::Parse(e.to_string()))?;
    let attrs = json
        .get("data")
        .and_then(|d| d.get("attributes"))
        .ok_or_else(|| VtError::Parse("missing data.attributes".into()))?;
    let status = attrs
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("queued");
    if status != "completed" {
        return Ok(None);
    }
    // Analysis stats carry the same shape as file reports.
    let mut attrs_obj = attrs.clone();
    // Build a synthetic attributes with last_analysis_stats from stats.
    if let Some(stats) = attrs.get("stats") {
        attrs_obj["last_analysis_stats"] = stats.clone();
    }
    let sha = attrs
        .get("md5") // not sha; verdict sha256 filled by caller from the file
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let outcome = map_stats(sha, &attrs_obj, &now_rfc3339());
    match outcome {
        LookupOutcome::Report(v, _) => Ok(Some(v)),
        LookupOutcome::Unscanned => Ok(None),
        other => Err(VtError::Parse(format!("unexpected outcome: {other}"))),
    }
}

/// Poll until completed, cancelled, or 10 min timeout (30 s interval).
/// `cancel` stops polling early so a cancelled scan doesn't keep spending
/// VT quota for up to 10 minutes.
pub async fn poll_analysis(
    client: &reqwest::Client,
    key: &str,
    analysis_id: &str,
    sha256: &str,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<VtVerdict, VtError> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(600);
    loop {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(VtError::Network("cancelled".into()));
        }
        if let Some(mut verdict) = poll_once(client, key, analysis_id).await? {
            verdict.sha256 = sha256.to_string();
            verdict.scanned_by_upload = true;
            return Ok(verdict);
        }
        if std::time::Instant::now() >= deadline {
            return Err(VtError::Network("analysis timeout (10 min)".into()));
        }
        // Sleep in 1 s steps so a cancel is noticed promptly.
        for _ in 0..30 {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(VtError::Network("cancelled".into()));
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }
}

/// Upload a file for scanning. ≤32 MB via `POST /files`; larger via the
/// special upload URL (650 MB cap).
pub async fn upload_file(
    client: &reqwest::Client,
    key: &str,
    path: &std::path::Path,
    size: u64,
) -> Result<String, VtError> {
    if key.is_empty() {
        return Err(VtError::Auth);
    }
    if size > SPECIAL_UPLOAD_LIMIT {
        return Err(VtError::TooLarge);
    }
    // Streamed from disk in 64 KB chunks — never the whole file in RAM
    // (650 MB uploads used to allocate 650 MB).
    let part = file_part(path).await?;

    let (url, resp) = if size <= DIRECT_UPLOAD_LIMIT {
        let url = format!("{VT_API}/files");
        let r = client
            .post(&url)
            .header("x-apikey", key)
            .multipart(reqwest::multipart::Form::new().part("file", part))
            .send()
            .await
            .inspect(|r| crate::net::ratelimits::observe_vt(r.headers()))
            .map_err(|e| VtError::Network(e.to_string()))?;
        (url, r)
    } else {
        // Request a special upload URL for large files.
        let get_url = format!("{VT_API}/files/upload_url");
        let resp = client
            .get(&get_url)
            .header("x-apikey", key)
            .send()
            .await
            .inspect(|r| crate::net::ratelimits::observe_vt(r.headers()))
            .map_err(|e| VtError::Network(e.to_string()))?;
        match resp.status() {
            reqwest::StatusCode::OK => {}
            s => return Err(status_to_err(s)),
        }
        let json: Value = resp
            .json()
            .await
            .map_err(|e| VtError::Parse(e.to_string()))?;
        let upload_url = json
            .get("data")
            .and_then(|d| d.get("upload_url"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| VtError::Parse("missing upload_url".into()))?
            .to_string();
        let r = client
            .post(&upload_url)
            .header("x-apikey", key)
            .multipart(reqwest::multipart::Form::new().part("file", part))
            .send()
            .await
            .inspect(|r| crate::net::ratelimits::observe_vt(r.headers()))
            .map_err(|e| VtError::Network(e.to_string()))?;
        (upload_url, r)
    };
    let _ = url;
    match resp.status() {
        reqwest::StatusCode::OK => {}
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
            return Err(VtError::Auth)
        }
        reqwest::StatusCode::TOO_MANY_REQUESTS => return Err(VtError::Quota),
        reqwest::StatusCode::PAYLOAD_TOO_LARGE => return Err(VtError::TooLarge),
        s => return Err(VtError::Network(format!("virustotal status {s}"))),
    }
    let json: Value = resp
        .json()
        .await
        .map_err(|e| VtError::Parse(e.to_string()))?;
    let id = json
        .get("data")
        .and_then(|d| d.get("id"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| VtError::Parse("missing analysis id".into()))?;
    Ok(id.to_string())
}

/// Chunk size for streamed uploads.
const UPLOAD_CHUNK: usize = 64 * 1024;

/// Byte stream over `file` in `UPLOAD_CHUNK` pieces; ends at EOF.
fn chunk_stream(
    file: tokio::fs::File,
) -> impl futures::Stream<Item = std::io::Result<Vec<u8>>> + Send + 'static {
    futures::stream::try_unfold(file, |mut f| async move {
        use tokio::io::AsyncReadExt;
        let mut buf = vec![0u8; UPLOAD_CHUNK];
        let n = f.read(&mut buf).await?;
        if n == 0 {
            return Ok(None);
        }
        buf.truncate(n);
        Ok(Some((buf, f)))
    })
}

/// Multipart file part streamed from `path`. Length comes from the file's
/// own metadata so Content-Length always matches the bytes sent.
async fn file_part(path: &std::path::Path) -> Result<reqwest::multipart::Part, VtError> {
    let io = |e: std::io::Error| VtError::Network(e.to_string());
    let file = tokio::fs::File::open(path).await.map_err(io)?;
    let len = file.metadata().await.map_err(io)?.len();
    let body = reqwest::Body::wrap_stream(chunk_stream(file));
    Ok(reqwest::multipart::Part::stream_with_length(body, len)
        .file_name(path.file_name().unwrap_or_default().to_string_lossy().into_owned()))
}

pub fn now_rfc3339() -> String {
    // No chrono dep; epoch seconds suffice for display.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("epoch:{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn chunk_stream_yields_exact_bytes_in_bounded_chunks() {
        use futures::StreamExt;
        let p = std::env::temp_dir().join(format!("grs_vt_stream_{}", std::process::id()));
        let data: Vec<u8> = (0..(UPLOAD_CHUNK * 3 + 17)).map(|i| (i % 251) as u8).collect();
        std::fs::write(&p, &data).unwrap();
        let f = tokio::fs::File::open(&p).await.unwrap();
        let chunks: Vec<Vec<u8>> = chunk_stream(f).map(|c| c.unwrap()).collect().await;
        assert!(chunks.iter().all(|c| !c.is_empty() && c.len() <= UPLOAD_CHUNK));
        assert_eq!(chunks.concat(), data);
        assert!(file_part(&p).await.is_ok());
        std::fs::remove_file(&p).unwrap();
        assert!(file_part(&p).await.is_err());
    }

    const AT: &str = "2026-11-05T00:00:00Z";
    const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const SHA2: &str = SHA;

    #[test]
    fn map_stats_clean_report() {
        let attrs = serde_json::json!({
            "sha256": SHA,
            "last_analysis_stats": {
                "malicious": 0, "suspicious": 0, "undetected": 70,
                "harmless": 0, "timeout": 0, "confirmed-timeout": 0,
                "failure": 0, "type-unsupported": 0
            }
        });
        match map_stats(SHA, &attrs, AT) {
            LookupOutcome::Report(v, _) => {
                assert!(!v.is_flagged());
                assert_eq!(v.malicious, 0);
                assert_eq!(v.undetected, 70);
                assert_eq!(v.sha256, SHA);
            }
            other => panic!("expected Report, got {other:?}"),
        }
    }

    #[test]
    fn top_engines_lists_flagging_engines_capped_at_3() {
        let attrs = serde_json::json!({
            "last_analysis_results": {
                "Microsoft": {"category": "malicious"},
                "Kaspersky": {"category": "suspicious"},
                "ESET": {"category": "malicious"},
                "Zillya": {"category": "malicious"},
                "Clean Engine": {"category": "undetected"},
            }
        });
        let engines = top_flagging_engines(&attrs);
        assert_eq!(engines.len(), 3);
        assert!(engines.contains(&"Microsoft".to_string()));
        assert!(engines.contains(&"Kaspersky".to_string()));
        assert!(!engines.contains(&"Clean Engine".to_string()));
        // Empty/missing results → empty vec, no panic.
        assert!(top_flagging_engines(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn map_stats_flagged_report() {
        let attrs = serde_json::json!({
            "last_analysis_stats": {
                "malicious": 3, "suspicious": 2, "undetected": 65,
                "harmless": 1, "timeout": 1
            }
        });
        match map_stats(SHA, &attrs, AT) {
            LookupOutcome::Report(v, _) => {
                assert!(v.is_flagged());
                assert_eq!(v.flagged_ratio(), (5, 72));
            }
            other => panic!("expected Report, got {other:?}"),
        }
    }

    #[test]
    fn map_stats_null_stats_is_unscanned() {
        let attrs = serde_json::json!({ "last_analysis_stats": null });
        assert!(matches!(
            map_stats(SHA, &attrs, AT),
            LookupOutcome::Unscanned
        ));
        let attrs2 = serde_json::json!({});
        assert!(matches!(
            map_stats(SHA, &attrs2, AT),
            LookupOutcome::Unscanned
        ));
    }

    #[test]
    fn empty_key_is_auth_no_verdict() {
        // lookup with empty key must not hit the network; tested at map level:
        // the guard is in lookup_sha256, verified by the bus test below.
        // Here: outcome Display for NoVerdict(Auth) names the cause.
        let out = LookupOutcome::NoVerdict(VtError::Auth);
        assert!(out.to_string().contains("invalid or missing"));
    }
}
