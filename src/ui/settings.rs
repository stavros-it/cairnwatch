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
/// `secrets_error`: last Credential Manager failure, shown under the keys.
pub fn show(ui: &mut Ui, config: &mut Config, secrets_error: Option<&str>) -> SettingsAction {
    let mut action = SettingsAction::default();
    ui.heading(RichText::new("Settings").size(Tokens::BODY + 6.0));
    ui.add_space(12.0);

    egui::Frame::none()
        .fill(Tokens::CARD)
        .rounding(8.0)
        .inner_margin(egui::Margin::same(16.0))
        .show(ui, |ui| {
            // One label/field grid so inputs line up (each row started at
            // its label's width before, and hints were cut off at 240px).
            section(ui, "VIRUSTOTAL");
            egui::Grid::new("settings_vt")
                .num_columns(2)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    ui.label("API key");
                    ui.add_sized(
                        [FIELD_W, 44.0],
                        field(&mut config.vt_api_key, "from virustotal.com", true),
                    );
                    ui.end_row();
                    ui.label("");
                    ui.checkbox(&mut config.vt_upload_fallback, "Upload unknown files to VirusTotal (hash lookup first)");
                    ui.end_row();
                });
            ui.add_space(12.0);

            section(ui, "SOURCES");
            egui::Grid::new("settings_src")
                .num_columns(2)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    ui.label("GitHub token");
                    let mut gh = config.github_pat.clone().unwrap_or_default();
                    ui.add_sized([FIELD_W, 44.0], field(&mut gh, "optional — raises 60/hr to 5000/hr", true));
                    config.github_pat = if gh.is_empty() { None } else { Some(gh) };
                    ui.end_row();
                    ui.label("GitLab token");
                    let mut gl = config.gitlab_pat.clone().unwrap_or_default();
                    ui.add_sized([FIELD_W, 44.0], field(&mut gl, "optional", true));
                    config.gitlab_pat = if gl.is_empty() { None } else { Some(gl) };
                    ui.end_row();
                });
            ui.add_space(4.0);
            // Where the three keys live (user decision: keyring only).
            match secrets_error {
                None => ui.label(
                    RichText::new(
                        "Keys are stored in Windows Credential Manager, not in config.toml. They don't travel with the folder; use the encrypted export to move them.",
                    )
                    .size(Tokens::SMALL + 1.0)
                    .color(Tokens::MUTED),
                ),
                Some(e) => ui.label(
                    RichText::new(format!(
                        "Credential Manager error: {e}. Keys work for this session but are NOT saved."
                    ))
                    .size(Tokens::SMALL + 1.0)
                    .color(Tokens::DESTRUCTIVE),
                ),
            };
            ui.add_space(12.0);

            section(ui, "DOWNLOADS");
            egui::Grid::new("settings_dl")
                .num_columns(2)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    ui.label("Folder");
                    ui.horizontal(|ui| {
                        let mut dir = config.download_dir.to_string_lossy().into_owned();
                        ui.add_sized([FIELD_W, 44.0], field(&mut dir, "./downloads", false))
                            .on_hover_text(dir.clone());
                        config.download_dir = dir.into();
                        if ui.button("Open folder").clicked() {
                            let _ = crate::ui::detail::open_browser(&config.download_dir.to_string_lossy());
                        }
                    });
                    ui.end_row();
                    ui.label("Parallel fetches");
                    let mut c = config.refresh_concurrency as i32;
                    ui.scope(|ui| {
                        // 44px interact height made the slider knob huge.
                        ui.spacing_mut().interact_size.y = 28.0;
                        ui.add(egui::Slider::new(&mut c, 1..=16).text("repos at once"));
                    });
                    config.refresh_concurrency = c.max(1) as u32;
                    ui.end_row();
                });
            ui.add_space(12.0);

            section(ui, "BACKUP (portable move)");
            ui.label(
                RichText::new("Export writes one file with repos, seen state, VT cache, stats and settings. Only the encrypted export (AES-256 + PBKDF2) includes your API keys.")
                    .size(Tokens::SMALL + 1.0)
                    .color(Tokens::MUTED),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("Export (plain JSON, no keys)").clicked() {
                    action.export_plain = true;
                }
                if ui.button("Export (encrypted, with keys…)").clicked() {
                    action.export_encrypted = true;
                }
                if ui.button("Import…").clicked() {
                    action.import_file = true;
                }
            });
        });
    action
}

const FIELD_W: f32 = 460.0;

/// Settings text field; `secret` masks keys/tokens on screen.
fn field<'a>(text: &'a mut String, hint: &str, secret: bool) -> egui::TextEdit<'a> {
    egui::TextEdit::singleline(text)
        .hint_text(hint.to_owned())
        .password(secret)
        .vertical_align(egui::Align::Center)
        .desired_width(FIELD_W)
}

/// Small caps section label.
fn section(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(title).size(Tokens::SMALL).color(Tokens::MUTED));
    ui.add_space(4.0);
}
