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
}

#[cfg(test)]
mod tests {
    use super::*;

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
