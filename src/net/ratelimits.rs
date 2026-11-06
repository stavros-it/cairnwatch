//! Rate-limit header parsing for the three API dialects.
//!
//! GitHub: `x-ratelimit-remaining` / `x-ratelimit-reset` (epoch seconds)
//! GitLab: `RateLimit-Remaining` / `RateLimit-Reset` (epoch, some instances)
//! VirusTotal: `x-quota-user-limit-minute: 4`, `x-quota-user-request-minute: 3`,
//!   `x-quota-user-limit-day: 500`, `x-quota-user-request-day: 211`

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimits {
    pub github_remaining: Option<u32>,
    pub github_reset_epoch: Option<u64>,
    pub gitlab_remaining: Option<u32>,
    pub gitlab_reset_epoch: Option<u64>,
    pub vt_minute_limit: Option<u32>,
    pub vt_minute_used: Option<u32>,
    pub vt_daily_limit: Option<u32>,
    pub vt_daily_used: Option<u32>,
    /// App-side VT request counts. VirusTotal sends no quota headers on
    /// file/analysis responses (verified live), so these are the only
    /// numbers the Rates view can show for VT.
    #[serde(default)]
    pub vt_requests_last_minute: u32,
    #[serde(default)]
    pub vt_requests_session: u32,
}

impl RateLimits {
    pub fn from_headers(headers: &reqwest::header::HeaderMap) -> RateLimits {
        let mut rl = RateLimits::default();
        for (name, value) in headers {
            let Ok(v) = value.to_str() else { continue };
            match name.as_str().to_ascii_lowercase().as_str() {
                "x-ratelimit-remaining" => rl.github_remaining = v.parse().ok(),
                "x-ratelimit-reset" => rl.github_reset_epoch = v.parse().ok(),
                "ratelimit-remaining" => rl.gitlab_remaining = v.parse().ok(),
                "ratelimit-reset" => rl.gitlab_reset_epoch = v.parse().ok(),
                "x-quota-user-limit-minute" => rl.vt_minute_limit = v.parse().ok(),
                "x-quota-user-request-minute" => rl.vt_minute_used = v.parse().ok(),
                "x-quota-user-limit-day" => rl.vt_daily_limit = v.parse().ok(),
                "x-quota-user-request-day" => rl.vt_daily_used = v.parse().ok(),
                _ => {}
            }
        }
        rl
    }

    /// Overlay the fields `other` actually carries. Each API only sends
    /// its own headers, so replacing wholesale (the old behavior) let a
    /// GitHub response erase GitLab/VT numbers and vice versa.
    pub fn merge(&mut self, other: &RateLimits) {
        fn set<T: Copy>(dst: &mut Option<T>, src: Option<T>) {
            if src.is_some() {
                *dst = src;
            }
        }
        set(&mut self.github_remaining, other.github_remaining);
        set(&mut self.github_reset_epoch, other.github_reset_epoch);
        set(&mut self.gitlab_remaining, other.gitlab_remaining);
        set(&mut self.gitlab_reset_epoch, other.gitlab_reset_epoch);
        set(&mut self.vt_minute_limit, other.vt_minute_limit);
        set(&mut self.vt_minute_used, other.vt_minute_used);
        set(&mut self.vt_daily_limit, other.vt_daily_limit);
        set(&mut self.vt_daily_used, other.vt_daily_used);
    }
}

/// Record one API response's rate-limit headers into the process-wide
/// snapshot. Called on every GitHub/GitLab/VirusTotal response — before
/// this, nothing called it for GitHub/GitLab, so Rates always showed
/// "no request yet".
pub fn observe(headers: &reqwest::header::HeaderMap) {
    let rl = RateLimits::from_headers(headers);
    latest().lock().unwrap().merge(&rl);
}

/// Process-global latest observed limits, read by the bus after any fetch.
static LATEST: std::sync::OnceLock<std::sync::Mutex<RateLimits>> = std::sync::OnceLock::new();

fn latest() -> &'static std::sync::Mutex<RateLimits> {
    LATEST.get_or_init(|| std::sync::Mutex::new(RateLimits::default()))
}

/// VT response: merge any headers and count the request.
pub fn observe_vt(headers: &reqwest::header::HeaderMap) {
    observe(headers);
    let mut log = vt_log().lock().unwrap();
    log.push_back(std::time::Instant::now());
    SESSION_VT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    prune(&mut log, std::time::Instant::now());
}

static SESSION_VT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static VT_LOG: std::sync::OnceLock<std::sync::Mutex<std::collections::VecDeque<std::time::Instant>>> =
    std::sync::OnceLock::new();

fn vt_log() -> &'static std::sync::Mutex<std::collections::VecDeque<std::time::Instant>> {
    VT_LOG.get_or_init(Default::default)
}

/// Drop request instants older than 60 s. Pure on its inputs — tested.
fn prune(log: &mut std::collections::VecDeque<std::time::Instant>, now: std::time::Instant) {
    while log
        .front()
        .is_some_and(|t| now.duration_since(*t) >= std::time::Duration::from_secs(60))
    {
        log.pop_front();
    }
}

/// Snapshot of the most recent rate-limit headers seen on any API
/// response, plus the app-side VT request counts.
pub fn latest_snapshot() -> RateLimits {
    let mut rl = latest().lock().unwrap().clone();
    let mut log = vt_log().lock().unwrap();
    prune(&mut log, std::time::Instant::now());
    rl.vt_requests_last_minute = log.len() as u32;
    rl.vt_requests_session = SESSION_VT.load(std::sync::atomic::Ordering::Relaxed);
    rl
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prune_drops_only_entries_older_than_a_minute() {
        use std::time::{Duration, Instant};
        let now = Instant::now() + Duration::from_secs(120);
        let mut log: std::collections::VecDeque<Instant> = [
            now - Duration::from_secs(90),
            now - Duration::from_secs(60),
            now - Duration::from_secs(59),
            now,
        ]
        .into_iter()
        .collect();
        prune(&mut log, now);
        assert_eq!(log.len(), 2);
    }

    #[test]
    fn merge_keeps_fields_the_new_response_lacks() {
        let mut acc = RateLimits {
            gitlab_remaining: Some(1999),
            vt_daily_used: Some(3),
            ..Default::default()
        };
        let gh = RateLimits {
            github_remaining: Some(57),
            github_reset_epoch: Some(1_800_000_000),
            ..Default::default()
        };
        acc.merge(&gh);
        assert_eq!(acc.github_remaining, Some(57));
        assert_eq!(acc.gitlab_remaining, Some(1999)); // not wiped
        assert_eq!(acc.vt_daily_used, Some(3));
        acc.merge(&RateLimits::default()); // header-less response
        assert_eq!(acc.github_remaining, Some(57));
    }

    fn headers(pairs: &[(&str, &str)]) -> reqwest::header::HeaderMap {
        let mut hm = reqwest::header::HeaderMap::new();
        for (k, v) in pairs {
            hm.insert(
                reqwest::header::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                v.parse().unwrap(),
            );
        }
        hm
    }

    #[test]
    fn github_dialect_parses() {
        let hm = headers(&[
            ("x-ratelimit-remaining", "42"),
            ("x-ratelimit-reset", "1767321600"),
        ]);
        let rl = RateLimits::from_headers(&hm);
        assert_eq!(rl.github_remaining, Some(42));
        assert_eq!(rl.github_reset_epoch, Some(1767321600));
    }

    #[test]
    fn gitlab_dialect_parses() {
        let hm = headers(&[
            ("RateLimit-Remaining", "180"),
            ("RateLimit-Reset", "1767321600"),
        ]);
        let rl = RateLimits::from_headers(&hm);
        assert_eq!(rl.gitlab_remaining, Some(180));
        assert_eq!(rl.gitlab_reset_epoch, Some(1767321600));
    }

    #[test]
    fn vt_dialect_parses_used_and_limit() {
        let hm = headers(&[
            ("x-quota-user-limit-minute", "4"),
            ("x-quota-user-request-minute", "3"),
            ("x-quota-user-limit-day", "500"),
            ("x-quota-user-request-day", "211"),
        ]);
        let rl = RateLimits::from_headers(&hm);
        assert_eq!(rl.vt_minute_limit, Some(4));
        assert_eq!(rl.vt_minute_used, Some(3));
        assert_eq!(rl.vt_daily_limit, Some(500));
        assert_eq!(rl.vt_daily_used, Some(211));
    }

    #[test]
    fn mixed_headers_parse_independently() {
        let hm = headers(&[
            ("x-ratelimit-remaining", "10"),
            ("x-quota-user-limit-day", "500"),
        ]);
        let rl = RateLimits::from_headers(&hm);
        assert_eq!(rl.github_remaining, Some(10));
        assert_eq!(rl.vt_daily_limit, Some(500));
        assert_eq!(rl.vt_minute_limit, None);
        assert_eq!(rl.gitlab_remaining, None);
    }

    #[test]
    fn garbage_values_are_ignored() {
        let hm = headers(&[("x-ratelimit-remaining", "not-a-number")]);
        let rl = RateLimits::from_headers(&hm);
        assert_eq!(rl.github_remaining, None);
    }

    #[test]
    fn empty_headers_yield_default() {
        let rl = RateLimits::from_headers(&reqwest::header::HeaderMap::new());
        assert_eq!(rl, RateLimits::default());
    }
}
