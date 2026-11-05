//! Command bus: UI sends Commands over a channel; a worker thread owning a
//! tokio runtime executes them and sends Events back. No background
//! polling — commands come only from user actions.

use super::download::{self, Progress};
use super::gh::{self, FetchError};
use super::gl::{self, GlError};
use super::ratelimits::RateLimits;
use super::vt::{self, LookupOutcome};
use crate::core::config::Config;
use crate::core::models::{Asset, Repo, RepoHost, RepoStatus};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const VT_THROTTLE_SECS: u64 = 16;

/// Commands the UI issues.
#[derive(Debug)]
pub enum Command {
    /// Refresh all repos (manual — Refresh button only).
    RefreshAll { repos: Vec<Repo>, config: Config },
    /// Add a repo from a pasted URL (verifies by fetching latest release).
    AddRepo { url: String, config: Config },
    /// Download an asset, then VT-scan it (the gate flow).
    DownloadAsset { repo_id: String, asset: Asset, config: Config },
    /// Re-lookup a cached sha256 (Re-scan button).
    RescanAsset { repo_id: String, asset_name: String, sha256: String, config: Config },
    /// Cancel the in-flight download (gate Cancel button).
    CancelDownload,
}

/// Events the worker sends back to the UI.
#[derive(Debug)]
pub enum Event {
    RepoUpdated(Repo),
    AddResult(Result<Repo, String>),
    DownloadProgress { repo_id: String, asset_name: String, downloaded: u64, total: u64 },
    /// Download finished + hashed; VT scan starting.
    ScanStarted { repo_id: String, asset_name: String },
    /// Download itself failed (network error).
    DownloadFailed { repo_id: String, reason: String },
    /// Verdict (or no-verdict reason) ready for a Re-scan.
    VtVerdictReady { repo_id: String, asset_name: String, sha256: String, outcome: LookupOutcome },
    /// File downloaded and hashed; verdict ready. UI gates delivery.
    FileReady { repo_id: String, asset_name: String, path: std::path::PathBuf, sha256: String, outcome: LookupOutcome },
    LogLine(String),
    RateLimitsUpdated(RateLimits),
    /// Refresh finished (all repos in the batch reported via RepoUpdated).
    RefreshDone,
}

/// VT throttle gate: max 1 request per VT_THROTTLE_SECS.
/// First request admits immediately; subsequent block until the interval
/// elapsed since the last admission. Clock-injectable for tests.
pub struct Throttle {
    next_allowed: Mutex<Instant>,
}

impl Throttle {
    pub fn new() -> Self {
        Self {
            next_allowed: Mutex::new(Instant::now() - Duration::from_secs(VT_THROTTLE_SECS)),
        }
    }

    /// Returns Some(instant when allowed) if blocked, None if admitted now.
    /// Admitting updates the gate.
    pub fn try_acquire(&self, now: Instant) -> Result<(), Duration> {
        let mut next = self.next_allowed.lock().unwrap();
        if now >= *next {
            *next = now + Duration::from_secs(VT_THROTTLE_SECS);
            Ok(())
        } else {
            Err(*next - now)
        }
    }
}

impl Default for Throttle {
    fn default() -> Self {
        Self::new()
    }
}

fn fetch_error_status(repo: &Repo, e: FetchError) -> Repo {
    let mut r = repo.clone();
    r.status = match e {
        FetchError::NotFound | FetchError::Parse(_) => RepoStatus::Error(format!("{e:?}")),
        FetchError::Auth => RepoStatus::Error("auth failed (bad PAT?)".into()),
        FetchError::RateLimited => RepoStatus::RateLimited,
        FetchError::Network(m) => RepoStatus::Error(format!("network: {m}")),
    };
    r
}

/// Forward rate-limit headers from any response to the UI.
fn report_limits(event_tx: &tokio::sync::mpsc::UnboundedSender<Event>, resp: &reqwest::Response) {
    let rl = RateLimits::from_headers(resp.headers());
    let _ = event_tx.send(Event::RateLimitsUpdated(rl));
}

fn gl_error_status(repo: &Repo, e: GlError) -> Repo {
    let mut r = repo.clone();
    r.status = match e {
        GlError::NotFound => RepoStatus::NoReleases,
        GlError::Auth => RepoStatus::Error("auth failed (bad token?)".into()),
        GlError::RateLimited => RepoStatus::RateLimited,
        GlError::Network(m) => RepoStatus::Error(format!("network: {m}")),
        GlError::Parse(m) => RepoStatus::Error(format!("parse: {m}")),
    };
    r
}

/// Human message for a failed add.
fn add_error(status: &RepoStatus) -> String {
    match status {
        RepoStatus::NoReleases => "repo has no releases of any kind".into(),
        RepoStatus::Error(m) => format!("repo unreachable: {m}"),
        RepoStatus::RateLimited => "API rate limited — try later or add a PAT in Settings".into(),
        RepoStatus::NoStableRelease | RepoStatus::Ok => "unexpected status".into(),
    }
}

/// Download + VT-scan flow, spawned per command so the bus loop stays free.
/// `cancel` is the shared flag; `throttle` is the shared VT gate.
async fn run_download_flow(
    client: reqwest::Client,
    throttle: std::sync::Arc<Throttle>,
    cancel: Arc<AtomicBool>,
    event_tx: tokio::sync::mpsc::UnboundedSender<Event>,
    repo_id: String,
    asset: crate::core::models::Asset,
    config: Config,
) {
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<Progress>(64);
    let cancel_dl = cancel.clone();
    let tmp_dir = crate::core::paths::tmp_dir();
    let dl_client = client.clone();
    let asset_name = asset.name.clone();
    let dl_handle = tokio::spawn(async move {
        download::download_to_temp(
            &dl_client,
            &asset.url,
            &tmp_dir,
            &asset_name,
            &progress_tx,
            cancel_dl,
        )
        .await
    });
    // Forward progress while the download runs.
    let event_tx_p = event_tx.clone();
    let repo_id_p = repo_id.clone();
    let asset_p = asset.name.clone();
    let fwd = tokio::spawn(async move {
        while let Some(p) = progress_rx.recv().await {
            let _ = event_tx_p.send(Event::DownloadProgress {
                repo_id: repo_id_p.clone(),
                asset_name: asset_p.clone(),
                downloaded: p.downloaded,
                total: p.total,
            });
        }
    });
    let result = dl_handle.await;
    let _ = fwd.await;
    let (path, sha) = match result {
        Ok(Ok(t)) => t,
        Ok(Err(download::DlError::Cancelled)) => {
            // User cancelled: drop quietly, no gate event (UI already left
            // the Downloading phase on its own cancel).
            let _ = event_tx.send(Event::LogLine("download cancelled".into()));
            return;
        }
        Ok(Err(e)) => {
            let msg = e.to_string();
            let _ = event_tx.send(Event::LogLine(format!("download failed: {msg}")));
            let _ = event_tx.send(Event::DownloadFailed { repo_id, reason: msg });
            return;
        }
        Err(e) => {
            let msg = format!("download task failed: {e}");
            let _ = event_tx.send(Event::LogLine(msg.clone()));
            let _ = event_tx.send(Event::DownloadFailed { repo_id, reason: msg });
            return;
        }
    };
    // VT gate: throttle (sleep until the slot opens), then hash lookup.
    let _ = event_tx.send(Event::ScanStarted {
        repo_id: repo_id.clone(),
        asset_name: asset.name.clone(),
    });
    if let Err(wait) = throttle.try_acquire(Instant::now()) {
        tokio::time::sleep(wait).await;
    }
    let mut outcome = vt::lookup_sha256(&client, &config.vt_api_key, &sha).await;
    // Upload fallback: 404 (Unknown) or never-analysed report (Unscanned)
    // — both mean "no verdict to read yet" (spec line 161).
    let fallback_eligible = matches!(
        outcome,
        vt::LookupOutcome::Unknown | vt::LookupOutcome::Unscanned
    );
    if fallback_eligible && config.vt_upload_fallback {
        let _ = event_tx.send(Event::LogLine(
            "no VT report — upload fallback engaging".to_string(),
        ));
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        match vt::upload_file(&client, &config.vt_api_key, &path, size).await {
            Ok(analysis_id) => {
                let _ = event_tx.send(Event::LogLine(
                    format!("uploaded, analysis {analysis_id} — polling"),
                ));
                match vt::poll_analysis(&client, &config.vt_api_key, &analysis_id, &sha).await {
                    Ok(verdict) => {
                        // Uploaded-by-us verdicts have no engine-name
                        // results in the analysis payload (stats only).
                        outcome = vt::LookupOutcome::Report(verdict, Vec::new());
                    }
                    Err(e) => {
                        outcome = vt::LookupOutcome::NoVerdict(e);
                    }
                }
            }
            Err(e) => {
                outcome = vt::LookupOutcome::NoVerdict(e);
            }
        }
    }
    let _ = event_tx.send(Event::FileReady {
        repo_id,
        asset_name: asset.name.clone(),
        path,
        sha256: sha,
        outcome,
    });
}

/// Fetch one repo's latest release applying its filters; update its
/// NEW/status state. Core of RefreshAll and AddRepo.
/// `limits_out` accumulates rate-limit headers from the responses.
async fn refresh_one(
    client: &reqwest::Client,
    repo: &Repo,
    config: &Config,
    limits_out: &mut RateLimits,
) -> Repo {
    let mut r = repo.clone();
    match &r.host {
        RepoHost::GitHub => {
            let pat = config.github_pat.as_deref();
            // Filters on (prereleases or tag regex) → list endpoint.
            let filter_mode = r.filters.include_prereleases || r.filters.tag_regex.is_some();
            if filter_mode {
                match gh::fetch_releases_list(client, &r.owner, &r.name, pat, &r.filters).await {
                    Ok(Some(rel)) => {
                        r.latest = Some(rel);
                        r.status = RepoStatus::Ok;
                    }
                    Ok(None) => r.status = RepoStatus::NoReleases,
                    Err(e) => return fetch_error_status(&r, e),
                }
            } else {
                // Fast path: latest stable release.
                match gh::fetch_latest(client, &r.owner, &r.name, pat).await {
                    Ok(rel) => {
                        r.latest = Some(rel);
                        r.status = RepoStatus::Ok;
                    }
                    Err(FetchError::NotFound) => {
                        // Prerelease-only repos: releases/latest 404s. Look
                        // with the repo's own filters (spec §Add-time).
                        match gh::fetch_releases_list(client, &r.owner, &r.name, pat, &r.filters).await {
                            Ok(Some(rel)) => {
                                // Only prereleases exist and the user wants
                                // them only if the filter says so.
                                if rel.prerelease && !r.filters.include_prereleases {
                                    r.status = RepoStatus::NoStableRelease;
                                } else {
                                    r.latest = Some(rel);
                                    r.status = RepoStatus::Ok;
                                }
                            }
                            Ok(None) => r.status = RepoStatus::NoReleases,
                            Err(e) => return fetch_error_status(&r, e),
                        }
                    }
                    Err(e) => return fetch_error_status(&r, e),
                }
            }
        }
        RepoHost::GitLab { host } => {
            let pat = config.gitlab_pat.as_deref();
            // Resolve project id if not cached.
            if r.gl_project_id.is_none() {
                match gl::resolve_project_id(client, host, &format!("{}/{}", r.owner, r.name), pat).await {
                    Ok(id) => r.gl_project_id = Some(id),
                    Err(e) => return gl_error_status(&r, e),
                }
            }
            let id = r.gl_project_id.unwrap();
            match gl::fetch_latest(client, host, id, pat).await {
                Ok(Some(rel)) => {
                    r.latest = Some(rel);
                    r.status = RepoStatus::Ok;
                }
                Ok(None) => r.status = RepoStatus::NoReleases,
                Err(e) => return gl_error_status(&r, e),
            }
        }
    }
    // Header capture: latest_snapshot records what the last response sent.
    *limits_out = crate::net::ratelimits::latest_snapshot();
    // Seen-state: a repo with no last_seen_tag adopts its current release
    // as seen (add-time baseline). NEW appears only on later changes.
    if let Some(rel) = &r.latest {
        let changed = r.last_seen_tag.as_deref() != Some(rel.tag.as_str());
        let first_adopt = r.last_seen_tag.is_none();
        if first_adopt {
            r.last_seen_tag = Some(rel.tag.clone());
            r.new_release = false;
        } else {
            r.new_release = changed;
        }
    }
    r
}

/// Spawn the worker. Returns the JoinHandle and the command sender.
/// The worker owns a tokio runtime; UI thread stays sync (egui).
pub fn spawn(
    mut cmd_rx: tokio::sync::mpsc::UnboundedReceiver<Command>,
    event_tx: tokio::sync::mpsc::UnboundedSender<Event>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        rt.block_on(async move {
            // No client-wide total timeout: it kills slow streaming
            // downloads; per-request timeouts cover API calls via
            // reqwest defaults, and VT poll steps carry their own deadline.
            let client = reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(30))
                .build()
                .expect("reqwest client");
            let throttle = std::sync::Arc::new(Throttle::new());
            // Shared cancel flag: UI Cancel button sets it; the download
            // stream checks it between chunks.
            let cancel_flag = Arc::new(AtomicBool::new(false));
            let throttle_rescan = throttle.clone();
            while let Some(cmd) = cmd_rx.recv().await {
                match cmd {
                    Command::RefreshAll { repos, config } => {
                        let sem = Arc::new(tokio::sync::Semaphore::new(
                            config.refresh_concurrency.max(1) as usize,
                        ));
                        let mut handles = Vec::new();
                        let limits = std::sync::Arc::new(std::sync::Mutex::new(RateLimits::default()));
                        for repo in repos {
                            // Acquire the permit and MOVE it into the task so
                            // concurrency is actually enforced.
                            let permit = sem
                                .clone()
                                .acquire_owned()
                                .await
                                .expect("semaphore closed");
                            let config = config.clone();
                            let event_tx = event_tx.clone();
                            let client = client.clone();
                            let limits = limits.clone();
                            handles.push(tokio::spawn(async move {
                                let _permit = permit; // held until task end
                                let mut out = RateLimits::default();
                                let updated = refresh_one(&client, &repo, &config, &mut out).await;
                                {
                                    let mut l = limits.lock().unwrap();
                                    *l = out;
                                }
                                let _ = event_tx.send(Event::RepoUpdated(updated));
                                let _ = event_tx.send(Event::RateLimitsUpdated(
                                    crate::net::ratelimits::latest_snapshot(),
                                ));
                            }));
                        }
                        for h in handles {
                            let _ = h.await;
                        }
                        let _ = event_tx.send(Event::RefreshDone);
                    }
                    Command::AddRepo { url, config } => {
                        let (host, owner, name) =
                            match crate::core::urlparse::parse_repo_url(&url) {
                                Ok(t) => t,
                                Err(e) => {
                                    let _ = event_tx.send(Event::AddResult(Err(e.to_string())));
                                    continue;
                                }
                            };
                        let id = match &host {
                            RepoHost::GitHub => format!("github.com/{owner}/{name}"),
                            RepoHost::GitLab { host: h } => format!("{h}/{owner}/{name}"),
                        };
                        let repo = Repo {
                            id,
                            host,
                            owner,
                            name,
                            category: None,
                            pinned: false,
                            filters: Default::default(),
                            gl_project_id: None,
                            last_seen_tag: None,
                            latest: None,
                            status: RepoStatus::Ok,
                            new_release: false,
                        };
                        let fresh = refresh_one(&client, &repo, &config, &mut RateLimits::default()).await;
                        // Spec §Add-time verification: add fails on bad URL,
                        // nonexistent repo, auth failure, or zero releases.
                        // NoReleases and resolve-failure (Error) both fail.
                        let ok = matches!(fresh.status, RepoStatus::Ok | RepoStatus::NoStableRelease);
                        let _ = event_tx.send(Event::AddResult(if ok { Ok(fresh) } else { Err(add_error(&fresh.status)) }));
                    }
                    Command::DownloadAsset { repo_id, asset, config } => {
                        // Spawn the whole download+scan flow so the bus loop
                        // stays free for other commands (spec: refresh during
                        // a scan must not queue behind it).
                        cancel_flag.store(false, Ordering::Relaxed);
                        let event_tx_d = event_tx.clone();
                        let dl_client = client.clone();
                        let throttle_d = throttle.clone();
                        let cancel_d = cancel_flag.clone();
                        tokio::spawn(async move {
                            run_download_flow(dl_client, throttle_d, cancel_d, event_tx_d, repo_id, asset, config).await;
                        });
                    }
                    Command::RescanAsset { repo_id, asset_name, sha256, config } => {
                        // Throttle: sleep until the slot opens (never skip).
                        if let Err(wait) = throttle_rescan.try_acquire(Instant::now()) {
                            tokio::time::sleep(wait).await;
                        }
                        let outcome = vt::lookup_sha256(&client, &config.vt_api_key, &sha256).await;
                        let _ = event_tx.send(Event::VtVerdictReady {
                            repo_id,
                            asset_name,
                            sha256,
                            outcome,
                        });
                    }
                    Command::CancelDownload => {
                        cancel_flag.store(true, Ordering::Relaxed);
                        let _ = event_tx.send(Event::LogLine("cancel requested".into()));
                    }
                }
            }
        });
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn throttle_first_request_admits_immediately() {
        let t = Throttle::new();
        let now = Instant::now();
        assert!(t.try_acquire(now).is_ok());
    }

    #[test]
    fn throttle_second_request_blocks_until_interval() {
        let t = Throttle::new();
        let now = Instant::now();
        t.try_acquire(now).unwrap();
        let soon = now + Duration::from_secs(1);
        let wait = t.try_acquire(soon).unwrap_err();
        assert_eq!(wait, Duration::from_secs(VT_THROTTLE_SECS - 1));
        // After the full interval, admitted.
        let later = now + Duration::from_secs(VT_THROTTLE_SECS);
        assert!(t.try_acquire(later).is_ok());
    }

    #[test]
    fn throttle_allows_one_per_interval_steadily() {
        let t = Throttle::new();
        let mut now = Instant::now();
        assert!(t.try_acquire(now).is_ok());
        now += Duration::from_secs(VT_THROTTLE_SECS);
        assert!(t.try_acquire(now).is_ok());
        now += Duration::from_secs(VT_THROTTLE_SECS);
        assert!(t.try_acquire(now).is_ok());
    }
}
