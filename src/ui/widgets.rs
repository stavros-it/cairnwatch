//! Reusable pill/badge/ribbon widgets (Telemetry Room grammar).

use crate::ui::theme::Tokens;
use egui::{Color32, Sense, Ui};

/// A small rounded pill. `filled` paints the color; else muted outline.
pub fn pill(ui: &mut Ui, text: &str, color: Color32, filled: bool) {
    // Outline pills keep their semantic color (was always MUTED, so
    // OK/error/no-stable looked identical).
    let bg = if filled { color } else { Tokens::MUTED_BG };
    let fg = if filled { Tokens::ON_ACCENT } else { color };
    let stroke = if filled {
        egui::Stroke::NONE
    } else {
        egui::Stroke::new(1.0_f32, color.gamma_multiply(0.6))
    };
    // Painted, not a Frame: a Frame inside a 44px row stretched to the row
    // height and turned pills into tall ovals.
    let galley = ui.painter().layout_no_wrap(
        text.to_owned(),
        egui::FontId::proportional(Tokens::SMALL + 1.0),
        fg,
    );
    let size = galley.size() + egui::vec2(20.0, 8.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    painter.rect(rect, size.y / 2.0, bg, stroke);
    painter.galley(rect.center() - galley.size() / 2.0, galley, fg);
}

/// Version pill: filled green with NEW prefix when new, muted when seen.
pub fn version_pill(ui: &mut Ui, tag: &str, is_new: bool) {
    if is_new {
        // No arrow glyph: "↓" is missing from egui's default fonts (tofu).
        pill(ui, &format!("NEW · {tag}"), Tokens::ACCENT, true);
    } else {
        pill(ui, tag, Tokens::MUTED, false);
    }
}

/// Status pill: color from status semantics.
pub fn status_pill(ui: &mut Ui, status: &crate::core::models::RepoStatus) {
    use crate::core::models::RepoStatus as RS;
    let (text, color, filled) = match status {
        RS::Ok => ("OK".to_string(), Tokens::ACCENT, false),
        RS::NoStableRelease => ("no stable".into(), Tokens::AMBER, false),
        RS::NoReleases => ("no releases".into(), Tokens::MUTED, false),
        RS::RateLimited => ("rate limited".into(), Tokens::AMBER, true),
        RS::Error(_) => ("error".to_string(), Tokens::DESTRUCTIVE, false),
    };
    pill(ui, &text, color, filled);
}

/// Trailing category ribbon: 3px vertical strip.
#[allow(dead_code)] // consumed by Task 8 category ribbons
pub fn ribbon(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(3.0, ui.available_height()),
        Sense::hover(),
    );
    ui.painter().rect_filled(rect, 1.5, color);
}

/// VT verdict badge (Task 7 fills real data; Task 6 shows placeholder).
pub fn vt_badge(ui: &mut Ui, text: &str, color: Color32) {
    pill(ui, text, color, false);
}
