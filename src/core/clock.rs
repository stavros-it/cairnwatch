//! Local wall-clock helpers without a date/time dependency.
//! Windows: `GetLocalTime` (kernel32). Elsewhere: UTC.

/// Days since 1970-01-01 for a civil date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Seconds since the epoch, UTC.
pub fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(windows)]
fn local_now_as_epoch() -> Option<i64> {
    #[repr(C)]
    #[derive(Default)]
    struct SystemTime {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        millis: u16,
    }
    extern "system" {
        fn GetLocalTime(out: *mut SystemTime);
    }
    let mut st = SystemTime::default();
    // SAFETY: GetLocalTime fills the struct we pass; layout matches SYSTEMTIME.
    unsafe { GetLocalTime(&mut st) };
    let days = days_from_civil(st.year as i64, st.month as i64, st.day as i64);
    Some(days * 86_400 + st.hour as i64 * 3600 + st.minute as i64 * 60 + st.second as i64)
}

#[cfg(not(windows))]
fn local_now_as_epoch() -> Option<i64> {
    None
}

/// Local UTC offset in seconds (0 when unknown).
pub fn local_offset_secs() -> i64 {
    local_now_as_epoch()
        .map(|local| local - now_epoch() as i64)
        // Offsets are whole minutes; rounding absorbs the second boundary.
        .map(|d| (d as f64 / 60.0).round() as i64 * 60)
        .unwrap_or(0)
}

/// `HH:MM:SS` of `epoch` shifted by `offset` seconds.
pub fn hms(epoch: u64, offset: i64) -> String {
    let t = (epoch as i64 + offset).rem_euclid(86_400);
    format!("{:02}:{:02}:{:02}", t / 3600, (t % 3600) / 60, t % 60)
}

/// Current local time as `HH:MM:SS`.
pub fn local_hms_now() -> String {
    hms(now_epoch(), local_offset_secs())
}

/// Local `HH:MM` for an epoch (rate-limit reset display).
pub fn local_hm(epoch: u64) -> String {
    hms(epoch, local_offset_secs())[..5].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_days_known_dates() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        assert_eq!(days_from_civil(2026, 11, 8), 20_765);
    }

    #[test]
    fn hms_applies_offset_and_wraps() {
        assert_eq!(hms(0, 0), "00:00:00");
        assert_eq!(hms(3661, 0), "01:01:01");
        assert_eq!(hms(0, 2 * 3600), "02:00:00");
        assert_eq!(hms(0, -3600), "23:00:00");
    }

    #[test]
    fn local_offset_is_sane() {
        let o = local_offset_secs();
        assert!(o % 60 == 0 && o.abs() <= 14 * 3600);
    }
}
