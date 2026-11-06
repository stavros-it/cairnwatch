//! Repo detail view: release notes, asset table, per-repo settings.

use crate::core::models::{Asset, Repo};
use crate::ui::theme::Tokens;
use crate::ui::widgets;
use egui::{RichText, ScrollArea, Ui};

/// Per-asset VT display info (from the cache — review fix).
pub struct AssetVtInfo {
    /// Cached verdict label, e.g. "0/72" or "3/72".
    pub label: String,
    pub flagged: bool,
    pub sha256: String,
}

/// Per-asset action results from the detail panel: Download, filter
/// edits, Re-scan request, Remove-repo request.
pub struct DetailActions {
    pub download: Option<Asset>,
    pub filters_changed: Option<crate::core::models::ReleaseFilters>,
    pub rescan: Option<(String, String)>,
    pub remove: bool,
}

/// Render the detail panel for `repo`. Returns the asset the user clicked
/// Download on, filter edits, a Re-scan request, and a Remove-repo request.
/// `vt_info` maps asset name → cached verdict info for badges.
pub fn show(
    ui: &mut Ui,
    repo: &Repo,
    vt_info: &dyn Fn(&str) -> Option<AssetVtInfo>,
) -> DetailActions {
    let mut actions = DetailActions {
        download: None,
        filters_changed: None,
        rescan: None,
        remove: false,
    };
    ScrollArea::vertical().show(ui, |ui| {
        ui.add_space(8.0);
        // Header: name, host, pin, remove.
        ui.horizontal(|ui| {
            ui.heading(RichText::new(repo.display_name()).size(Tokens::BODY + 6.0));
            let host_label = match &repo.host {
                crate::core::models::RepoHost::GitHub => "GitHub",
                crate::core::models::RepoHost::GitLab { host } => host,
            };
            widgets::pill(ui, host_label, Tokens::MUTED, false);
            if repo.pinned {
                ui.label(RichText::new("📌 pinned").size(Tokens::SMALL + 2.0).color(Tokens::MUTED));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Remove affordance: visible button (the right-click
                // context menu on the list row was the only path before —
                // undiscoverable; users read this as "cannot remove").
                let remove = egui::Button::new(
                    RichText::new("Remove repo").size(Tokens::SMALL + 2.0),
                )
                .fill(Tokens::MUTED_BG);
                if ui
                    .add(remove)
                    .on_hover_text("Remove this repo from the list (its downloads are kept)")
                    .clicked()
                {
                    actions.remove = true;
                }
            });
        });
        ui.add_space(8.0);

        // Per-repo filters (Task 8): prerelease toggle, tag regex, asset glob.
        egui::Frame::none()
            .fill(Tokens::MUTED_BG.gamma_multiply(0.4))
            .rounding(8.0)
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                ui.label(RichText::new("FILTERS").size(Tokens::SMALL).color(Tokens::MUTED));
                ui.horizontal(|ui| {
                    let mut filters = repo.filters.clone();
                    if ui
                        .checkbox(&mut filters.include_prereleases, "Include prereleases")
                        .changed()
                    {
                        actions.filters_changed = Some(filters);
                    }
                });
                ui.horizontal(|ui| {
                    let mut tag_re = repo.filters.tag_regex.clone().unwrap_or_default();
                    let re_field = ui.add_sized(
                        [300.0, 44.0],
                        egui::TextEdit::singleline(&mut tag_re)
                            .hint_text("tag regex, e.g. ^v\\d+\\.\\d+")
                            .vertical_align(egui::Align::Center),
                    );
                    let valid = tag_re.is_empty() || regex::Regex::new(&tag_re).is_ok();
                    let _ = re_field;
                    if re_field.changed() {
                        let mut f = repo.filters.clone();
                        f.tag_regex = if tag_re.is_empty() { None } else { Some(tag_re) };
                        actions.filters_changed = Some(f);
                    }
                    if !valid {
                        ui.label(RichText::new("invalid").size(Tokens::SMALL).color(Tokens::DESTRUCTIVE));
                    }
                });
                ui.horizontal(|ui| {
                    let mut pat = repo.filters.asset_pattern.clone().unwrap_or_default();
                    let pat_field = ui.add_sized(
                        [300.0, 44.0],
                        egui::TextEdit::singleline(&mut pat)
                            .hint_text("asset glob, e.g. *-x86_64.zip")
                            .vertical_align(egui::Align::Center),
                    );
                    if pat_field.changed() {
                        let mut f = repo.filters.clone();
                        f.asset_pattern = if pat.trim().is_empty() { None } else { Some(pat) };
                        actions.filters_changed = Some(f);
                    }
                    ui.label(RichText::new("matching assets sort first").size(Tokens::SMALL).color(Tokens::MUTED));
                });
            });
        ui.add_space(8.0);

        match &repo.latest {
            None => {
                ui.label(
                    RichText::new(crate::ui::list::status_label(&repo.status))
                        .size(Tokens::BODY)
                        .color(Tokens::MUTED),
                );
            }
            Some(rel) => {
                ui.horizontal(|ui| {
                    widgets::version_pill(ui, &rel.tag, repo.new_release);
                    ui.label(
                        RichText::new(short_date(&rel.published_at))
                            .size(Tokens::DATA)
                            .color(Tokens::MUTED),
                    );
                    if rel.prerelease {
                        widgets::pill(ui, "prerelease", Tokens::AMBER, false);
                    }
                    if ui
                        .button(RichText::new("Open release page").size(Tokens::DATA))
                        .clicked()
                    {
                        let url = release_page_url(repo, &rel.tag);
                        let _ = open::url(&url);
                    }
                });
                ui.add_space(12.0);

                ui.label(
                    RichText::new("ASSETS")
                        .size(Tokens::SMALL)
                        .color(Tokens::MUTED),
                );
                ui.add_space(4.0);
                // Asset table: pattern-matching assets sort first + get a
                // "match" mark (Task 8).
                let pattern = repo.filters.asset_pattern.as_deref();
                let mut sorted: Vec<&Asset> = rel.assets.iter().collect();
                sorted.sort_by_key(|a| !crate::ui::list::asset_matches(pattern, &a.name));
                egui::Frame::none()
                    .fill(Tokens::CARD)
                    .rounding(8.0)
                    .inner_margin(egui::Margin::same(12.0))
                    .show(ui, |ui| {
                        // Grid keeps size/VT/buttons in aligned columns (rows
                        // drifted with name length before).
                        egui::Grid::new("assets")
                            .num_columns(4)
                            .spacing([16.0, 8.0])
                            .min_row_height(44.0)
                            .show(ui, |ui| {
                                for asset in sorted {
                                    asset_row(ui, asset, pattern, vt_info, &mut actions);
                                }
                            });
                        if rel.assets.is_empty() {
                            ui.label(
                                RichText::new("No downloadable assets in this release")
                                    .size(Tokens::DATA)
                                    .color(Tokens::MUTED),
                            );
                        }
                    });
                ui.add_space(12.0);

                // Release notes as styled plain text (bold headings heuristic:
                // lines starting with # get heading size).
                ui.label(
                    RichText::new("NOTES")
                        .size(Tokens::SMALL)
                        .color(Tokens::MUTED),
                );
                ui.add_space(4.0);
                egui::Frame::none()
                    .fill(Tokens::CARD)
                    .rounding(8.0)
                    .inner_margin(egui::Margin::same(12.0))
                    .show(ui, |ui| {
                        for line in rel.notes.lines() {
                            if line.trim_start().starts_with('#') {
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new(line.trim_start_matches('#').trim())
                                        .size(Tokens::BODY)
                                        .strong()
                                        .color(Tokens::TEXT),
                                );
                            } else if !line.trim().is_empty() {
                                ui.label(
                                    RichText::new(clean_md_line(line))
                                        .size(Tokens::DATA + 1.0)
                                        .color(Tokens::TEXT),
                                );
                            }
                        }
                        if rel.notes.trim().is_empty() {
                            ui.label(
                                RichText::new("(no notes)")
                                    .size(Tokens::DATA)
                                    .color(Tokens::MUTED),
                            );
                        }
                    });
            }
        }
    });
    actions
}

/// One asset row in the 4-column grid: name(+match) · size · VT · Download.
fn asset_row(
    ui: &mut Ui,
    asset: &Asset,
    pattern: Option<&str>,
    vt_info: &dyn Fn(&str) -> Option<AssetVtInfo>,
    actions: &mut DetailActions,
) {
    let matched = crate::ui::list::asset_matches(pattern, &asset.name);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(&asset.name)
                .size(Tokens::DATA)
                .color(if matched { Tokens::TEXT } else { Tokens::MUTED }),
        );
        if matched {
            widgets::pill(ui, "match", Tokens::ACCENT, false);
        }
    });
    ui.label(
        RichText::new(format_bytes(asset.size))
            .size(Tokens::DATA)
            .color(Tokens::MUTED),
    );
    // VT badge from cache (review fix) or "not scanned".
    ui.horizontal(|ui| {
        if let Some(info) = vt_info(&asset.name) {
            widgets::vt_badge(
                ui,
                &format!("VT {}", info.label),
                if info.flagged { Tokens::DESTRUCTIVE } else { Tokens::ACCENT },
            );
            if ui.button(RichText::new("Re-scan").size(Tokens::DATA)).clicked() {
                actions.rescan = Some((asset.name.clone(), info.sha256.clone()));
            }
        } else {
            widgets::vt_badge(ui, "not scanned", Tokens::MUTED);
        }
    });
    let dl = egui::Button::new(
        RichText::new("Download").size(Tokens::DATA).color(Tokens::ON_ACCENT),
    )
    .fill(Tokens::ACCENT);
    if ui
        .add(dl)
        .on_hover_text("Downloads to a temp folder, scans on VirusTotal, then delivers")
        .clicked()
    {
        actions.download = Some(asset.clone());
    }
    ui.end_row();
}

/// `2026-10-30T12:00:00Z` → `2026-10-30`; anything shorter passes through.
pub fn short_date(ts: &str) -> &str {
    ts.get(..10).unwrap_or(ts)
}

/// Light markdown cleanup for one notes line: `**`/`__`/backticks dropped,
/// `- `/`* ` bullets become `• `, `[text](url)` keeps the text.
pub fn clean_md_line(line: &str) -> String {
    let trimmed = line.trim_start();
    let indent = &line[..line.len() - trimmed.len()];
    let body = if let Some(rest) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")) {
        format!("{indent}• {rest}")
    } else {
        line.to_string()
    };
    let body = body.replace("**", "").replace("__", "").replace('`', "");
    // [text](url) → text
    let mut out = String::with_capacity(body.len());
    let mut rest = body.as_str();
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        match (after.find("]("), after.find(']')) {
            (Some(close), Some(first_close)) if close == first_close => {
                let tail = &after[close + 2..];
                if let Some(paren) = tail.find(')') {
                    out.push_str(&rest[..open]);
                    out.push_str(&after[..close]);
                    rest = &tail[paren + 1..];
                    continue;
                }
                out.push_str(&rest[..=open]);
                rest = after;
            }
            _ => {
                out.push_str(&rest[..=open]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn release_page_url(repo: &Repo, tag: &str) -> String {
    match &repo.host {
        crate::core::models::RepoHost::GitHub => {
            format!("https://github.com/{}/{}/releases/tag/{}", repo.owner, repo.name, tag)
        }
        crate::core::models::RepoHost::GitLab { host } => {
            format!("https://{host}/{}/{}/-/releases/{}", repo.owner, repo.name, tag)
        }
    }
}

pub fn format_bytes(n: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if n >= GB {
        format!("{:.1} GB", n as f64 / GB as f64)
    } else if n >= MB {
        format!("{:.1} MB", n as f64 / MB as f64)
    } else if n >= KB {
        format!("{:.1} KB", n as f64 / KB as f64)
    } else {
        format!("{n} B")
    }
}

/// Browser-open shim. Uses rundll32 FileProtocolHandler on Windows —
/// `cmd /C start` with an unquoted URL allows argument injection
/// (e.g. a git tag containing `&`), a review CRITICAL.
pub mod open {
    pub fn url(url: &str) -> std::io::Result<()> {
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            std::process::Command::new("rundll32")
                .args(["url.dll,FileProtocolHandler", url])
                .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
                .spawn()?;
        }
        #[cfg(not(target_os = "windows"))]
        {
            std::process::Command::new("xdg-open").arg(url).spawn()?;
        }
        Ok(())
    }
}

pub fn open_browser(url: &str) -> std::io::Result<()> {
    open::url(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_date_trims_rfc3339() {
        assert_eq!(short_date("2026-10-30T12:00:00Z"), "2026-10-30");
        assert_eq!(short_date("2026"), "2026");
    }

    #[test]
    fn clean_md_line_strips_common_markup() {
        assert_eq!(clean_md_line("- **bold** fix"), "• bold fix");
        assert_eq!(clean_md_line("  * `code` item"), "  • code item");
        assert_eq!(clean_md_line("see [docs](https://x.y/z) now"), "see docs now");
        assert_eq!(clean_md_line("array[0] stays"), "array[0] stays");
        assert_eq!(clean_md_line("[broken](no close"), "[broken](no close");
        assert_eq!(clean_md_line("plain"), "plain");
    }

    #[test]
    fn format_bytes_scales() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(2048), "2.0 KB");
        assert_eq!(format_bytes(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(format_bytes(3 * 1024 * 1024 * 1024), "3.0 GB");
    }
}
