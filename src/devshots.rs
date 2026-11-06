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
//! - `mouse X Y` / `click X Y` / `rclick X Y` — synthetic pointer input
//!   (logical px), injected through `raw_input_hook`
//! - `key esc|enter|tab`
//! - `gate downloading|scanning|flagged|flagged-noengines|unverified|done|failed|idle`
//! - `category <index>` — open the category dialog for the Nth repo
//! - `rescan clean|flagged|none` — show the re-scan result dialog
//! - `glyphs` — write `<dir>/glyphs.txt` (font coverage probe)
//!
//! Used to capture screenshots when the desktop cannot be screen-grabbed.

use std::path::PathBuf;

pub struct DevShots {
    dir: PathBuf,
    /// Screenshot name waiting for the next `Event::Screenshot`.
    pending: Option<String>,
    last_poll: f64,
    /// One entry per future frame: events injected by `raw_input_hook`
    /// (press and release land on separate frames, like a real click).
    inject: std::collections::VecDeque<Vec<egui::Event>>,
}

impl DevShots {
    /// Next frame's synthetic events (called from `raw_input_hook`).
    pub fn take_injected(&mut self) -> Vec<egui::Event> {
        self.inject.pop_front().unwrap_or_default()
    }

    fn pointer(&mut self, arg: &str, button: Option<egui::PointerButton>) {
        let mut it = arg.split_whitespace().filter_map(|v| v.parse::<f32>().ok());
        let (Some(x), Some(y)) = (it.next(), it.next()) else {
            return;
        };
        let pos = egui::pos2(x, y);
        self.inject.push_back(vec![egui::Event::PointerMoved(pos)]);
        if let Some(button) = button {
            let ev = |pressed| egui::Event::PointerButton {
                pos,
                button,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            self.inject.push_back(vec![ev(true)]);
            self.inject.push_back(vec![ev(false)]);
        }
    }

    fn key(&mut self, name: &str) {
        let key = match name {
            "esc" => egui::Key::Escape,
            "enter" => egui::Key::Enter,
            "tab" => egui::Key::Tab,
            _ => return,
        };
        let ev = |pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        self.inject.push_back(vec![ev(true)]);
        self.inject.push_back(vec![ev(false)]);
    }
}

fn sample_verdict(mal: u32) -> crate::core::models::VtVerdict {
    crate::core::models::VtVerdict {
        sha256: "ab".repeat(32),
        malicious: mal,
        suspicious: 0,
        undetected: 68,
        harmless: 0,
        timeout: 0,
        scanned_by_upload: false,
        retrieved_at: String::new(),
    }
}

impl DevShots {
    pub fn from_env() -> Option<Self> {
        let dir = std::env::var_os("GIRUST_SHOTS")?;
        Some(Self {
            dir: PathBuf::from(dir),
            pending: None,
            last_poll: 0.0,
            inject: Default::default(),
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
                        // Keyboard-focus probe: viewport focus + focused id.
                        "focusinfo" => {
                            let info = format!(
                                "window_focused={} focused_id={:?}\n",
                                ctx.input(|i| i.focused),
                                ctx.memory(|m| m.focused())
                            );
                            use std::io::Write;
                            if let Ok(mut f) = std::fs::OpenOptions::new()
                                .create(true)
                                .append(true)
                                .open(shots.dir.join("focus.txt"))
                            {
                                let _ = f.write_all(info.as_bytes());
                            }
                        }
                        "mouse" => shots.pointer(arg, None),
                        "click" => shots.pointer(arg, Some(egui::PointerButton::Primary)),
                        "rclick" => shots.pointer(arg, Some(egui::PointerButton::Secondary)),
                        "key" => shots.key(arg),
                        "gate" => {
                            use crate::ui::gate::{ConfirmKind, GatePhase};
                            self.gate_asset_name = "ripgrep-14.1.1-x86_64-pc-windows-msvc.zip".into();
                            // No temp file in demo phases: Cancel/confirm must
                            // never touch real files from here.
                            self.gate_temp_path = None;
                            self.gate = match arg {
                                "downloading" => GatePhase::Downloading { downloaded: 1_200_000, total: 2_345_678 },
                                "scanning" => GatePhase::Scanning,
                                "flagged" => GatePhase::AwaitingConfirm(ConfirmKind::Flagged {
                                    verdict: sample_verdict(4),
                                    top_engines: vec!["EngineA".into(), "EngineB".into(), "EngineC".into()],
                                }),
                                "flagged-noengines" => GatePhase::AwaitingConfirm(ConfirmKind::Flagged {
                                    verdict: sample_verdict(1),
                                    top_engines: vec![],
                                }),
                                "unverified" => GatePhase::AwaitingConfirm(ConfirmKind::Unverified {
                                    reason: "VirusTotal API key missing or rejected".into(),
                                }),
                                "done" => GatePhase::Done {
                                    path: "downloads/ripgrep-14.1.1-x86_64-pc-windows-msvc.zip".into(),
                                    sha256: "ab".repeat(32),
                                },
                                "failed" => GatePhase::Failed("network: connection reset".into()),
                                _ => GatePhase::Idle,
                            };
                        }
                        "category" => {
                            let idx: usize = arg.parse().unwrap_or(0);
                            if let Some(r) = self.state.repos.get(idx) {
                                self.category_dialog_for = Some(r.id.clone());
                                self.category_draft = r.category.clone().unwrap_or_default();
                            }
                        }
                        "rescan" => {
                            use crate::net::vt::{LookupOutcome as LO, VtError};
                            let outcome = match arg {
                                "flagged" => LO::Report(sample_verdict(3), vec!["EngineA".into()]),
                                "none" => LO::NoVerdict(VtError::Quota),
                                _ => LO::Report(sample_verdict(0), vec![]),
                            };
                            self.rescan_result = Some(crate::ui::gate::rescan_summary(
                                "ripgrep-14.1.1-x86_64-pc-windows-msvc.zip",
                                &"ab".repeat(32),
                                &outcome,
                            ));
                        }
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
