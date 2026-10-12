//! The property card's form, rendered from a `FormView` (docs/UI-RENDERING.md
//! §Declarative plugin UI, ADR-0018). One renderer draws every feature
//! type's form, a built-in's and a plugin's alike: the view carries what a
//! feature type's spec declared and the draft or record holds, and this
//! crate never sees where either came from.
//!
//! A text field's buffer is the renderer's own, kept in egui's memory under
//! the form's id and the field's name. It follows the view's text whenever
//! that changes (an undo with the form open shows the document's value) and
//! keeps what was typed when it does not (a refused text stays in its field
//! with the reason beneath it).

use egui::{Color32, Id, Key, RichText};
use serde::{Deserialize, Serialize};

use crate::view::{FeatureState, UiIntent};

/// A feature type's form: a header, then one field per declared parameter,
/// choice and input the choices leave shown, in the spec's order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FormView {
    /// Names the form being edited: the draft's, or the feature's. A
    /// different id starts every field's buffer afresh.
    pub id: String,
    /// The feature type's title.
    pub title: String,
    /// A create form (a draft) ends in OK or Cancel; an edit form commits
    /// field by field and has neither.
    pub create: bool,
    /// The feature's name.
    pub name: String,
    /// Where the feature stands; none for a draft not yet evaluated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<FeatureState>,
    /// The feature's diagnostic, or the preview's: what only evaluation can
    /// say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<FormDiagnostic>,
    /// Why the values are read only (a frozen feature), if they are.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_only: Option<String>,
    pub fields: Vec<FormField>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FormDiagnostic {
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FormField {
    /// The spec's name for the field: what an intent about it says.
    pub name: String,
    pub title: String,
    pub value: FieldValue,
    /// Why the field's text was refused, or what is wrong with it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum FieldValue {
    /// A quantity or a count: an expression, typed.
    Quantity {
        text: String,
        /// What the text is measured in when it names no unit: `mm`, `deg`,
        /// or nothing for a count.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        unit: String,
    },
    /// One of a few words.
    Choice {
        /// The option held; none when the choice is unset.
        selected: Option<String>,
        options: Vec<ChoiceOptionView>,
        /// The choice may be left unset (it has no default).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        clearable: bool,
    },
    /// A colour: its hex text (`#rrggbb`), typed and committed like a
    /// quantity, with a swatch of what it is.
    Colour {
        hex: String,
        /// Red, green and blue in 0 to 1: the swatch.
        rgb: [f32; 3],
    },
    /// A reference or a list of them, picked in the scene or the tree.
    Reference {
        /// What it takes, in words: `plane`, `sketch region`.
        takes: String,
        values: Vec<ReferenceValueView>,
        list: bool,
        /// This is the field a pick fills.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        active: bool,
        /// For a field that holds a lost reference: what it may be
        /// re-picked as, best first, each a row that rebinds it. The scene
        /// may have nothing to click (a failed feature has no body), so
        /// the form offers them itself.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        candidates: Vec<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChoiceOptionView {
    pub name: String,
    pub title: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReferenceValueView {
    pub label: String,
    /// The reference no longer resolves.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub lost: bool,
}

/// The id the name field's intents carry nothing for; its text is the form's.
const NAME_FIELD: &str = "\u{0}name";

/// A text buffer the renderer keeps for one field.
#[derive(Clone, Default)]
struct Buffer {
    /// The view's text when the buffer last followed it.
    seen: String,
    text: String,
    /// Edited since the last commit.
    dirty: bool,
}

/// Draws `form` into `ui` and says what the user did with it this frame.
pub fn form(ui: &mut egui::Ui, form: &FormView) -> Vec<UiIntent> {
    let mut intents = Vec::new();
    // The state shares the title's row: a preview's state comes and goes
    // as it evaluates, and must not move the fields under the pointer.
    ui.horizontal(|ui| {
        ui.heading(&form.title);
        if let Some(state) = form.state.and_then(FeatureState::tag) {
            ui.weak(state);
        }
    });
    if let Some(d) = &form.diagnostic {
        ui.colored_label(ERROR, &d.code);
        ui.colored_label(ERROR, &d.message);
    }
    if let Some(reason) = &form.read_only {
        ui.colored_label(ui.visuals().warn_fg_color, reason);
    }
    ui.add_enabled_ui(form.read_only.is_none(), |ui| {
        egui::Grid::new(Id::new(("arrix.form.grid", &form.id)))
            .num_columns(2)
            .spacing([8.0, 6.0])
            .show(ui, |ui| {
                ui.label("Name");
                if let Some(text) = text_field(ui, form, NAME_FIELD, &form.name, &mut intents) {
                    intents.push(UiIntent::NameCommitted { text });
                }
                ui.end_row();
                for field in &form.fields {
                    field_ui(ui, form, field, &mut intents);
                }
            });
        if form.create {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("OK").clicked() {
                    intents.push(UiIntent::FormOk);
                }
                if ui.button("Cancel").clicked() {
                    intents.push(UiIntent::FormCancelled);
                }
            });
        }
    });
    intents
}

const ERROR: Color32 = Color32::from_rgb(255, 120, 100);

fn field_ui(ui: &mut egui::Ui, form: &FormView, field: &FormField, intents: &mut Vec<UiIntent>) {
    match &field.value {
        FieldValue::Quantity { text, unit } => {
            ui.label(&field.title);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    if let Some(committed) = text_field(ui, form, &field.name, text, intents) {
                        intents.push(UiIntent::FieldCommitted {
                            field: field.name.clone(),
                            text: committed,
                        });
                    }
                    if !unit.is_empty() {
                        ui.weak(unit);
                    }
                });
                message(ui, field);
            });
        }
        FieldValue::Colour { hex, rgb } => {
            ui.label(&field.title);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let to_byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
                    let swatch =
                        Color32::from_rgb(to_byte(rgb[0]), to_byte(rgb[1]), to_byte(rgb[2]));
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 3.0, swatch);
                    if let Some(committed) = text_field(ui, form, &field.name, hex, intents) {
                        intents.push(UiIntent::FieldCommitted {
                            field: field.name.clone(),
                            text: committed,
                        });
                    }
                });
                message(ui, field);
            });
        }
        FieldValue::Choice {
            selected,
            options,
            clearable,
        } => {
            ui.label(&field.title);
            ui.vertical(|ui| {
                let shown = selected
                    .as_ref()
                    .and_then(|s| options.iter().find(|o| &o.name == s))
                    .map_or("—", |o| o.title.as_str());
                egui::ComboBox::from_id_salt((&form.id, &field.name))
                    .selected_text(shown)
                    .show_ui(ui, |ui| {
                        if *clearable
                            && ui.selectable_label(selected.is_none(), "—").clicked()
                            && selected.is_some()
                        {
                            intents.push(UiIntent::ChoiceSelected {
                                field: field.name.clone(),
                                value: None,
                            });
                        }
                        for option in options {
                            let held = selected.as_ref() == Some(&option.name);
                            if ui.selectable_label(held, &option.title).clicked() && !held {
                                intents.push(UiIntent::ChoiceSelected {
                                    field: field.name.clone(),
                                    value: Some(option.name.clone()),
                                });
                            }
                        }
                    });
                message(ui, field);
            });
        }
        FieldValue::Reference {
            takes,
            values,
            list,
            active,
            candidates,
        } => reference_ui(
            ui, field, takes, values, *list, *active, candidates, intents,
        ),
    }
    ui.end_row();
}

#[allow(
    clippy::too_many_arguments,
    reason = "the field's parts, as the view has them"
)]
fn reference_ui(
    ui: &mut egui::Ui,
    field: &FormField,
    takes: &str,
    values: &[ReferenceValueView],
    list: bool,
    active: bool,
    candidates: &[String],
    intents: &mut Vec<UiIntent>,
) {
    let title = ui.selectable_label(active, &field.title);
    if title.clicked() && !active {
        intents.push(UiIntent::ReferenceFieldActivated {
            field: field.name.clone(),
        });
    }
    ui.vertical(|ui| {
        let stroke = if active {
            ui.visuals().selection.stroke
        } else {
            ui.visuals().widgets.noninteractive.bg_stroke
        };
        egui::Frame::new()
            .stroke(stroke)
            .inner_margin(4.0)
            .corner_radius(3.0)
            .show(ui, |ui| {
                ui.set_min_width(120.0);
                if values.is_empty() {
                    let what = if list { "one or more" } else { "a" };
                    let hint =
                        egui::Label::new(RichText::new(format!("pick {what} {takes}")).weak())
                            .sense(egui::Sense::click());
                    if ui.add(hint).clicked() && !active {
                        intents.push(UiIntent::ReferenceFieldActivated {
                            field: field.name.clone(),
                        });
                    }
                }
                for (index, value) in values.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if value.lost {
                            ui.colored_label(ERROR, format!("lost: {}", value.label));
                        } else {
                            ui.label(&value.label);
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("×").on_hover_text("Remove").clicked() {
                                intents.push(UiIntent::ReferenceRemoved {
                                    field: field.name.clone(),
                                    index,
                                });
                            }
                        });
                    });
                }
            });
        message(ui, field);
        if !candidates.is_empty() {
            ui.weak("Re-pick as, best first:");
            for (index, candidate) in candidates.iter().enumerate() {
                if ui.small_button(candidate).clicked() {
                    intents.push(UiIntent::CandidatePicked {
                        field: field.name.clone(),
                        index,
                    });
                }
            }
        }
    });
}

fn message(ui: &mut egui::Ui, field: &FormField) {
    if let Some(message) = &field.message {
        ui.label(RichText::new(message).color(ERROR).small());
    }
}

/// A single-line text box on `shown`, the view's text. Reports each change
/// as `FieldEdited`, and returns the text when it is committed by Enter or
/// by leaving the box; Escape puts the view's text back.
fn text_field(
    ui: &mut egui::Ui,
    form: &FormView,
    name: &str,
    shown: &str,
    intents: &mut Vec<UiIntent>,
) -> Option<String> {
    let buffer_id = Id::new(("arrix.form.buffer", &form.id, name));
    let mut buffer: Buffer = ui.data_mut(|d| d.get_temp(buffer_id)).unwrap_or_default();
    if buffer.seen != shown {
        // The view moved. If it moved to what was typed (the draft took the
        // edit), the box is as it was; if it moved elsewhere (an undo with
        // the form open), the box follows it.
        if buffer.text != shown {
            buffer.text = shown.to_owned();
            buffer.dirty = false;
        }
        buffer.seen = shown.to_owned();
    }
    let edit = ui.add(
        egui::TextEdit::singleline(&mut buffer.text)
            .id(buffer_id.with("edit"))
            .desired_width(120.0),
    );
    if edit.changed() {
        buffer.dirty = true;
        if name != NAME_FIELD {
            intents.push(UiIntent::FieldEdited {
                field: name.to_owned(),
                text: buffer.text.clone(),
            });
        }
    }
    let mut committed = None;
    if edit.lost_focus() {
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            buffer.text = shown.to_owned();
            buffer.dirty = false;
        } else if buffer.dirty {
            committed = Some(buffer.text.clone());
            buffer.dirty = false;
        }
    }
    ui.data_mut(|d| d.insert_temp(buffer_id, buffer));
    committed
}
