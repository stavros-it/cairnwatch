//! Streaming download engine: temp `.part` file, SHA-256 while streaming,
//! progress events, cancel flag.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Debug)]
pub enum DlError {
    Network(String),
    Io(String),
    Cancelled,
}

impl std::fmt::Display for DlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DlError::Network(e) => write!(f, "download network error: {e}"),
            DlError::Io(e) => write!(f, "download io error: {e}"),
            DlError::Cancelled => write!(f, "download cancelled"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Progress {
    pub downloaded: u64,
    pub total: u64,
}

/// Sanitize an asset name into a safe file stem: replace characters that
/// Windows forbids (`<>:"/\|?*` and control chars), collapse whitespace.
/// Pure — unit-tested.
pub fn sanitize_stem(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => out.push('_'),
            c if (c as u32) < 0x20 => out.push('_'),
            c => out.push(c),
        }
    }
    let trimmed = out.trim().trim_end_matches('.');
    if trimmed.is_empty() {
        "download".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Download `url` into `tmp_dir/<stem>.part`, hashing while streaming, then
/// rename to `<stem>` on completion. Returns the final path + sha256 hex.
/// `cancel` is polled between chunks.
pub async fn download_to_temp(
    client: &reqwest::Client,
    url: &str,
    tmp_dir: &Path,
    asset_name: &str,
    progress_tx: &tokio::sync::mpsc::Sender<Progress>,
    cancel: Arc<AtomicBool>,
) -> Result<(PathBuf, String), DlError> {
    tokio::fs::create_dir_all(tmp_dir)
        .await
        .map_err(|e| DlError::Io(e.to_string()))?;

    let resp = client
        .get(url)
        .header("User-Agent", "Cairnwatch")
        .send()
        .await
        .map_err(|e| DlError::Network(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(DlError::Network(format!("status {}", resp.status())));
    }
    let total = resp.content_length().unwrap_or(0);

    let stem = sanitize_stem(asset_name);
    let part_path = tmp_dir.join(format!("{stem}.part"));
    let final_path = tmp_dir.join(&stem);

    let mut file = tokio::fs::File::create(&part_path)
        .await
        .map_err(|e| DlError::Io(e.to_string()))?;

    let mut hasher = Sha256::new();
    let mut downloaded: u64 = 0;
    let mut stream = resp.bytes_stream();
    use futures::StreamExt;
    let mut last_report = 0u64;
    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            let _ = tokio::fs::remove_file(&part_path).await;
            return Err(DlError::Cancelled);
        }
        let chunk = chunk.map_err(|e| DlError::Network(e.to_string()))?;
        hasher.update(&chunk);
        tokio::io::AsyncWriteExt::write_all(&mut file, &chunk)
            .await
            .map_err(|e| DlError::Io(e.to_string()))?;
        downloaded += chunk.len() as u64;
        // Report at most every 256 KiB to keep the channel quiet.
        if downloaded - last_report >= 256 * 1024 || total == 0 {
            last_report = downloaded;
            let _ = progress_tx.try_send(Progress { downloaded, total });
        }
    }
    let _ = progress_tx.try_send(Progress { downloaded, total });

    tokio::io::AsyncWriteExt::flush(&mut file)
        .await
        .map_err(|e| DlError::Io(e.to_string()))?;
    drop(file);

    tokio::fs::rename(&part_path, &final_path)
        .await
        .map_err(|e| DlError::Io(e.to_string()))?;
    let sha = format!("{:x}", hasher.finalize());
    Ok((final_path, sha))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_replaces_windows_forbidden() {
        assert_eq!(sanitize_stem("app:v2<>.zip"), "app_v2__.zip");
        assert_eq!(sanitize_stem("a/b\\c|d?e*f"), "a_b_c_d_e_f");
        assert_eq!(sanitize_stem("normal-name.zip"), "normal-name.zip");
    }

    #[test]
    fn sanitize_handles_control_and_trailing_dots() {
        assert_eq!(sanitize_stem("bad\u{7}name"), "bad_name");
        assert_eq!(sanitize_stem("name..."), "name");
        assert_eq!(sanitize_stem("   "), "download");
        assert_eq!(sanitize_stem("..."), "download");
    }

    #[test]
    fn sanitize_keeps_unicode() {
        assert_eq!(sanitize_stem("ñame-β.zip"), "ñame-β.zip");
    }
}
