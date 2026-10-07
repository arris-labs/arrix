//! The field a dimension's value is typed in (docs/UI-RENDERING.md §Sketch
//! mode): a text box at the label, over the viewport. Enter or losing focus
//! commits the text, Escape dismisses it, and a text the app could not use
//! stays with the reason beneath it. The text being typed is the field's
//! own; the view carries only the text it opened with.

use egui::text::{CCursor, CCursorRange};
use egui::{Color32, Id, Key};
use serde::{Deserialize, Serialize};

use crate::PlaneToScreen;

/// A dimension's open field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DimensionFieldView {
    /// The dimension's constraint id.
    pub id: String,
    /// Where the label is, in plane-local metres.
    pub at: [f64; 2],
    /// What the field opened with: the expression, or the value.
    pub text: String,
    /// Why the last commit was not taken.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// What the field was asked.
#[derive(Clone, Debug, PartialEq)]
pub enum FieldEvent {
    Commit { constraint: String, text: String },
    Cancel,
}

const TEXT: &str = "arrix.dimension-field.text";
const OPENED: &str = "arrix.dimension-field.opened";
const RESELECT: &str = "arrix.dimension-field.reselect";
const OPEN: &str = "arrix.dimension-field.open";

/// Draws the field, if one is open, and says what was done with it this
/// frame. With none open it forgets what the last one had typed, so the
/// next opens with its own text.
pub fn dimension_field(
    ui: &mut egui::Ui,
    to_screen: &PlaneToScreen,
    field: Option<&DimensionFieldView>,
) -> Option<FieldEvent> {
    let ctx = ui.ctx().clone();
    let open_id = Id::new(OPEN);
    let Some(field) = field else {
        if let Some(gone) = ctx.data_mut(|d| d.remove_temp::<String>(open_id)) {
            ctx.data_mut(|d| {
                d.remove_temp::<String>(Id::new((TEXT, &gone)));
                d.remove_temp::<bool>(Id::new((OPENED, &gone)));
                d.remove_temp::<bool>(Id::new((RESELECT, &gone)));
            });
        }
        return None;
    };
    ctx.data_mut(|d| d.insert_temp(open_id, field.id.clone()));
    let at = to_screen.project(field.at)?;
    let text_id = Id::new((TEXT, &field.id));
    let opened_id = Id::new((OPENED, &field.id));
    let reselect_id = Id::new((RESELECT, &field.id));
    let mut text: String = ctx
        .data_mut(|d| d.get_temp(text_id))
        .unwrap_or_else(|| field.text.clone());
    // The text is selected when the field opens, and again after a commit
    // that was not taken, so typing replaces it.
    let first = ctx.data(|d| d.get_temp::<bool>(opened_id)).is_none();
    let reselect = ctx
        .data(|d| d.get_temp::<bool>(reselect_id))
        .unwrap_or(false);
    let mut event = None;
    egui::Area::new(Id::new("arrix.dimension-field"))
        .order(egui::Order::Foreground)
        .fixed_pos(at - egui::vec2(60.0, 14.0))
        .show(&ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut text)
                        .id(text_id.with("edit"))
                        .desired_width(120.0),
                );
                if let Some(error) = &field.error {
                    ui.colored_label(Color32::from_rgb(255, 120, 100), error);
                }
                if edit.lost_focus() {
                    event = Some(if ui.input(|i| i.key_pressed(Key::Escape)) {
                        FieldEvent::Cancel
                    } else {
                        FieldEvent::Commit {
                            constraint: field.id.clone(),
                            text: text.clone(),
                        }
                    });
                } else {
                    edit.request_focus();
                }
                if first || reselect {
                    let edit_id = text_id.with("edit");
                    if let Some(mut state) = egui::TextEdit::load_state(&ctx, edit_id) {
                        state.cursor.set_char_range(Some(CCursorRange::two(
                            CCursor::new(0),
                            CCursor::new(text.chars().count()),
                        )));
                        state.store(&ctx, edit_id);
                    }
                }
            });
        });
    ctx.data_mut(|d| {
        d.insert_temp(text_id, text);
        d.insert_temp(opened_id, true);
        d.insert_temp(reselect_id, event.is_some());
    });
    event
}
