//! Repo detail view: release notes, asset table, per-repo settings.

use crate::core::models::{Asset, Repo};
use crate::ui::theme::Tokens;
use crate::ui::widgets;
use egui::{RichText, ScrollArea, Ui};

/// Render the detail panel for `repo`. Returns the asset the user clicked
/// Download on, if any this frame.
pub fn show(ui: &mut Ui, repo: &Repo) -> Option<Asset> {
    let mut clicked: Option<Asset> = None;
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
                egui::Frame::none()
                    .fill(Tokens::CARD)
                    .rounding(8.0)
                    .inner_margin(egui::Margin::same(12.0))
                    .show(ui, |ui| {
                        for asset in &rel.assets {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(&asset.name)
                                        .size(Tokens::DATA)
                                        .color(Tokens::TEXT),
                                );
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
    clicked
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
