//! Repo detail view: release notes, asset table, per-repo settings.

use crate::core::models::{Asset, Repo};
use crate::ui::theme::Tokens;
use crate::ui::widgets;
use egui::{RichText, ScrollArea, Ui};

/// Render the detail panel for `repo`. Returns the asset the user clicked
/// Download on, if any this frame, plus filter edits.
pub fn show(ui: &mut Ui, repo: &Repo) -> (Option<Asset>, Option<crate::core::models::ReleaseFilters>) {
    let mut clicked: Option<Asset> = None;
    let mut filters_changed: Option<crate::core::models::ReleaseFilters> = None;
    ScrollArea::vertical().show(ui, |ui| {
        ui.add_space(8.0);
        // Header: name, host, pin.
        ui.horizontal(|ui| {
            ui.heading(RichText::new(repo.display_name()).size(Tokens::BODY + 6.0));
            let host_label = match &repo.host {
                crate::core::models::RepoHost::GitHub => "GitHub",
                crate::core::models::RepoHost::GitLab { host } => host,
            };
            widgets::pill(ui, host_label, Tokens::MUTED_BG, false);
            if repo.pinned {
                ui.label(RichText::new("📌 pinned").size(Tokens::SMALL + 2.0).color(Tokens::MUTED));
            }
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
                        filters_changed = Some(filters);
                    }
                });
                ui.horizontal(|ui| {
                    let mut tag_re = repo.filters.tag_regex.clone().unwrap_or_default();
                    let re_field = ui.add_sized(
                        [240.0, 28.0],
                        egui::TextEdit::singleline(&mut tag_re).hint_text("tag regex, e.g. ^v\\d+\\.\\d+"),
                    );
                    let valid = tag_re.is_empty() || regex::Regex::new(&tag_re).is_ok();
                    let _ = re_field;
                    if re_field.changed() {
                        let mut f = repo.filters.clone();
                        f.tag_regex = if tag_re.is_empty() { None } else { Some(tag_re) };
                        filters_changed = Some(f);
                    }
                    if !valid {
                        ui.label(RichText::new("invalid").size(Tokens::SMALL).color(Tokens::DESTRUCTIVE));
                    }
                });
                ui.horizontal(|ui| {
                    let mut pat = repo.filters.asset_pattern.clone().unwrap_or_default();
                    let pat_field = ui.add_sized(
                        [240.0, 28.0],
                        egui::TextEdit::singleline(&mut pat).hint_text("asset glob, e.g. *-x86_64.zip"),
                    );
                    if pat_field.changed() {
                        let mut f = repo.filters.clone();
                        f.asset_pattern = if pat.trim().is_empty() { None } else { Some(pat) };
                        filters_changed = Some(f);
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
                        RichText::new(&rel.published_at)
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
                        for asset in sorted {
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
                                ui.label(
                                    RichText::new(format_bytes(asset.size))
                                        .size(Tokens::DATA)
                                        .color(Tokens::MUTED),
                                );
                                widgets::vt_badge(ui, "—", Tokens::MUTED_BG);
                                if ui
                                    .button(RichText::new("Download").size(Tokens::DATA))
                                    .clicked()
                                {
                                    clicked = Some(asset.clone());
                                }
                            });
                            ui.add_space(4.0);
                        }
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
                                    RichText::new(line)
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
    (clicked, filters_changed)
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

/// Browser-open shim (webbrowser crate not added; shell out).
pub mod open {
    pub fn url(url: &str) -> std::io::Result<()> {
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            std::process::Command::new("cmd")
                .args(["/C", "start", "", url])
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
    fn format_bytes_scales() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(2048), "2.0 KB");
        assert_eq!(format_bytes(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(format_bytes(3 * 1024 * 1024 * 1024), "3.0 GB");
    }
}
