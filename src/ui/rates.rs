//! Rate-limit health panel: per-API quota state with reset countdowns.

use crate::core::models::Stats;
use crate::net::ratelimits::RateLimits;
use crate::ui::theme::Tokens;
use egui::{Color32, RichText, Ui};

fn quota_row(
    ui: &mut Ui,
    name: &str,
    used: Option<u32>,
    limit: Option<u32>,
    reset_in: Option<String>,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(name).size(Tokens::DATA).strong());
        let (text, color) = match (used, limit) {
            (Some(u), Some(l)) => {
                let pct = if l > 0 { u as f32 / l as f32 } else { 0.0 };
                let c = if pct >= 0.9 {
                    Tokens::DESTRUCTIVE
                } else if pct >= 0.7 {
                    Tokens::AMBER
                } else {
                    Tokens::ACCENT
                };
                (format!("{u} / {l} used"), c)
            }
            (Some(u), None) => (format!("{u} used"), Tokens::TEXT),
            (None, _) => ("— (no request yet)".into(), Tokens::MUTED),
        };
        ui.label(RichText::new(text).size(Tokens::DATA).color(color));
        if let Some(r) = reset_in {
            ui.label(
                RichText::new(format!("resets {r}"))
                    .size(Tokens::SMALL + 2.0)
                    .color(Tokens::MUTED),
            );
        }
    });
}

/// VirusTotal public (free) API limits.
const VT_FREE_PER_MIN: u32 = 4;
const VT_FREE_PER_DAY: u32 = 500;

fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// "in 12 min" / "in 1 h 5 min" / "now". Pure — unit-tested.
pub fn reset_in(reset_epoch: u64, now: u64) -> String {
    let secs = reset_epoch.saturating_sub(now);
    if secs == 0 {
        return "now".into();
    }
    let mins = secs.div_ceil(60);
    if mins < 60 {
        format!("in {mins} min")
    } else {
        format!("in {} h {} min", mins / 60, mins % 60)
    }
}

fn remaining_row(ui: &mut Ui, name: &str, remaining: Option<u32>, reset: Option<u64>, now: u64) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(name).size(Tokens::DATA).strong());
        match remaining {
            None => {
                ui.label(RichText::new("— (no request yet)").size(Tokens::DATA).color(Tokens::MUTED));
            }
            Some(r) => {
                let color = if r == 0 {
                    Tokens::DESTRUCTIVE
                } else if r <= 10 {
                    Tokens::AMBER
                } else {
                    Tokens::ACCENT
                };
                ui.label(RichText::new(format!("{r} remaining")).size(Tokens::DATA).color(color));
                if let Some(e) = reset {
                    ui.label(
                        RichText::new(format!("resets {}", reset_in(e, now)))
                            .size(Tokens::SMALL + 2.0)
                            .color(Tokens::MUTED),
                    );
                }
            }
        }
    });
}

/// Render the rates view from live header state.
pub fn show(ui: &mut Ui, rl: &RateLimits) {
    ui.heading(RichText::new("Rate limits").size(Tokens::BODY + 6.0));
    ui.add_space(8.0);
    ui.label(
        RichText::new("Live from API response headers. Updates as you refresh and scan.")
            .size(Tokens::SMALL + 1.0)
            .color(Tokens::MUTED),
    );
    ui.add_space(12.0);

    egui::Frame::none()
        .fill(Tokens::CARD)
        .rounding(8.0)
        .inner_margin(egui::Margin::same(16.0))
        .show(ui, |ui| {
            // GitHub/GitLab headers give *remaining* (limit is 60/hr
            // anonymous, 5000/hr with a PAT — the old "used of 60" row was
            // wrong with a PAT). Show remaining + a relative reset time.
            let now = now_epoch();
            remaining_row(ui, "GitHub (hourly)", rl.github_remaining, rl.github_reset_epoch, now);
            remaining_row(ui, "GitLab", rl.gitlab_remaining, rl.gitlab_reset_epoch, now);
            ui.add_space(8.0);
            ui.add_space(8.0);
            // VirusTotal sends no quota headers on these endpoints, so
            // fall back to the app's own count vs the free-tier limits.
            let (min_used, min_limit) = match rl.vt_minute_used {
                Some(u) => (Some(u), rl.vt_minute_limit),
                None => (Some(rl.vt_requests_last_minute), Some(VT_FREE_PER_MIN)),
            };
            quota_row(ui, "VirusTotal (last minute)", min_used, min_limit, None);
            let (day_used, day_limit) = match rl.vt_daily_used {
                Some(u) => (Some(u), rl.vt_daily_limit),
                None => (Some(rl.vt_requests_session), Some(VT_FREE_PER_DAY)),
            };
            quota_row(ui, "VirusTotal (this session)", day_used, day_limit, None);
            if rl.vt_daily_used.is_none() {
                ui.label(
                    RichText::new(
                        "VirusTotal reports no quota headers; counts are this app's own requests vs the free tier (4/min, 500/day).",
                    )
                    .size(Tokens::SMALL)
                    .color(Tokens::MUTED),
                );
            }
        });
}

/// Render the statistics view from cumulative counters.
pub fn show_stats(ui: &mut Ui, stats: &Stats, repo_count: usize, new_count: usize) {
    ui.heading(RichText::new("Statistics").size(Tokens::BODY + 6.0));
    ui.add_space(12.0);
    let cards: [(String, String, Color32); 6] = [
        ("Tracked repos".into(), repo_count.to_string(), Tokens::TEXT),
        ("New releases".into(), new_count.to_string(), Tokens::ACCENT),
        ("Downloads started".into(), stats.downloads_started.to_string(), Tokens::TEXT),
        ("Downloads completed".into(), stats.downloads_completed.to_string(), Tokens::ACCENT),
        ("VT scans".into(), stats.vt_scans.to_string(), Tokens::TEXT),
        ("Flagged files seen".into(), stats.vt_flagged_seen.to_string(), Tokens::DESTRUCTIVE),
    ];
    // Equal-width tiles in as many columns as fit (they stacked one per
    // row before; horizontal_wrapped can't wrap Frames).
    const TILE_W: f32 = 200.0;
    let cols = ((ui.available_width() + 12.0) / (TILE_W + 32.0 + 12.0)).floor().max(1.0) as usize;
    egui::Grid::new("stats_tiles").spacing([12.0, 12.0]).show(ui, |ui| {
        for (i, (label, value, color)) in cards.into_iter().enumerate() {
            if i > 0 && i % cols == 0 {
                ui.end_row();
            }
            egui::Frame::none()
                .fill(Tokens::CARD)
                .rounding(8.0)
                .inner_margin(egui::Margin::symmetric(16.0, 12.0))
                .show(ui, |ui| {
                    ui.set_width(TILE_W);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&label).size(Tokens::SMALL + 2.0).color(Tokens::MUTED));
                        ui.label(RichText::new(&value).size(Tokens::BODY + 10.0).color(color).strong());
                    });
                });
        }
    });
    ui.add_space(8.0);
    ui.add_space(8.0);
    ui.label(
        RichText::new("Counters persist in repos.json.")
            .size(Tokens::SMALL)
            .color(Tokens::MUTED),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_in_formats_relative() {
        assert_eq!(reset_in(100, 200), "now");
        assert_eq!(reset_in(100, 100), "now");
        assert_eq!(reset_in(130, 100), "in 1 min");
        assert_eq!(reset_in(100 + 12 * 60, 100), "in 12 min");
        assert_eq!(reset_in(100 + 65 * 60, 100), "in 1 h 5 min");
    }

    #[test]
    fn quota_state_smoke() {
        // Pure-render views: exercise construction paths.
        let rl = RateLimits::default();
        assert_eq!(rl.github_remaining, None);
        let stats = Stats::default();
        assert_eq!(stats.downloads_completed, 0);
    }
}
