#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

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
use ui::list::SortKey;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([960.0, 640.0]),
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

struct App {
    state: AppState,
    config: Config,
    state_error: Option<String>,
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
        let config = Config::load(&crate::core::paths::config_file()).unwrap_or_default();
        let (cmd_tx, cmd_rx) = tokio::sync::mpsc::unbounded_channel();
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let refresh_in_flight = Arc::new(AtomicBool::new(false));
        net::bus::spawn(cmd_rx, event_tx);
        Self {
            state,
            config,
            state_error,
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
                ui.add_enabled_ui(true, |ui| {
                    ui.label(
                        egui::RichText::new("GiRustStatus")
                            .size(ui::theme::Tokens::BODY)
                            .strong(),
                    );
                });
                ui.add_space(16.0);
                // Single view for now (Repos); nav lands in Task 9.
                let active = egui::RichText::new("Repos")
                    .size(ui::theme::Tokens::BODY)
                    .color(ui::theme::Tokens::TEXT);
                let _ = ui.selectable_label(true, active);
                for item in ["Logs", "Rates", "Stats", "Settings"] {
                    let dim = egui::RichText::new(item)
                        .size(ui::theme::Tokens::BODY)
                        .color(ui::theme::Tokens::MUTED);
                    let _ = ui.selectable_label(false, dim);
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

                if self.selected_repo.is_some() {
                    self.show_detail(ui);
                } else {
                    self.show_list(ui);
                }
            });

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
                    } else {
                        self.state.repos.push(repo);
                    }
                }
                Event::AddResult(res) => match res {
                    Ok(repo) => {
                        self.log(format!("added: {}", repo.display_name()));
                        self.state.repos.push(repo);
                        self.show_add_dialog = false;
                        self.add_url.clear();
                        self.add_error = None;
                    }
                    Err(e) => {
                        self.add_error = Some(e);
                    }
                },
                Event::DownloadProgress { repo_id: _, asset_name: _, downloaded, total } => {
                    let _ = (downloaded, total); // Task 7 renders progress
                }
                Event::VtVerdictReady { repo_id: _, asset_name: _, sha256: _, outcome: _ } => {
                    // Task 7 wires the gate UI.
                }
                Event::FileReady { repo_id: _, asset_name: _, path: _, sha256: _, outcome: _ } => {
                    // Task 7 wires the gate UI.
                }
                Event::LogLine(line) => self.log(line),
                Event::RateLimitsUpdated(_rl) => {
                    // Task 9 renders the rate panel.
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
                            row_response.on_hover_cursor(egui::CursorIcon::PointingHand);
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
        if let Some(asset) = ui::detail::show(ui, &repo) {
            self.send(Command::DownloadAsset {
                repo_id: repo.id.clone(),
                asset,
                config: self.config.clone(),
            });
            self.log("download requested");
        }
    }
}
