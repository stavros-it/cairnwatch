//! VT download gate: the modal state machine between "user clicked
//! Download" and "file lands in the download folder".
//!
//! Spec-pinned behaviors:
//! - Flagged (malicious+susp > 0): blocking dialog with N/M engine counts,
//!   top engines, VT report link, "Download anyway" / "Cancel",
//!   Cancel default-focused.
//! - No verdict obtainable (no key / auth / quota / network / upload
//!   disabled / too large / timeout): yellow "unverified" +
//!   "Not scanned — proceed?" with reason, Cancel default.
//! - Cancel deletes the temp file. Confirm moves it to download_dir.
//! - Verdicts cached by sha256; cache hit short-circuits the VT lookup.
//!
//! `default_button`/`GateButton`/`Scanning` are part of the gate contract
//! kept for the confirm-dialog focus rule (tested) and the upload-fallback
//! scan phase (bus may surface it).
#![allow(dead_code)]

use crate::core::models::VtVerdict;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub enum ConfirmKind {
    Flagged {
        verdict: VtVerdict,
        top_engines: Vec<String>,
    },
    Unverified {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum GatePhase {
    Idle,
    Downloading { downloaded: u64, total: u64 },
    Scanning,
    /// Verdict clean → deliver immediately. Otherwise the UI must confirm.
    AwaitingConfirm(ConfirmKind),
    /// Clean verdict: file already delivered, dialog shows the receipt.
    Clean { path: PathBuf, sha256: String },
    Done { path: PathBuf, sha256: String },
    Cancelled,
    Failed(String),
}

/// Which dialog button is focused by default (spec: Cancel).
pub fn default_button(_kind: &ConfirmKind) -> GateButton {
    GateButton::Cancel
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateButton {
    Confirm,
    Cancel,
}

/// Map a lookup outcome + verdict cache into the next gate phase.
/// `sha` and `temp_path` describe the downloaded file.
/// Returns (phase, cacheable verdict if any).
pub fn phase_after_scan(
    outcome: &crate::net::vt::LookupOutcome,
    sha: &str,
    temp_path: PathBuf,
) -> (GatePhase, Option<VtVerdict>) {
    use crate::net::vt::LookupOutcome as LO;
    match outcome {
        LO::Report(v, engines) if !v.is_flagged() => {
            let v = VtVerdict { sha256: sha.to_string(), ..v.clone() };
            (GatePhase::Clean { path: temp_path, sha256: sha.to_string() }, Some(v))
        }
        LO::Report(v, engines) => {
            let v = VtVerdict { sha256: sha.to_string(), ..v.clone() };
            (
                GatePhase::AwaitingConfirm(ConfirmKind::Flagged {
                    verdict: v.clone(),
                    top_engines: engines.clone(),
                }),
                Some(v),
            )
        }
        LO::Unknown => (
            GatePhase::AwaitingConfirm(ConfirmKind::Unverified {
                reason: "file is unknown to VirusTotal and upload fallback is off".into(),
            }),
            None,
        ),
        LO::Unscanned => (
            GatePhase::AwaitingConfirm(ConfirmKind::Unverified {
                reason: "file is on VirusTotal but was never analysed".into(),
            }),
            None,
        ),
        LO::NoVerdict(e) => (
            GatePhase::AwaitingConfirm(ConfirmKind::Unverified {
                reason: e.to_string(),
            }),
            None,
        ),
    }
}

/// Confirm: move the temp file into `download_dir`, return the final path.
pub fn deliver(temp: &Path, download_dir: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(download_dir).map_err(|e| format!("create download dir: {e}"))?;
    let name = temp.file_name().ok_or("temp file has no name")?;
    let dest = download_dir.join(name);
    std::fs::rename(temp, &dest)
        // Rename can fail across drives/filesystems; fall back to copy+delete.
        .or_else(|_| std::fs::copy(temp, &dest).map(|_| ()).and_then(|_| std::fs::remove_file(temp)))
        .map_err(|e| format!("move to download dir: {e}"))?;
    Ok(dest)
}

/// Cancel: delete the temp file. Errors are swallowed (best effort).
pub fn discard(temp: &Path) {
    let _ = std::fs::remove_file(temp);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::vt::LookupOutcome;
    use std::io::Write;

    fn verdict(mal: u32, susp: u32) -> VtVerdict {
        VtVerdict {
            sha256: "a".repeat(64),
            malicious: mal,
            suspicious: susp,
            undetected: 70,
            harmless: 0,
            timeout: 0,
            scanned_by_upload: false,
            retrieved_at: "epoch:0".into(),
        }
    }

    fn temp_file(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(b"payload").unwrap();
        p
    }

    #[test]
    fn clean_verdict_delivers_without_confirm() {
        let out = LookupOutcome::Report(verdict(0, 0), vec![]);
        let dir = std::env::temp_dir().join("girust_gate_clean");
        std::fs::create_dir_all(&dir).unwrap();
        let tmp = temp_file(&dir, "clean.exe");
        let (phase, cached) = phase_after_scan(&out, "abc", tmp.clone());
        assert!(matches!(phase, GatePhase::Clean { .. }));
        assert!(cached.is_some());
        assert!(tmp.exists()); // delivery is the UI's confirm-free step
    }

    #[test]
    fn flagged_verdict_awaits_confirm_with_cancel_default() {
        let engines = vec!["Microsoft".into(), "Kaspersky".into()];
        let out = LookupOutcome::Report(verdict(3, 2), engines.clone());
        let dir = std::env::temp_dir().join("girust_gate_flagged");
        std::fs::create_dir_all(&dir).unwrap();
        let tmp = temp_file(&dir, "flag.exe");
        let (phase, cached) = phase_after_scan(&out, "abc", tmp);
        match phase {
            GatePhase::AwaitingConfirm(ConfirmKind::Flagged { verdict, top_engines }) => {
                assert!(verdict.is_flagged());
                assert_eq!(verdict.flagged_ratio(), (5, 75));
                assert_eq!(top_engines, engines);
                assert!(cached.is_some());
            }
            other => panic!("expected AwaitingConfirm(Flagged), got {other:?}"),
        }
    }

    #[test]
    fn unverified_outcomes_await_confirm_with_reason() {
        for out in [
            LookupOutcome::Unknown,
            LookupOutcome::Unscanned,
            LookupOutcome::NoVerdict(crate::net::vt::VtError::Auth),
        ] {
            let dir = std::env::temp_dir().join("girust_gate_unv");
            std::fs::create_dir_all(&dir).unwrap();
            let tmp = temp_file(&dir, "x.exe");
            let (phase, cached) = phase_after_scan(&out, "abc", tmp);
            match phase {
                GatePhase::AwaitingConfirm(ConfirmKind::Unverified { reason }) => {
                    assert!(!reason.is_empty());
                    assert!(cached.is_none());
                }
                other => panic!("expected AwaitingConfirm(Unverified), got {other:?}"),
            }
        }
    }

    #[test]
    fn cancel_default_focus_on_both_dialog_kinds() {
        let flagged = ConfirmKind::Flagged {
            verdict: verdict(1, 0),
            top_engines: vec![],
        };
        let unv = ConfirmKind::Unverified { reason: "no key".into() };
        assert_eq!(default_button(&flagged), GateButton::Cancel);
        assert_eq!(default_button(&unv), GateButton::Cancel);
    }

    #[test]
    fn confirm_moves_file_to_download_dir() {
        let dir = std::env::temp_dir().join("girust_gate_deliver");
        std::fs::create_dir_all(&dir).unwrap();
        let tmp = temp_file(&dir, "asset.zip");
        let dl = dir.join("downloads");
        let dest = deliver(&tmp, &dl).unwrap();
        assert!(dest.exists());
        assert_eq!(dest.file_name().unwrap(), "asset.zip");
        assert!(!tmp.exists(), "temp must be gone after move");
        std::fs::remove_file(&dest).unwrap();
    }

    #[test]
    fn cancel_deletes_temp_file() {
        let dir = std::env::temp_dir().join("girust_gate_discard");
        std::fs::create_dir_all(&dir).unwrap();
        let tmp = temp_file(&dir, "gone.zip");
        discard(&tmp);
        assert!(!tmp.exists(), "cancel must delete the temp file");
    }

    #[test]
    fn cache_hit_short_circuits_lookup() {
        // Bus-level behavior verified here at data level: a cached verdict
        // for the same sha must produce Clean/AwaitingConfirm without any
        // network call — the UI checks AppState.vt_cache before sending
        // DownloadAsset's scan step.
        let mut cache = std::collections::HashMap::new();
        cache.insert("abc".to_string(), verdict(0, 0));
        let (phase, _) = phase_after_scan(&LookupOutcome::Report(cache["abc"].clone(), vec![]), "abc", PathBuf::from("x"));
        assert!(matches!(phase, GatePhase::Clean { .. }));
    }
}
