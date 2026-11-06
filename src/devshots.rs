//! Debug-build-only UI test driver (compiled out of release builds).
//!
//! Set `GIRUST_SHOTS=<dir>` and run a debug build. The app polls
//! `<dir>/cmd.txt` every ~150ms; each line is one command, the file is
//! deleted once read. Commands:
//!
//! - `view repos|logs|rates|stats|settings`
//! - `select <index>` — open the detail view of the Nth repo (state order)
//! - `back` — leave the detail view
//! - `add` — open the Add-repo dialog; `noadd` closes it
//! - `newfilter on|off`
//! - `search <text>` (empty text clears)
//! - `shot <name>` — save `<dir>/<name>.png` of the current frame
//! - `quit`
//!
//! Real mouse input (hover, clicks) is driven from outside via Win32.
//! Used to capture screenshots when the desktop cannot be screen-grabbed.

use std::path::PathBuf;

pub struct DevShots {
    dir: PathBuf,
    /// Screenshot name waiting for the next `Event::Screenshot`.
    pending: Option<String>,
    last_poll: f64,
}

impl DevShots {
    pub fn from_env() -> Option<Self> {
        let dir = std::env::var_os("GIRUST_SHOTS")?;
        Some(Self {
            dir: PathBuf::from(dir),
            pending: None,
            last_poll: 0.0,
        })
    }
}

impl crate::App {
    pub(crate) fn dev_shots_tick(&mut self, ctx: &egui::Context) {
        let Some(mut shots) = self.dev_shots.take() else {
            return;
        };
        ctx.request_repaint_after(std::time::Duration::from_millis(150));

        // Save any screenshot that arrived this frame.
        let images: Vec<std::sync::Arc<egui::ColorImage>> = ctx.input(|i| {
            i.raw
                .events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
                .collect()
        });
        if let (Some(img), Some(name)) = (images.last(), shots.pending.as_ref()) {
            let [w, h] = img.size;
            let raw: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
            let path = shots.dir.join(format!("{name}.png"));
            if let Err(e) = image::save_buffer(&path, &raw, w as u32, h as u32, image::ColorType::Rgba8) {
                self.log(format!("devshots: save failed: {e}"));
            }
            shots.pending = None;
        }

        let now = ctx.input(|i| i.time);
        if shots.pending.is_none() && now - shots.last_poll > 0.15 {
            shots.last_poll = now;
            let cmd_file = shots.dir.join("cmd.txt");
            if let Ok(bytes) = std::fs::read(&cmd_file) {
                let _ = std::fs::remove_file(&cmd_file);
                // Lossy + BOM-tolerant: a bad encoding must never wedge the
                // driver (the file would never be consumed).
                let text = String::from_utf8_lossy(&bytes);
                let text = text.trim_start_matches('\u{feff}');
                for line in text.lines() {
                    let (cmd, arg) = line.trim().split_once(' ').unwrap_or((line.trim(), ""));
                    match cmd {
                        "view" => {
                            self.view = match arg {
                                "logs" => crate::View::Logs,
                                "rates" => crate::View::Rates,
                                "stats" => crate::View::Stats,
                                "settings" => crate::View::Settings,
                                _ => crate::View::Repos,
                            }
                        }
                        "select" => {
                            let idx: usize = arg.parse().unwrap_or(0);
                            self.selected_repo = self.state.repos.get(idx).map(|r| r.id.clone());
                        }
                        "back" => self.selected_repo = None,
                        "add" => self.show_add_dialog = true,
                        "noadd" => self.show_add_dialog = false,
                        "newfilter" => self.filter_new_only = arg == "on",
                        "search" => self.search = arg.to_string(),
                        "shot" => {
                            shots.pending = Some(arg.to_string());
                            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
                        }
                        "quit" => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                        // Which UI glyphs the bundled fonts can render.
                        "glyphs" => {
                            let probe = [
                                '\u{2713}', '\u{26A0}', '\u{2190}', '\u{2193}', '\u{2B07}',
                                '\u{2022}', '\u{00B7}', '\u{2026}', '\u{2014}', '\u{1F4CC}',
                                '\u{2715}', '\u{1F504}', '\u{2192}', '\u{23F7}',
                            ];
                            let font = egui::FontId::proportional(16.0);
                            let report: String = ctx.fonts(|f| {
                                probe
                                    .iter()
                                    .map(|c| format!("U+{:04X} {}\n", *c as u32, f.has_glyph(&font, *c)))
                                    .collect()
                            });
                            let _ = std::fs::write(shots.dir.join("glyphs.txt"), report);
                        }
                        _ => {}
                    }
                }
            }
        }
        self.dev_shots = Some(shots);
    }
}
