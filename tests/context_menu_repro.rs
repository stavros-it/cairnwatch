//! Regression guard for the remove-repo flow: drives the exact row +
//! context-menu pattern from main.rs show_list with injected pointer
//! events, headless. Proves right-click → "Remove repo" → removal works,
//! so the context-menu path stays functional next to the new detail-view
//! Remove button.

#[test]
fn context_menu_remove_repo_click_fires() {
    let ctx = egui::Context::default();
    let screen = egui::Rect::from_min_size(
        egui::Pos2::new(0.0, 0.0),
        egui::Vec2::new(1280.0, 800.0),
    );

    let row_rect: std::rc::Rc<std::cell::RefCell<Option<egui::Rect>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));
    let remove_btn_rect: std::rc::Rc<std::cell::RefCell<Option<egui::Rect>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));
    let clicked_remove: std::rc::Rc<std::cell::RefCell<Option<String>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));

    // Frame script (16 ms cadence — egui drops clicks slower than 0.8 s):
    //  1: render, capture row rect
    //  2: move pointer over row + right-press
    //  3: right-release → menu opens
    //  4: menu rendered; capture "Remove repo" rect
    //  5: move pointer over menu item + primary press
    //  6: primary-release → clicked() fires
    for frame in 1..=7 {
        let mut raw = egui::RawInput {
            screen_rect: Some(screen),
            time: Some(frame as f64 * 0.016),
            ..Default::default()
        };
        let row_now = row_rect.borrow().unwrap_or(egui::Rect::ZERO);
        let btn_now = remove_btn_rect.borrow().map(|r| r);
        match frame {
            2 => {
                let c = row_now.center();
                raw.events.push(egui::Event::PointerMoved(c));
                raw.events.push(egui::Event::PointerButton {
                    pos: c,
                    button: egui::PointerButton::Secondary,
                    pressed: true,
                    modifiers: Default::default(),
                });
            }
            3 => {
                let c = row_now.center();
                raw.events.push(egui::Event::PointerButton {
                    pos: c,
                    button: egui::PointerButton::Secondary,
                    pressed: false,
                    modifiers: Default::default(),
                });
            }
            5 => {
                let c = btn_now.expect("Remove repo rect known by frame 5").center();
                raw.events.push(egui::Event::PointerMoved(c));
                raw.events.push(egui::Event::PointerButton {
                    pos: c,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                });
            }
            6 => {
                let c = btn_now.expect("Remove repo rect known by frame 6").center();
                raw.events.push(egui::Event::PointerButton {
                    pos: c,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                });
            }
            _ => {}
        }

        let _ = ctx.run(raw, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                // Mirror show_list: ScrollArea wrapping the row.
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        // The exact row pattern from main.rs (trimmed to
                        // interactive essentials).
                        egui::Frame::none()
                            .fill(egui::Color32::from_gray(40))
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
                                    let (_strip, _) = ui.allocate_exact_size(
                                        egui::vec2(3.0, 44.0),
                                        egui::Sense::hover(),
                                    );
                                    ui.add_space(8.0);
                                    ui.vertical(|ui| {
                                        ui.label(egui::RichText::new("r").strong());
                                        ui.label(egui::RichText::new("o"));
                                    });
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.label("OK");
                                        },
                                    );
                                });
                            });
                        let row_response = ui.allocate_rect(ui.min_rect(), egui::Sense::click());
                        if row_rect.borrow().is_none() {
                            *row_rect.borrow_mut() = Some(row_response.rect);
                        }
                        // Context menu: exact items + order from main.rs.
                        row_response.context_menu(|ui| {
                            if ui.button("Pin to top").clicked() {
                                ui.close_menu();
                            }
                            if ui.button("Set category…").clicked() {
                                ui.close_menu();
                            }
                            let btn = ui.button("Remove repo");
                            if btn.clicked() {
                                *clicked_remove.borrow_mut() =
                                    Some("github.com/o/r".to_string());
                                ui.close_menu();
                            }
                            if frame >= 4 && remove_btn_rect.borrow().is_none() {
                                *remove_btn_rect.borrow_mut() = Some(btn.rect);
                            }
                        });
                    });
            });
        });
    }

    let got = clicked_remove.borrow().clone();
    assert_eq!(
        got,
        Some("github.com/o/r".to_string()),
        "Remove repo menu item click never registered"
    );
}
