//! Log viewer: in-memory ring buffer (Task 6 stores lines), rendered here.

use crate::ui::theme::Tokens;
use egui::{RichText, ScrollArea, Ui};

/// Render the logs view. Returns an export request if the button was hit.
pub fn show(ui: &mut Ui, log_ring: &std::collections::VecDeque<String>) -> bool {
    let mut export_requested = false;
    ui.horizontal(|ui| {
        ui.heading(RichText::new("Logs").size(Tokens::BODY + 6.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Export to file…").clicked() {
                export_requested = true;
            }
            if ui.button("Copy all").clicked() {
                let all: Vec<&str> = log_ring.iter().map(|s| s.as_str()).collect();
                ui.ctx().copy_text(all.join("\n"));
            }
            ui.label(
                RichText::new(format!("{} lines (in-memory only)", log_ring.len()))
                    .size(Tokens::SMALL)
                    .color(Tokens::MUTED),
            );
        });
    });
    ui.add_space(8.0);

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            egui::Frame::none()
                .fill(Tokens::RAIL)
                .rounding(8.0)
                .inner_margin(egui::Margin::same(12.0))
                .show(ui, |ui| {
                    if log_ring.is_empty() {
                        ui.label(
                            RichText::new("No log lines yet — refresh or download something")
                                .size(Tokens::DATA)
                                .color(Tokens::MUTED),
                        );
                    }
                    for line in log_ring {
                        ui.label(
                            RichText::new(line)
                                .size(Tokens::SMALL + 2.0)
                                .color(Tokens::TEXT)
                                .family(egui::FontFamily::Monospace),
                        );
                    }
                });
        });
    export_requested
}
