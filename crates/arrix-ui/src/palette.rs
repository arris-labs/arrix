//! The command palette: a query field over a list of commands, each with its
//! keys and whether it applies (docs/UI-RENDERING.md §The shell). Which key
//! moves the selection or runs it is the keymap's, not this widget's.

use crate::view::{PaletteView, UiIntent};

const WIDTH: f32 = 520.0;
const TOP: f32 = 96.0;

pub fn palette(ctx: &egui::Context, view: &PaletteView, intents: &mut Vec<UiIntent>) {
    let window = egui::Window::new("arrix.palette")
        .title_bar(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, TOP))
        .fixed_size(egui::vec2(WIDTH, 0.0))
        .show(ctx, |ui| {
            let mut text = view.query.clone();
            let field = ui.add(
                egui::TextEdit::singleline(&mut text)
                    .hint_text("Type a command")
                    .desired_width(f32::INFINITY),
            );
            // The palette is modal: the field always has the keyboard.
            field.request_focus();
            if text != view.query {
                intents.push(UiIntent::PaletteQuery { text });
            }
            ui.separator();
            if view.results.is_empty() {
                ui.weak("No command matches");
            }
            for (i, entry) in view.results.iter().enumerate() {
                ui.add_enabled_ui(entry.enabled, |ui| {
                    ui.horizontal(|ui| {
                        let row = ui.selectable_label(i == view.selected, &entry.title);
                        ui.weak(&entry.id);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.weak(entry.keys.join(", "));
                        });
                        if row.clicked() {
                            intents.push(UiIntent::CommandInvoked {
                                command: entry.id.clone(),
                            });
                        }
                    });
                });
            }
        });
    let Some(window) = window else { return };
    let pressed_outside = ctx.input(|i| {
        i.pointer.any_pressed()
            && i.pointer
                .interact_pos()
                .is_some_and(|p| !window.response.rect.contains(p))
    });
    if pressed_outside {
        intents.push(UiIntent::PaletteClosed);
    }
}
