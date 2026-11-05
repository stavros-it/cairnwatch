#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! App root for the binary target. Module tree mirrors the lib.

mod core;
mod net;
mod ui;

use crate::core::config::Config;
use crate::core::models::Repo;
use crate::core::state::AppState;
use crate::net::bus::{Command, Event};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use ui::gate::GatePhase;
use ui::list::SortKey;

/// Which view the rail shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    Repos,
    Logs,
    Rates,
    Stats,
    Settings,
}

/// Import flow stages (Task 10).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
enum ImportStage {
    #[default]
    None,
    ConfirmReplace {
        path: std::path::PathBuf,
        encrypted: bool,
    },
    Failed(String),
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([960.0, 640.0])
            .with_icon(load_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "GiRustStatus",
        options,
        Box::new(|cc| {
            ui::theme::apply(&cc.egui_ctx, Config::default().theme);
            Ok(Box::new(App::new()))
        }),
    )
}

/// Load the embedded window icon (include_bytes keeps the exe portable —
/// no asset file needed next to the exe).
fn load_icon() -> egui::IconData {
    let png = include_bytes!("../assets/icon.png");
    let img = image::load_from_memory(png).expect("icon png").to_rgba8();
    let (w, h) = img.dimensions();
    egui::IconData {
        width: w,
        height: h,
        rgba: img.into_raw(),
    }
}

struct App {
    state: AppState,
    config: Config,
    state_error: Option<String>,
    config_error: Option<String>,
    // Bus
    cmd_tx: tokio::sync::mpsc::UnboundedSender<Command>,
    event_rx: tokio::sync::mpsc::UnboundedReceiver<Event>,
    refresh_in_flight: Arc<AtomicBool>,
    // UI state
    selected_repo: Option<String>,
    search: String,
    sort_key: SortKey,
    show_add_dialog: bool,
    add_url: String,
    add_error: Option<String>,
    filter_new_only: bool,
    log_ring: VecDeque<String>,
    // Task 8: context-menu driven state.
    category_dialog_for: Option<String>,
    category_draft: String,
    // Task 9: nav + diagnostics state.
    view: View,
    live_rate_limits: crate::net::ratelimits::RateLimits,
    // Task 10: import/export dialogs.
    export_password_dialog: bool,
    export_password: String,
    import_stage: ImportStage,
    import_password: String,
    // VT gate (Task 7): exactly one download at a time.
    gate: GatePhase,
    gate_asset_name: String,
    gate_repo_id: String,
    /// Real temp path of the gated file (FileReady carries it; dialogs use
    /// it instead of reconstructing from the asset name — review fix).
    gate_temp_path: Option<std::path::PathBuf>,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    fn new() -> Self {
        let (state, state_error) = match AppState::load(&crate::core::paths::repos_file()) {
            Ok(s) => (s, None),
            Err(e) => (AppState::default(), Some(e)),
        };
        let (config, config_error) = match Config::load(&crate::core::paths::config_file()) {
            Ok(c) => (c, None),
            // Missing file: normal first run, defaults, no banner.
            // Corrupt file: keep defaults in memory but DON'T write them
            // over the file until the user changes something intentionally
            // (review: silent reset loses VT key/PATs).
            Err(crate::core::config::LoadError::Missing) => (Config::default(), None),
            Err(crate::core::config::LoadError::Corrupt(e)) => (Config::default(), Some(e)),
        };
        let (cmd_tx, cmd_rx) = tokio::sync::mpsc::unbounded_channel();
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let refresh_in_flight = Arc::new(AtomicBool::new(false));
        net::bus::spawn(cmd_rx, event_tx);
        Self {
            state,
            config,
            state_error,
            config_error,
            cmd_tx,
            event_rx,
            refresh_in_flight,
            selected_repo: None,
            search: String::new(),
            sort_key: SortKey::Name,
            show_add_dialog: false,
            add_url: String::new(),
            add_error: None,
            filter_new_only: false,
            log_ring: VecDeque::with_capacity(500),
            category_dialog_for: None,
            category_draft: String::new(),
            view: View::Repos,
            live_rate_limits: Default::default(),
            export_password_dialog: false,
            export_password: String::new(),
            import_stage: ImportStage::None,
            import_password: String::new(),
            gate: GatePhase::Idle,
            gate_asset_name: String::new(),
            gate_repo_id: String::new(),
            gate_temp_path: None,
        }
    }

    fn log(&mut self, line: impl Into<String>) {
        self.log_ring.push_back(line.into());
        while self.log_ring.len() > 500 {
            self.log_ring.pop_front();
        }
    }

    fn send(&self, cmd: Command) {
        let _ = self.cmd_tx.send(cmd);
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events();

        // Left rail nav.
        egui::SidePanel::left("rail")
            .frame(egui::Frame::none().fill(ui::theme::Tokens::RAIL))
            .resizable(false)
            .show(ctx, |ui| {
                ui.add_space(16.0);
                ui.label(
                    egui::RichText::new("GiRustStatus")
                        .size(ui::theme::Tokens::BODY)
                        .strong(),
                );
                ui.add_space(16.0);
                for (item, view) in [
                    ("Repos", View::Repos),
                    ("Logs", View::Logs),
                    ("Rates", View::Rates),
                    ("Stats", View::Stats),
                    ("Settings", View::Settings),
                ] {
                    let active = self.view == view;
                    let label = egui::RichText::new(item)
                        .size(ui::theme::Tokens::BODY)
                        .color(if active {
                            ui::theme::Tokens::TEXT
                        } else {
                            ui::theme::Tokens::MUTED
                        });
                    let resp = ui.selectable_label(active, label);
                    if resp.clicked() {
                        self.view = view;
                    }
                }
            });

        // Top bar + list, or detail.
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(ui::theme::Tokens::BG))
            .show(ctx, |ui| {
                // Corrupt-state banner.
                if let Some(err) = &self.state_error {
                    egui::Frame::none()
                        .fill(ui::theme::Tokens::DESTRUCTIVE.gamma_multiply(0.15))
                        .inner_margin(egui::Margin::same(8.0))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(format!("State file error: {err}"))
                                    .color(ui::theme::Tokens::TEXT),
                            );
                        });
                }
                // Corrupt-config banner (never silently reset — review fix).
                if let Some(err) = &self.config_error {
                    egui::Frame::none()
                        .fill(ui::theme::Tokens::AMBER.gamma_multiply(0.15))
                        .inner_margin(egui::Margin::same(8.0))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "config.toml could not be read ({err}). Defaults shown; SAVING IN SETTINGS WILL OVERWRITE THE FILE — back it up first."
                                ))
                                .color(ui::theme::Tokens::TEXT),
                            );
                        });
                }

                match self.view {
                    View::Repos => {
                        if self.selected_repo.is_some() {
                            self.show_detail(ui);
                        } else {
                            self.show_list(ui);
                        }
                    }
                    View::Logs => {
                        let export = ui::logs::show(ui, &self.log_ring);
                        if export {
                            let path = crate::core::paths::app_dir().join("girust-log.txt");
                            let all: Vec<&str> = self.log_ring.iter().map(|s| s.as_str()).collect();
                            match std::fs::write(&path, all.join("\n")) {
                                Ok(_) => self.log(format!("log exported: {}", path.display())),
                                Err(e) => self.log(format!("log export failed: {e}")),
                            }
                        }
                    }
                    View::Rates => {
                        ui::rates::show(ui, &self.live_rate_limits);
                    }
                    View::Stats => {
                        let new_count = self.state.repos.iter().filter(|r| r.new_release).count();
                        ui::rates::show_stats(ui, &self.state.stats, self.state.repos.len(), new_count);
                    }
                    View::Settings => {
                        let before = self.config.clone();
                        let action = ui::settings::show(ui, &mut self.config);
                        // Persist config ONLY when it actually changed (not
                        // every frame — review fix).
                        if self.config != before {
                            if let Err(e) =
                                self.config.save(&crate::core::paths::config_file())
                            {
                                self.log(format!("config save failed: {e}"));
                            }
                        }
                        if action.export_plain {
                            let path = crate::core::paths::app_dir()
                                .join("girust-export.json");
                            let bytes = crate::core::io::export(
                                &self.state,
                                &self.config,
                                None,
                            );
                            match std::fs::write(&path, bytes) {
                                Ok(_) => self.log(format!("exported: {}", path.display())),
                                Err(e) => self.log(format!("export failed: {e}")),
                            }
                        }
                        if action.export_encrypted {
                            self.export_password_dialog = true;
                        }
                        if action.import_file {
                            // Accept both export filenames (review fix).
                            let enc = crate::core::paths::app_dir().join("girust-export.enc");
                            let plain =
                                crate::core::paths::app_dir().join("girust-export.json");
                            let path = if enc.exists() {
                                enc
                            } else {
                                plain
                            };
                            self.import_stage = ImportStage::ConfirmReplace { path, encrypted: false };
                        }
                    }
                }
            });

        // VT gate dialog (Task 7).
        self.show_gate(ctx);

        // Task 10 dialogs: export password, import confirm.
        self.show_io_dialogs(ctx);

        // Keep repainting while a refresh/download is in flight.
        if self.refresh_in_flight.load(Ordering::Relaxed) {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
    }
}

impl App {
    fn drain_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                Event::RepoUpdated(repo) => {
                    self.log(format!("repo updated: {}", repo.display_name()));
                    if let Some(existing) = self.state.repos.iter_mut().find(|r| r.id == repo.id) {
                        *existing = repo;
                    }
                    // Repos removed mid-refresh are NOT re-added (review).
                }
                Event::AddResult(res) => match res {
                    Ok(repo) => {
                        self.log(format!("added: {}", repo.display_name()));
                        self.state.repos.push(repo);
                        self.show_add_dialog = false;
                        self.add_url.clear();
                        self.add_error = None;
                        self.save_state();
                    }
                    Err(e) => {
                        self.add_error = Some(e);
                    }
                },
                Event::DownloadProgress { repo_id: _, asset_name: _, downloaded, total } => {
                    if matches!(self.gate, GatePhase::Downloading { .. }) {
                        self.gate = GatePhase::Downloading { downloaded, total };
                        // Keep the UI live during transfer.
                    }
                }
                Event::VtVerdictReady { repo_id, asset_name, sha256, outcome } => {
                    let (phase, cached) = ui::gate::phase_after_scan(
                        &outcome,
                        &sha256,
                        std::path::PathBuf::new(), // rescan has no temp file
                    );
                    if let Some(v) = cached {
                        self.state.vt_cache.insert(sha256.clone(), v.clone());
                        self.state.stats.vt_scans += 1;
                        if v.is_flagged() {
                            self.state.stats.vt_flagged_seen += 1;
                        }
                    }
                    match phase {
                        GatePhase::Clean { .. } | GatePhase::AwaitingConfirm(_) => {
                            // Re-scan of a previously downloaded asset: show
                            // the verdict as a banner-style gate phase.
                            self.gate = phase;
                            self.gate_asset_name = asset_name;
                            self.gate_repo_id = repo_id;
                        }
                        _ => {}
                    }
                }
                Event::FileReady { repo_id, asset_name, path, sha256, outcome } => {
                    // Record asset → sha for the detail-view VT badge.
                    self.state
                        .asset_shas
                        .insert(format!("{repo_id}/{asset_name}"), sha256.clone());
                    // If the user cancelled or another flow took the gate,
                    // discard this file — no delivery after cancel (review
                    // CRITICAL #2).
                    if !matches!(self.gate, GatePhase::Downloading { .. } | GatePhase::Scanning) {
                        ui::gate::discard(&path);
                        self.log("discarded stale download (gate moved on)");
                        continue;
                    }
                    let (phase, cached) = ui::gate::phase_after_scan(&outcome, &sha256, path.clone());
                    if let Some(v) = cached {
                        self.state.vt_cache.insert(sha256.clone(), v.clone());
                        self.state.stats.vt_scans += 1;
                        if v.is_flagged() {
                            self.state.stats.vt_flagged_seen += 1;
                        }
                    }
                    match &phase {
                        GatePhase::Clean { path, sha256 } => {
                            // Clean: deliver immediately (spec).
                            let dl = self.config.download_dir.clone();
                            match ui::gate::deliver(path, &dl) {
                                Ok(dest) => {
                                    self.state.stats.downloads_completed += 1;
                                    self.gate = GatePhase::Done { path: dest.clone(), sha256: sha256.clone() };
                                    self.log(format!("clean → delivered: {}", dest.display()));
                                }
                                Err(e) => {
                                    self.gate = GatePhase::Failed(e);
                                }
                            }
                        }
                        GatePhase::AwaitingConfirm(_) => {
                            // Carry the REAL temp path so Cancel/Confirm
                            // operate on the file the engine wrote.
                            self.gate_temp_path = Some(path);
                            self.gate = phase;
                            self.gate_asset_name = asset_name;
                            self.gate_repo_id = repo_id;
                        }
                        _ => self.gate = phase,
                    }
                    self.save_state();
                }
                Event::LogLine(line) => self.log(line),
                Event::ScanStarted { repo_id: _, asset_name: _ } => {
                    if matches!(self.gate, GatePhase::Downloading { .. }) {
                        self.gate = GatePhase::Scanning;
                    }
                }
                Event::DownloadFailed { repo_id: _, reason } => {
                    self.gate = GatePhase::Failed(reason);
                }
                Event::RateLimitsUpdated(rl) => {
                    self.live_rate_limits = rl;
                }
                Event::RefreshDone => {
                    self.refresh_in_flight.store(false, Ordering::Relaxed);
                    self.log("refresh done");
                    self.save_state();
                }
            }
        }
    }

    fn save_state(&mut self) {
        if let Err(e) = self.state.save_atomic(&crate::core::paths::repos_file()) {
            self.state_error = Some(e);
        }
    }

    fn show_list(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            // Search box.
            let search = ui.add_sized(
                [360.0, 36.0],
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("Search repos, owners, categories…"),
            );
            let _ = search;
            // Refresh.
            let busy = self.refresh_in_flight.load(Ordering::Relaxed);
            let label = if busy { "Refreshing…" } else { "Refresh" };
            let refresh_btn = egui::Button::new(
                egui::RichText::new(label)
                    .size(ui::theme::Tokens::BODY)
                    .color(if busy {
                        ui::theme::Tokens::MUTED
                    } else {
                        ui::theme::Tokens::ON_ACCENT
                    }),
            )
            .fill(if busy {
                ui::theme::Tokens::MUTED_BG
            } else {
                ui::theme::Tokens::ACCENT
            });
            if ui.add_enabled(!busy, refresh_btn).clicked() {
                self.refresh_in_flight.store(true, Ordering::Relaxed);
                self.send(Command::RefreshAll {
                    repos: self.state.repos.clone(),
                    config: self.config.clone(),
                });
            }
            // N new chip.
            let new_count = self.state.repos.iter().filter(|r| r.new_release).count();
            if new_count > 0 {
                let chip = egui::Button::new(
                    egui::RichText::new(format!("{new_count} new"))
                        .size(ui::theme::Tokens::SMALL + 2.0)
                        .color(ui::theme::Tokens::ACCENT),
                )
                .fill(ui::theme::Tokens::CARD_NEW);
                if ui.add(chip).clicked() {
                    self.filter_new_only = !self.filter_new_only;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Sort dropdown.
                egui::ComboBox::from_id_salt("sort")
                    .width(140.0)
                    .selected_text(match self.sort_key {
                        SortKey::Name => "Name",
                        SortKey::NewestRelease => "Newest release",
                        SortKey::LastChecked => "Last checked",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.sort_key, SortKey::Name, "Name");
                        ui.selectable_value(&mut self.sort_key, SortKey::NewestRelease, "Newest release");
                        ui.selectable_value(&mut self.sort_key, SortKey::LastChecked, "Last checked");
                    });
                if ui.button("+ Add repo").clicked() {
                    self.show_add_dialog = true;
                    self.add_error = None;
                }
            });
        });
        ui.add_space(8.0);

        // Rows.
        let mut clicked_repo: Option<String> = None;
        let mut clicked_pin: Option<String> = None;
        let mut clicked_remove: Option<String> = None;
        let mut cat_dialog_open: Option<String> = None;
        let mut cat_draft = String::new();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut sorted: Vec<&Repo> = self
                    .state
                    .repos
                    .iter()
                    .filter(|r| {
                        if self.filter_new_only {
                            r.new_release
                        } else {
                            true
                        }
                    })
                    .filter(|r| ui::list::matches_query(r, &self.search))
                    .collect();
                sorted.sort_by(|a, b| ui::list::compare(a, b, self.sort_key));
                if sorted.is_empty() {
                    ui.add_space(32.0);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            egui::RichText::new(if self.state.repos.is_empty() {
                                "No repos yet — click “+ Add repo” to paste a GitHub or GitLab URL"
                            } else {
                                "No repos match the current filter"
                            })
                            .size(ui::theme::Tokens::BODY)
                            .color(ui::theme::Tokens::MUTED),
                        );
                        if !self.state.repos.is_empty() {
                            ui.label(
                                egui::RichText::new(
                                    "Tip: right-click a repo for pin, category and remove",
                                )
                                .size(ui::theme::Tokens::SMALL)
                                .color(ui::theme::Tokens::MUTED),
                            );
                        }
                    });
                }
                for repo in sorted {
                    let id = egui::Id::new(&repo.id);
                    let is_new = repo.new_release;
                    let fill = if is_new {
                        ui::theme::Tokens::CARD_NEW
                    } else {
                        ui::theme::Tokens::CARD
                    };
                    egui::Frame::none()
                        .fill(fill)
                        .rounding(8.0)
                        .inner_margin(egui::Margin::symmetric(16.0, 12.0))
                        .outer_margin(egui::Margin {
                            left: 12.0,
                            right: 12.0,
                            top: 4.0,
                            bottom: 4.0,
                        })
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                // Leading strip: green when NEW.
                                let (strip_rect, _) = ui.allocate_exact_size(
                                    egui::vec2(3.0, 44.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().rect_filled(
                                    strip_rect,
                                    1.5,
                                    if is_new {
                                        ui::theme::Tokens::ACCENT
                                    } else {
                                        ui::theme::Tokens::BORDER
                                    },
                                );
                                ui.add_space(8.0);
                                // Host glyph.
                                let host_glyph = match &repo.host {
                                    crate::core::models::RepoHost::GitHub => "GH",
                                    crate::core::models::RepoHost::GitLab { .. } => "GL",
                                };
                                egui::Frame::none()
                                    .fill(ui::theme::Tokens::MUTED_BG)
                                    .rounding(6.0)
                                    .inner_margin(egui::Margin::symmetric(8.0, 8.0))
                                    .show(ui, |ui| {
                                        ui.label(
                                            egui::RichText::new(host_glyph)
                                                .size(ui::theme::Tokens::SMALL + 1.0)
                                                .color(ui::theme::Tokens::MUTED),
                                        );
                                    });
                                ui.add_space(8.0);
                                // Name + owner (clickable → detail).
                                ui.vertical(|ui| {
                                    ui.label(
                                        egui::RichText::new(&repo.name)
                                            .size(ui::theme::Tokens::BODY)
                                            .strong(),
                                    );
                                    ui.label(
                                        egui::RichText::new(&repo.owner)
                                            .size(ui::theme::Tokens::SMALL + 2.0)
                                            .color(ui::theme::Tokens::MUTED),
                                    );
                                });
                                // Tag + date.
                                if let Some(rel) = &repo.latest {
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        // Trailing ribbon.
                                        let ribbon_color = ui::list::ribbon_color(repo.category.as_deref());
                                        let (r_rect, _) = ui.allocate_exact_size(
                                            egui::vec2(3.0, 44.0),
                                            egui::Sense::hover(),
                                        );
                                        ui.painter().rect_filled(r_rect, 1.5, ribbon_color);
                                        ui.add_space(8.0);
                                        ui::widgets::status_pill(ui, &repo.status);
                                        ui::widgets::version_pill(ui, &rel.tag, is_new);
                                        ui.label(
                                            egui::RichText::new(&rel.published_at)
                                                .size(ui::theme::Tokens::DATA)
                                                .color(ui::theme::Tokens::MUTED),
                                        );
                                    });
                                } else {
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui::widgets::status_pill(ui, &repo.status);
                                    });
                                }
                            });
                            // Row click target (whole row → detail).
                            let row_response =
                                ui.allocate_rect(ui.min_rect(), egui::Sense::click());
                            if row_response.clicked() {
                                clicked_repo = Some(repo.id.clone());
                            }
                            let row_hover = row_response.on_hover_cursor(egui::CursorIcon::PointingHand)
                                .on_hover_text("Click for details · right-click for pin, category, remove");
                            // Context menu: pin, category, remove.
                            row_hover.context_menu(|ui| {
                                let pin_label = if repo.pinned { "Unpin" } else { "Pin to top" };
                                if ui.button(pin_label).clicked() {
                                    clicked_pin = Some(repo.id.clone());
                                    ui.close_menu();
                                }
                                if ui.button("Set category…").clicked() {
                                    cat_dialog_open = Some(repo.id.clone());
                                    cat_draft = repo.category.clone().unwrap_or_default();
                                    ui.close_menu();
                                }
                                if !repo.pinned && ui.button("Remove repo").clicked() {
                                    clicked_remove = Some(repo.id.clone());
                                    ui.close_menu();
                                }
                            });
                        });
                    ui.add_space(2.0);
                    let _ = id;
                }
            });

        // Handle row click outside the borrow zone.
        if let Some(repo_id) = clicked_repo {
            self.selected_repo = Some(repo_id.clone());
            // Auto-clear NEW on open.
            if let Some(r) = self.state.repos.iter_mut().find(|r| r.id == repo_id) {
                if r.new_release {
                    r.new_release = false;
                    r.last_seen_tag = r.latest.as_ref().map(|l| l.tag.clone());
                }
            }
            self.save_state();
        }
        if let Some(repo_id) = clicked_pin {
            let log_line = if let Some(r) = self.state.repos.iter_mut().find(|r| r.id == repo_id) {
                r.pinned = !r.pinned;
                Some(format!("{} {}", if r.pinned { "pinned" } else { "unpinned" }, r.display_name()))
            } else {
                None
            };
            if let Some(l) = log_line {
                self.log(l);
            }
            self.save_state();
        }
        if let Some(repo_id) = clicked_remove {
            self.state.repos.retain(|r| r.id != repo_id);
            self.log("repo removed");
            self.save_state();
        }

        // Category assignment dialog (8-slot palette preview).
        if cat_dialog_open.is_some() {
            self.category_dialog_for = cat_dialog_open;
            self.category_draft = cat_draft;
        }
        if let Some(repo_id) = self.category_dialog_for.clone() {
            let mut open = true;
            egui::Window::new("Set category")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(&ctx, |ui| {
                    ui.label("Category name (color is derived):");
                    ui.add_sized(
                        [320.0, 32.0],
                        egui::TextEdit::singleline(&mut self.category_draft)
                            .hint_text("e.g. tools, media, dev"),
                    );
                    ui.add_space(8.0);
                    // Palette preview: 8 swatches.
                    ui.horizontal(|ui| {
                        for color in ui::theme::Tokens::RIBBONS {
                            let (rect, _) = ui.allocate_exact_size(
                                egui::vec2(24.0, 24.0),
                                egui::Sense::hover(),
                            );
                            ui.painter().rect_filled(rect, 4.0, color);
                        }
                    });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            let cat = self.category_draft.trim().to_string();
                            let log_line = if let Some(r) =
                                self.state.repos.iter_mut().find(|r| r.id == repo_id)
                            {
                                r.category = if cat.is_empty() { None } else { Some(cat.clone()) };
                                Some(format!("category set: {}", r.display_name()))
                            } else {
                                None
                            };
                            if let Some(l) = log_line {
                                self.log(l);
                            }
                            self.category_dialog_for = None;
                            self.save_state();
                        }
                        if ui.button("Cancel").clicked() {
                            self.category_dialog_for = None;
                        }
                    });
                });
            if !open {
                self.category_dialog_for = None;
            }
        }

        // Add-repo modal.
        if self.show_add_dialog {
            egui::Window::new("Add repository")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(&ctx, |ui| {
                    ui.label("Paste a GitHub or GitLab repo URL:");
                    ui.add_sized(
                        [420.0, 32.0],
                        egui::TextEdit::singleline(&mut self.add_url)
                            .hint_text("https://github.com/owner/repo"),
                    );
                    if let Some(err) = &self.add_error {
                        ui.label(
                            egui::RichText::new(err)
                                .color(ui::theme::Tokens::DESTRUCTIVE),
                        );
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Add").clicked() {
                            self.send(Command::AddRepo {
                                url: self.add_url.clone(),
                                config: self.config.clone(),
                            });
                        }
                        if ui.button("Cancel").clicked() {
                            self.show_add_dialog = false;
                            self.add_error = None;
                        }
                    });
                });
        }
    }

    fn show_detail(&mut self, ui: &mut egui::Ui) {        // Back button + repo detail.
        ui.horizontal(|ui| {
            if ui.button("← Back").clicked() {
                self.selected_repo = None;
            }
        });
        let Some(repo_id) = self.selected_repo.clone() else {
            return;
        };
        let Some(repo) = self.state.repos.iter().find(|r| r.id == repo_id).cloned() else {
            self.selected_repo = None;
            return;
        };
        // VT badge lookup closure: asset name → cached verdict info.
        let vt_info = |asset_name: &str| -> Option<ui::detail::AssetVtInfo> {
            let key = format!("{}/{}", repo_id, asset_name);
            let sha = self.state.asset_shas.get(&key)?;
            let v = self.state.vt_cache.get(sha)?;
            let (flagged, total) = v.flagged_ratio();
            Some(ui::detail::AssetVtInfo {
                label: format!("{flagged}/{total}"),
                flagged: flagged > 0,
                sha256: sha.clone(),
            })
        };
        let actions = ui::detail::show(ui, &repo, &vt_info);
        if actions.remove {
            // Don't remove mid-gate: the dialog would reference a dead repo.
            let gate_busy_for_this =
                matches!(self.gate, GatePhase::Downloading { .. } | GatePhase::Scanning)
                    && self.gate_repo_id == repo.id;
            if gate_busy_for_this {
                self.log("remove blocked: download/scan in progress for this repo");
            } else {
                let name = repo.display_name();
                self.state.repos.retain(|r| r.id != repo.id);
                self.selected_repo = None;
                self.log(format!("repo removed: {name}"));
                self.save_state();
            }
        }
        if let Some((asset_name, sha256)) = actions.rescan {
            self.send(Command::RescanAsset {
                repo_id: repo.id.clone(),
                asset_name,
                sha256,
                config: self.config.clone(),
            });
            self.log("re-scan requested");
        }
        if let Some(f) = actions.filters_changed {
            if let Some(r) = self.state.repos.iter_mut().find(|r| r.id == repo.id) {
                r.filters = f;
            }
            self.save_state();
            self.log("filters updated");
        }
        if let Some(asset) = actions.download {
            if !matches!(self.gate, GatePhase::Idle | GatePhase::Done { .. } | GatePhase::Cancelled | GatePhase::Failed(_)) {
                self.log("download blocked: another download is in the gate");
                return;
            }
            self.gate = GatePhase::Downloading { downloaded: 0, total: asset.size };
            self.gate_asset_name = asset.name.clone();
            self.gate_repo_id = repo.id.clone();
            self.state.stats.downloads_started += 1;
            self.send(Command::DownloadAsset {
                repo_id: repo.id.clone(),
                asset,
                config: self.config.clone(),
            });
            self.log(format!("download requested: {}", self.gate_asset_name));
        }
    }

    /// Import/export dialogs (Task 10).
    fn show_io_dialogs(&mut self, ctx: &egui::Context) {
        if self.export_password_dialog {
            let mut open = true;
            egui::Window::new("Encrypted export")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label("Set a password for the export file:");
                    ui.add_sized(
                        [320.0, 32.0],
                        egui::TextEdit::singleline(&mut self.export_password),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Export").clicked() && !self.export_password.is_empty() {
                            let path = crate::core::paths::app_dir().join("girust-export.enc");
                            let bytes = crate::core::io::export(
                                &self.state,
                                &self.config,
                                Some(&self.export_password),
                            );
                            match std::fs::write(&path, bytes) {
                                Ok(_) => self.log(format!("exported: {}", path.display())),
                                Err(e) => self.log(format!("export failed: {e}")),
                            }
                            self.export_password_dialog = false;
                            self.export_password.clear();
                        }
                        if ui.button("Cancel").clicked() {
                            self.export_password_dialog = false;
                            self.export_password.clear();
                        }
                    });
                });
            if !open {
                self.export_password_dialog = false;
                self.export_password.clear();
            }
        }

        match self.import_stage.clone() {
            ImportStage::None => {}
            ImportStage::ConfirmReplace { path, encrypted } => {
                let mut open = true;
                let detected = std::fs::read(&path)
                    .map(|b| b.starts_with(crate::core::io::MAGIC.as_bytes()))
                    .unwrap_or(encrypted);
                egui::Window::new("Import state")
                    .open(&mut open)
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label(format!("Import from: {}", path.display()));
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(
                                "This REPLACES your current repos, seen state, VT cache, stats and settings.",
                            )
                            .color(ui::theme::Tokens::AMBER),
                        );
                        ui.add_space(8.0);
                        if detected {
                            ui.label("The file is password-encrypted:");
                            ui.add_sized(
                                [320.0, 32.0],
                                egui::TextEdit::singleline(&mut self.import_password),
                            );
                        }
                        ui.horizontal(|ui| {
                            if ui.button("Import and replace").clicked() {
                                let bytes = match std::fs::read(&path) {
                                    Ok(b) => b,
                                    Err(e) => {
                                        self.import_stage =
                                            ImportStage::Failed(format!("read: {e}"));
                                        return;
                                    }
                                };
                                let pw = if detected && !self.import_password.is_empty() {
                                    Some(self.import_password.as_str())
                                } else {
                                    None
                                };
                                match crate::core::io::import(&bytes, pw) {
                                    Ok((state, config)) => {
                                        self.state = state;
                                        self.config = config;
                                        self.save_state();
                                        let _ =
                                            self.config.save(&crate::core::paths::config_file());
                                        self.log("import complete — state replaced");
                                        self.import_stage = ImportStage::None;
                                        self.import_password.clear();
                                    }
                                    Err(e) => {
                                        self.import_stage = ImportStage::Failed(e.to_string());
                                        self.import_password.clear();
                                    }
                                }
                            }
                            if ui.button("Cancel").clicked() {
                                self.import_stage = ImportStage::None;
                                self.import_password.clear();
                            }
                        });
                    });
                if !open {
                    self.import_stage = ImportStage::None;
                    self.import_password.clear();
                }
            }
            ImportStage::Failed(msg) => {
                let mut open = true;
                egui::Window::new("Import failed")
                    .open(&mut open)
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label(
                            egui::RichText::new(&msg)
                                .color(ui::theme::Tokens::DESTRUCTIVE),
                        );
                        if ui.button("Close").clicked() {
                            self.import_stage = ImportStage::None;
                        }
                    });
                if !open {
                    self.import_stage = ImportStage::None;
                }
            }
        }
    }

    /// The VT gate modal: progress, scanning, confirm dialogs, receipts.
    fn show_gate(&mut self, ctx: &egui::Context) {
        let phase = self.gate.clone();
        let asset_name = self.gate_asset_name.clone();
        match phase {
            GatePhase::Idle => {}
            GatePhase::Downloading { downloaded, total } => {
                egui::Window::new("Downloading")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label(&asset_name);
                        let pct = if total > 0 {
                            (downloaded as f32 / total as f32).clamp(0.0, 1.0)
                        } else {
                            0.0
                        };
                        let bar = egui::ProgressBar::new(pct)
                            .text(if total > 0 {
                                format!(
                                    "{} / {}",
                                    ui::detail::format_bytes(downloaded),
                                    ui::detail::format_bytes(total)
                                )
                            } else {
                                ui::detail::format_bytes(downloaded)
                            })
                            .desired_width(360.0);
                        ui.add(bar);
                        if ui.button("Cancel").clicked() {
                            self.send(Command::CancelDownload);
                            self.gate = GatePhase::Cancelled;
                            self.log("download cancelled by user");
                        }
                    });
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
            GatePhase::Scanning => {
                egui::Window::new("Scanning")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label(format!("Scanning {asset_name} on VirusTotal…"));
                        ui.add(egui::Spinner::new());
                    });
                ctx.request_repaint_after(std::time::Duration::from_millis(500));
            }
            GatePhase::AwaitingConfirm(kind) => {
                // Cancel is the default-focused button (spec). egui has no
                // focus-on-open API here; we render Cancel FIRST and larger,
                // and it owns the Response marked as default.
                egui::Window::new(match &kind {
                    ui::gate::ConfirmKind::Flagged { .. } => "⚠ Flagged file",
                    ui::gate::ConfirmKind::Unverified { .. } => "Not scanned",
                })
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    match &kind {
                        ui::gate::ConfirmKind::Flagged { verdict, top_engines } => {
                            let (flagged, total) = verdict.flagged_ratio();
                            ui.label(
                                egui::RichText::new(format!(
                                    "{flagged}/{total} engines flagged this file"
                                ))
                                .size(ui::theme::Tokens::BODY)
                                .color(ui::theme::Tokens::DESTRUCTIVE)
                                .strong(),
                            );
                            ui.label(&asset_name);
                            ui.add_space(8.0);
                            ui.label(format!("Flagged by: {}", top_engines.join(", ")));
                            ui.add_space(8.0);
                            let url = format!(
                                "https://www.virustotal.com/gui/file/{}",
                                verdict.sha256
                            );
                            if ui.button("Open VirusTotal report").clicked() {
                                let _ = crate::ui::detail::open_browser(&url);
                            }
                        }
                        ui::gate::ConfirmKind::Unverified { reason } => {
                            ui.label(
                                egui::RichText::new("Not scanned — proceed?")
                                    .size(ui::theme::Tokens::BODY)
                                    .color(ui::theme::Tokens::AMBER)
                                    .strong(),
                            );
                            ui.label(&asset_name);
                            ui.add_space(8.0);
                            ui.label(format!("Reason: {reason}"));
                        }
                    }
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        // Cancel first and visually primary (default-focused).
                        let cancel = egui::Button::new(
                            egui::RichText::new("Cancel")
                                .size(ui::theme::Tokens::BODY)
                                .color(ui::theme::Tokens::ON_ACCENT),
                        )
                        .fill(ui::theme::Tokens::ACCENT);
                        if ui.add(cancel).clicked() {
                            // Delete the temp file at its REAL path.
                            if let Some(tmp) = self.gate_temp_path.clone() {
                                ui::gate::discard(&tmp);
                            }
                            self.gate_temp_path = None;
                            self.gate = GatePhase::Cancelled;
                            self.log("gate: cancelled, temp deleted");
                        }
                        if ui.button("Download anyway").clicked() {
                            let tmp = self
                                .gate_temp_path
                                .clone()
                                .unwrap_or_else(|| crate::core::paths::tmp_dir().join(&asset_name));
                            let dl = self.config.download_dir.clone();
                            match ui::gate::deliver(&tmp, &dl) {
                                Ok(dest) => {
                                    let sha = match &kind {
                                        ui::gate::ConfirmKind::Flagged { verdict, .. } => {
                                            verdict.sha256.clone()
                                        }
                                        _ => String::new(),
                                    };
                                    self.state.stats.downloads_completed += 1;
                                    self.log(format!("gate: confirmed, delivered {}", dest.display()));
                                    self.gate = GatePhase::Done { path: dest, sha256: sha };
                                    self.gate_temp_path = None;
                                }
                                Err(e) => {
                                    self.gate = GatePhase::Failed(e);
                                }
                            }
                        }
                    });
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("Cancel is the default action")
                            .size(ui::theme::Tokens::SMALL)
                            .color(ui::theme::Tokens::MUTED),
                    );
                });
            }
            GatePhase::Clean { path, sha256 } => {
                // Should not linger — delivery happens in drain_events.
                let dl = self.config.download_dir.clone();
                match ui::gate::deliver(&path, &dl) {
                    Ok(dest) => {
                        self.state.stats.downloads_completed += 1;
                        self.log(format!("clean → delivered: {}", dest.display()));
                        self.gate = GatePhase::Done { path: dest, sha256 };
                    }
                    Err(e) => self.gate = GatePhase::Failed(e),
                }
            }
            GatePhase::Done { path, sha256 } => {
                egui::Window::new("Downloaded")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label(egui::RichText::new("Saved").strong());
                        ui.label(path.to_string_lossy().as_ref());
                        if !sha256.is_empty() {
                            ui.label(
                                egui::RichText::new(format!("sha256: {sha256}"))
                                    .size(ui::theme::Tokens::SMALL)
                                    .color(ui::theme::Tokens::MUTED),
                            );
                        }
                        if ui.button("Close").clicked() {
                            self.gate = GatePhase::Idle;
                        }
                    });
            }
            GatePhase::Cancelled => {
                self.gate = GatePhase::Idle;
            }
            GatePhase::Failed(e) => {
                egui::Window::new("Download failed")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label(
                            egui::RichText::new(&e).color(ui::theme::Tokens::DESTRUCTIVE),
                        );
                        if ui.button("Close").clicked() {
                            self.gate = GatePhase::Idle;
                        }
                    });
            }
        }
    }
}
