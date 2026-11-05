//! Settings view: VT key, PATs, download dir, concurrency, import/export.

use crate::core::config::Config;
use crate::ui::theme::Tokens;
use egui::{RichText, Ui};

/// What the settings view wants the App to do this frame.
#[derive(Debug, Default)]
pub struct SettingsAction {
    pub export_plain: bool,
    pub export_encrypted: bool,
    pub import_file: bool,
}

/// Render settings editing `config` in place. Returns requested actions.
pub fn show(ui: &mut Ui, config: &mut Config) -> SettingsAction {
    let mut action = SettingsAction::default();
    ui.heading(RichText::new("Settings").size(Tokens::BODY + 6.0));
    ui.add_space(12.0);

    egui::Frame::none()
        .fill(Tokens::CARD)
        .rounding(8.0)
        .inner_margin(egui::Margin::same(16.0))
        .show(ui, |ui| {
            ui.label(RichText::new("VIRUSTOTAL").size(Tokens::SMALL).color(Tokens::MUTED));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("API key:");
                ui.add_sized(
                    [320.0, 32.0],
                    egui::TextEdit::singleline(&mut config.vt_api_key)
                        .hint_text("from virustotal.com (stored in plaintext next to the exe)"),
                );
            });
            ui.checkbox(&mut config.vt_upload_fallback, "Upload unknown files to VirusTotal (hash lookup first)");
            ui.add_space(12.0);

            ui.label(RichText::new("SOURCES").size(Tokens::SMALL).color(Tokens::MUTED));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("GitHub PAT:");
                let mut gh = config.github_pat.clone().unwrap_or_default();
                ui.add_sized(
                    [240.0, 32.0],
                    egui::TextEdit::singleline(&mut gh).hint_text("optional — raises 60/hr to 5000/hr"),
                );
                config.github_pat = if gh.is_empty() { None } else { Some(gh) };
            });
            ui.horizontal(|ui| {
                ui.label("GitLab token:");
                let mut gl = config.gitlab_pat.clone().unwrap_or_default();
                ui.add_sized(
                    [240.0, 32.0],
                    egui::TextEdit::singleline(&mut gl).hint_text("optional"),
                );
                config.gitlab_pat = if gl.is_empty() { None } else { Some(gl) };
            });
            ui.add_space(12.0);

            ui.label(RichText::new("DOWNLOADS").size(Tokens::SMALL).color(Tokens::MUTED));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Folder:");
                let mut dir = config.download_dir.to_string_lossy().into_owned();
                ui.add_sized(
                    [240.0, 32.0],
                    egui::TextEdit::singleline(&mut dir).hint_text("./downloads"),
                );
                config.download_dir = dir.into();
                if ui.button("Open folder").clicked() {
                    let _ = crate::ui::detail::open_browser(&config.download_dir.to_string_lossy());
                }
            });
            ui.add_space(12.0);

            ui.label(RichText::new("REFRESH").size(Tokens::SMALL).color(Tokens::MUTED));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Parallel repo fetches:");
                let mut c = config.refresh_concurrency as i32;
                ui.add(egui::Slider::new(&mut c, 1..=16));
                config.refresh_concurrency = c.max(1) as u32;
            });
            ui.add_space(12.0);

            ui.label(RichText::new("BACKUP (portable move)").size(Tokens::SMALL).color(Tokens::MUTED));
            ui.add_space(4.0);
            ui.label(
                RichText::new("Export writes one JSON file with everything (repos, seen state, VT cache, stats, settings). Encrypted export uses AES-256 + PBKDF2.")
                    .size(Tokens::SMALL + 1.0)
                    .color(Tokens::MUTED),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("Export (plain JSON)").clicked() {
                    action.export_plain = true;
                }
                if ui.button("Export (encrypted…)").clicked() {
                    action.export_encrypted = true;
                }
                if ui.button("Import…").clicked() {
                    action.import_file = true;
                }
            });
        });
    action
}
