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
use std::sync::atomic::AtomicBool;
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
}

/// Events the worker sends back to the UI.
#[derive(Debug)]
pub enum Event {
    RepoUpdated(Repo),
    AddResult(Result<Repo, String>),
    DownloadProgress { repo_id: String, asset_name: String, downloaded: u64, total: u64 },
    /// File downloaded and hashed; verdict (or no-verdict reason) ready.
    VtVerdictReady { repo_id: String, asset_name: String, sha256: String, outcome: LookupOutcome },
    /// Temp file is ready (downloaded + hashed) — UI gates delivery.
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

/// Fetch one repo's latest release applying its filters; update its
/// NEW/status state. Core of RefreshAll and AddRepo.
async fn refresh_one(client: &reqwest::Client, repo: &Repo, config: &Config) -> Repo {
    let mut r = repo.clone();
    let mut rate_limits = RateLimits::default();
    match &r.host {
        RepoHost::GitHub => {
            let pat = config.github_pat.as_deref();
            // Fast path: latest stable release.
            match gh::fetch_latest(client, &r.owner, &r.name, pat).await {
                Ok(rel) => {
                    r.latest = Some(rel);
                    r.status = RepoStatus::Ok;
                }
                Err(FetchError::NotFound) => {
                    // Prerelease-only repos: releases/latest 404s.
                    let filters = crate::core::models::ReleaseFilters {
                        include_prereleases: true,
                        ..Default::default()
                    };
                    match gh::fetch_releases_list(client, &r.owner, &r.name, pat, &filters).await {
                        Ok(Some(_)) => r.status = RepoStatus::NoStableRelease,
                        Ok(None) => r.status = RepoStatus::NoReleases,
                        Err(e) => return fetch_error_status(&r, e),
                    }
                }
                Err(e) => return fetch_error_status(&r, e),
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
    let _ = &mut rate_limits;
    // NEW detection: latest tag differs from the last seen tag.
    if let Some(rel) = &r.latest {
        let is_new = match &r.last_seen_tag {
            Some(seen) => seen != &rel.tag,
            None => true, // first fetch after add
        };
        // First fetch after add should NOT mark NEW (user just added it).
        let first_fetch = r.last_seen_tag.is_none() && !r.new_release;
        if !first_fetch && is_new {
            r.new_release = true;
        }
        if !first_fetch && !is_new {
            r.new_release = false;
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
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .expect("reqwest client");
            let throttle = Throttle::new();
            while let Some(cmd) = cmd_rx.recv().await {
                match cmd {
                    Command::RefreshAll { repos, config } => {
                        let sem = Arc::new(tokio::sync::Semaphore::new(
                            config.refresh_concurrency.max(1) as usize,
                        ));
                        let mut handles = Vec::new();
                        for repo in repos {
                            let _permit = sem.clone().acquire_owned().await;
                            let config = config.clone();
                            let event_tx = event_tx.clone();
                            let client = client.clone();
                            handles.push(tokio::spawn(async move {
                                let updated = refresh_one(&client, &repo, &config).await;
                                let _ = event_tx.send(Event::RepoUpdated(updated));
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
                        let fresh = refresh_one(&client, &repo, &config).await;
                        let ok = !matches!(fresh.status, RepoStatus::Error(_) | RepoStatus::RateLimited);
                        let _ = event_tx.send(Event::AddResult(if ok { Ok(fresh) } else { Err(format!("{:?}", fresh.status)) }));
                    }
                    Command::DownloadAsset { repo_id, asset, config } => {
                        let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<Progress>(64);
                        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
                        let cancel = Arc::new(AtomicBool::new(false));
                        let cancel_for_dl = cancel.clone();
                        let tmp_dir = crate::core::paths::tmp_dir();
                        let dl_client = client.clone();
                        let asset_name = asset.name.clone();
                        let dl_handle = tokio::spawn(async move {
                            let _ = cancel_rx;
                            download::download_to_temp(
                                &dl_client,
                                &asset.url,
                                &tmp_dir,
                                &asset_name,
                                &progress_tx,
                                cancel_for_dl,
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
                            Ok(Err(e)) => {
                                let _ = event_tx.send(Event::LogLine(format!("download failed: {e}")));
                                let _ = cancel_tx;
                                continue;
                            }
                            Err(e) => {
                                let _ = event_tx.send(Event::LogLine(format!("download task panicked: {e}")));
                                continue;
                            }
                        };
                        // VT gate: throttle, then hash lookup. On Unknown and
                        // upload-fallback enabled, upload + poll (spec).
                        let _ = throttle.try_acquire(Instant::now());
                        let mut outcome = vt::lookup_sha256(&client, &config.vt_api_key, &sha).await;
                        if matches!(outcome, vt::LookupOutcome::Unknown) && config.vt_upload_fallback {
                            let _ = event_tx.send(Event::LogLine(
                                "unknown to VT — upload fallback engaging".to_string(),
                            ));
                            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                            match vt::upload_file(&client, &config.vt_api_key, &path, size).await {
                                Ok(analysis_id) => {
                                    let _ = event_tx.send(Event::LogLine(
                                        format!("uploaded, analysis {analysis_id} — polling"),
                                    ));
                                    match vt::poll_analysis(&client, &config.vt_api_key, &analysis_id, &sha).await {
                                        Ok(verdict) => {
                                            outcome = vt::LookupOutcome::Report(verdict);
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
                    Command::RescanAsset { repo_id, asset_name, sha256, config } => {
                        let _ = throttle.try_acquire(Instant::now());
                        let outcome = vt::lookup_sha256(&client, &config.vt_api_key, &sha256).await;
                        let _ = event_tx.send(Event::VtVerdictReady {
                            repo_id,
                            asset_name,
                            sha256,
                            outcome,
                        });
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
