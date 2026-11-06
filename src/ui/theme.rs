//! egui theme from the design system (Telemetry Room tokens).
//!
//! Sources: `design-system/giruststatus/MASTER.md` + accessibility
//! overrides in `.impeccable/surfaces/src-main-rs.md` (user is 45+,
//! eyeglasses, 1080p: base 16px, mono data 14-15px, nothing < 12px,
//! muted #A8B5C7, 64px rows, >= 44px hit targets, 2px focus ring).

use crate::core::config::ThemeChoice;
use egui::{Color32, Context, Visuals};


pub struct Tokens;

impl Tokens {
    pub const BG: Color32 = Color32::from_rgb(0x0F, 0x17, 0x2A); // #0F172A
    pub const RAIL: Color32 = Color32::from_rgb(0x0B, 0x11, 0x20); // darker rail
    pub const CARD: Color32 = Color32::from_rgb(0x1B, 0x23, 0x36); // #1B2336
    pub const CARD_NEW: Color32 = Color32::from_rgb(0x1D, 0x2A, 0x42); // green-tinted card
    pub const MUTED_BG: Color32 = Color32::from_rgb(0x27, 0x2F, 0x42); // #272F42
    pub const BORDER: Color32 = Color32::from_rgb(0x47, 0x56, 0x69); // #475569
    #[allow(dead_code)]
    pub const PRIMARY: Color32 = Color32::from_rgb(0x1E, 0x29, 0x3B); // #1E293B
    pub const TEXT: Color32 = Color32::from_rgb(0xF8, 0xFA, 0xFC); // #F8FAFC
    /// Accessibility-raised muted text (MASTER's #94A3B8 is too dim at small sizes).
    pub const MUTED: Color32 = Color32::from_rgb(0xA8, 0xB5, 0xC7);
    pub const ACCENT: Color32 = Color32::from_rgb(0x22, 0xC5, 0x5E); // #22C55E signal green
    pub const ON_ACCENT: Color32 = Color32::from_rgb(0x0F, 0x17, 0x2A);
    pub const DESTRUCTIVE: Color32 = Color32::from_rgb(0xEF, 0x44, 0x44); // #EF4444
    /// Control outlines: BORDER was too faint on CARD for empty checkboxes
    /// and text fields.
    pub const BORDER_STRONG: Color32 = Color32::from_rgb(0x64, 0x74, 0x8B); // #64748B
    /// Hover fill: clearly lighter than CARD.
    pub const HOVER: Color32 = Color32::from_rgb(0x33, 0x41, 0x55); // #334155
    /// Selection fill: deep green, TEXT on it is ~10:1 contrast.
    pub const SELECTED: Color32 = Color32::from_rgb(0x14, 0x53, 0x2D); // #14532D
    pub const AMBER:Color32 = Color32::from_rgb(0xFA, 0xCC, 0x15); // #FACC15 scanning/unverified

    /// Category ribbon palette, 8 slots (Task 8 assigns by category).
    pub const RIBBONS: [Color32; 8] = [
        Color32::from_rgb(0x3B, 0x82, 0xF6), // blue
        Color32::from_rgb(0x8B, 0x5C, 0xF6), // violet
        Color32::from_rgb(0xEC, 0x48, 0x99), // pink
        Color32::from_rgb(0xF9, 0x73, 0x16), // orange
        Color32::from_rgb(0x14, 0xB8, 0xA6), // teal
        Color32::from_rgb(0xEA, 0xB3, 0x08), // yellow
        Color32::from_rgb(0xF4, 0x3F, 0x5E), // rose
        Color32::from_rgb(0x84, 0xCC, 0x16), // lime
    ];

    pub const BODY: f32 = 16.0;
    pub const DATA: f32 = 14.0; // monospace data floor
    pub const SMALL: f32 = 12.0; // hard floor: pill labels only
}

/// Apply the dark theme. `choice` kept for the settings UI; dark-only for
/// now (design system: light not recommended).
pub fn apply(ctx: &Context, _choice: ThemeChoice) {
    let mut visuals = Visuals::dark();
    visuals.panel_fill = Tokens::BG;
    visuals.window_fill = Tokens::CARD;
    visuals.extreme_bg_color = Tokens::RAIL;
    visuals.faint_bg_color = Tokens::MUTED_BG;
    visuals.widgets.noninteractive.bg_fill = Tokens::MUTED_BG;
    visuals.widgets.noninteractive.fg_stroke.color = Tokens::TEXT;
    visuals.widgets.noninteractive.bg_stroke.color = Tokens::BORDER;
    visuals.widgets.noninteractive.bg_stroke.width = 1.0; // hairline seams
    // bg_fill: checkbox box, slider rail. weak_bg_fill: buttons, combos.
    // Both unset before → egui's default grey buttons and checkboxes
    // invisible on CARD.
    visuals.widgets.inactive.bg_fill = Tokens::MUTED_BG;
    visuals.widgets.inactive.weak_bg_fill = Tokens::MUTED_BG;
    visuals.widgets.inactive.fg_stroke.color = Tokens::TEXT;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, Tokens::BORDER_STRONG);
    for w in [
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        // 4px: at 6px the 16px checkbox read as a radio button.
        w.rounding = egui::Rounding::same(4.0);
    }
    // Hover must be visibly distinct from CARD (PRIMARY was ~identical).
    visuals.widgets.hovered.bg_fill = Tokens::HOVER;
    visuals.widgets.hovered.weak_bg_fill = Tokens::HOVER;
    visuals.widgets.hovered.fg_stroke.color = Tokens::TEXT;
    visuals.widgets.hovered.bg_stroke.color = Tokens::MUTED;
    visuals.widgets.active.bg_fill = Tokens::SELECTED;
    visuals.widgets.active.weak_bg_fill = Tokens::SELECTED;
    visuals.widgets.active.fg_stroke.color = Tokens::TEXT;
    visuals.widgets.active.bg_stroke.color = Tokens::ACCENT;
    // Selection: deep green fill + bright text. Bright-green fill behind
    // light text (old) was unreadable in rail, combos and text selection.
    visuals.selection.bg_fill = Tokens::SELECTED;
    visuals.selection.stroke.color = Tokens::TEXT;
    visuals.widgets.open.bg_fill = Tokens::HOVER;
    visuals.widgets.open.weak_bg_fill = Tokens::HOVER;
    visuals.widgets.open.fg_stroke.color = Tokens::TEXT;
    // 2px text cursor (user accessibility requirement).
    visuals.text_cursor.stroke.width = 2.0;
    visuals.text_cursor.stroke.color = Tokens::ACCENT;
    visuals.window_stroke = egui::Stroke::new(1.0_f32, Tokens::BORDER);
    visuals.window_rounding = egui::Rounding::same(10.0);
    visuals.hyperlink_color = Tokens::ACCENT;
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0); // --space-sm
    style.spacing.button_padding = egui::vec2(16.0, 10.0); // >= 44px hit height
    style.spacing.menu_margin = egui::Margin::same(8.0);
    style.spacing.window_margin = egui::Margin::same(16.0);
    // One control height everywhere (was 36-48px mix in the top bar);
    // 44px = accessibility hit-target floor.
    style.spacing.interact_size.y = 44.0;
    style.spacing.slider_width = 220.0;
    // Type scale: 16px body, 12px small floor (accessibility).
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::proportional(Tokens::BODY),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::proportional(Tokens::BODY),
    );
    style.text_styles.insert(
        egui::TextStyle::Small,
        egui::FontId::proportional(Tokens::SMALL),
    );
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::proportional(Tokens::BODY + 6.0),
    );
    ctx.set_style(style);
}
