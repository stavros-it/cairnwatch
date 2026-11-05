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
            (None, _) => ("—".into(), Tokens::MUTED),
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
            quota_row(ui, "GitHub (hourly)", rl.github_remaining.map(|r| 60 - r.min(60)), Some(60), None);
            // GitHub gives remaining + reset; show remaining directly.
            ui.label(
                RichText::new(format!(
                    "remaining: {} · resets at epoch {}",
                    rl.github_remaining.map(|r| r.to_string()).unwrap_or_else(|| "?".into()),
                    rl.github_reset_epoch.map(|r| r.to_string()).unwrap_or_else(|| "?".into())
                ))
                .size(Tokens::SMALL + 2.0)
                .color(Tokens::MUTED),
            );
            ui.add_space(8.0);
            quota_row(ui, "GitLab", rl.gitlab_remaining, None, None);
            ui.add_space(8.0);
            quota_row(ui, "VirusTotal (minute)", rl.vt_minute_used, rl.vt_minute_limit, None);
            quota_row(ui, "VirusTotal (daily)", rl.vt_daily_used, rl.vt_daily_limit, None);
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
    for (label, value, color) in cards {
        egui::Frame::none()
            .fill(Tokens::CARD)
            .rounding(8.0)
            .inner_margin(egui::Margin::symmetric(16.0, 12.0))
            .show(ui, |ui| {
                ui.label(RichText::new(&label).size(Tokens::SMALL + 2.0).color(Tokens::MUTED));
                ui.label(RichText::new(&value).size(Tokens::BODY + 10.0).color(color).strong());
            });
        ui.add_space(8.0);
    }
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
    fn quota_state_smoke() {
        // Pure-render views: exercise construction paths.
        let rl = RateLimits::default();
        assert_eq!(rl.github_remaining, None);
        let stats = Stats::default();
        assert_eq!(stats.downloads_completed, 0);
    }
}
