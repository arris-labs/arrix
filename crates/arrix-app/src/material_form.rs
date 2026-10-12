//! The part's material form (ADR-0031, plans/c2-m3 step 8): which material
//! a part is made of, picked from the document's or made new, and the
//! picked one's name, density and colour. It is opened by a double-click on
//! a part in the tree. Like the component form it has no feature spec, so
//! the app builds the `FormView` the one renderer draws; it is an edit form
//! always: each change is one command (`AssignMaterial`, `EditMaterial`,
//! or a `Group` of `AddMaterial` and `AssignMaterial` for a new material).

use std::collections::BTreeMap;

use arrix_core::{MaterialId, PartId, QuantityKind};
use arrix_doc::expr::Expr;
use arrix_doc::{Command, Document, Material, Rgb};
use arrix_ui::{ChoiceOptionView, FieldValue, FormDiagnostic, FormField, FormView, UiIntent};

use crate::ArrixApp;

/// The choice's option that makes a new material.
const NEW: &str = "\u{1}new";
const MATERIAL: &str = "material";
const DENSITY: &str = "density";
const COLOUR: &str = "colour";

/// The grey a new material starts as.
const NEW_COLOUR: [f64; 3] = [0.7, 0.7, 0.7];

/// A part's material form: the part, and the field texts the record does
/// not say yet (a refused one stays with its reason).
#[derive(Clone, Debug)]
pub struct MaterialForm {
    part: PartId,
    texts: BTreeMap<String, String>,
    refused: BTreeMap<String, String>,
}

/// `#rrggbb` for a colour.
fn hex_of(c: [f64; 3]) -> String {
    let byte = |v: f64| (v * 255.0).round().clamp(0.0, 255.0) as u8;
    format!("#{:02x}{:02x}{:02x}", byte(c[0]), byte(c[1]), byte(c[2]))
}

/// The colour `#rrggbb` (the `#` optional) names.
fn parse_hex(text: &str) -> Result<[f64; 3], String> {
    let digits = text.trim().trim_start_matches('#');
    if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("{text:?} is not a colour: six hex digits, #rrggbb"));
    }
    let channel =
        |at: usize| u8::from_str_radix(&digits[at..at + 2], 16).map(|v| f64::from(v) / 255.0);
    match (channel(0), channel(2), channel(4)) {
        (Ok(r), Ok(g), Ok(b)) => Ok([r, g, b]),
        _ => Err(format!("{text:?} is not a colour: six hex digits, #rrggbb")),
    }
}

impl MaterialForm {
    pub fn new(part: PartId) -> Self {
        Self {
            part,
            texts: BTreeMap::new(),
            refused: BTreeMap::new(),
        }
    }

    pub fn part(&self) -> PartId {
        self.part
    }

    /// The material the part is made of.
    fn current<'a>(&self, doc: &'a Document) -> Option<&'a Material> {
        let id = doc.parts().get(&self.part)?.material?;
        doc.materials().get(&id)
    }

    /// The form as the card draws it; none once the part is gone.
    pub fn view(
        &self,
        doc: &Document,
        refusal: Option<&arrix_core::Diagnostic>,
    ) -> Option<FormView> {
        let part = doc.parts().get(&self.part)?;
        let current = self.current(doc);
        let mut options: Vec<ChoiceOptionView> = doc
            .materials()
            .values()
            .map(|m| ChoiceOptionView {
                name: m.id.to_string(),
                title: m.name.clone(),
            })
            .collect();
        options.push(ChoiceOptionView {
            name: NEW.into(),
            title: "New material…".into(),
        });
        let mut fields = vec![FormField {
            name: MATERIAL.into(),
            title: "Made of".into(),
            value: FieldValue::Choice {
                selected: current.map(|m| m.id.to_string()),
                options,
                clearable: true,
            },
            message: None,
        }];
        if let Some(m) = current {
            let density = self
                .texts
                .get(DENSITY)
                .cloned()
                .or_else(|| m.density.as_ref().map(|d| d.text().to_owned()))
                .unwrap_or_default();
            fields.push(FormField {
                name: DENSITY.into(),
                title: "Density".into(),
                value: FieldValue::Quantity {
                    text: density,
                    unit: "kg/m3".into(),
                },
                message: self.refused.get(DENSITY).cloned(),
            });
            fields.push(FormField {
                name: COLOUR.into(),
                title: "Colour".into(),
                value: FieldValue::Colour {
                    hex: self
                        .texts
                        .get(COLOUR)
                        .cloned()
                        .unwrap_or_else(|| hex_of(m.appearance.0)),
                    rgb: m.appearance.0.map(|c| c as f32),
                },
                message: self.refused.get(COLOUR).cloned(),
            });
        }
        Some(FormView {
            id: format!("material:{}", self.part),
            title: format!("Material of {}", part.name),
            create: false,
            name: current.map_or_else(String::new, |m| m.name.clone()),
            state: None,
            diagnostic: refusal.map(|d| FormDiagnostic {
                code: d.code.to_string(),
                message: d.message.clone(),
            }),
            read_only: None,
            fields,
        })
    }
}

/// A material name free in `doc`, from `base`.
fn free_name(doc: &Document, base: &str) -> String {
    let used = |n: &str| doc.materials().values().any(|m| m.name == n);
    std::iter::once(base.to_owned())
        .chain((2..).map(|i| format!("{base} {i}")))
        .find(|n| !used(n))
        .expect("an unbounded range has a free name")
}

impl ArrixApp {
    /// A double-click on a part in the tree: its material form. No command.
    pub(crate) fn open_material_form(&mut self, part: &str) {
        let Ok(part) = part.parse::<PartId>() else {
            return;
        };
        let Some(document) = self.documents.get_mut(self.active) else {
            return;
        };
        if !document.replica.document().parts().contains_key(&part) {
            return;
        }
        document.form = None;
        document.component_form = None;
        document.joint_form = None;
        document.clear_preview();
        document.material_form = Some(MaterialForm::new(part));
    }

    pub(crate) fn material_form_open(&self) -> bool {
        self.active_document()
            .is_some_and(|d| d.material_form.is_some())
    }

    pub(crate) fn material_form_view(&self) -> Option<FormView> {
        let document = self.active_document()?;
        document
            .material_form
            .as_ref()?
            .view(document.replica.document(), document.refusal.as_ref())
    }

    /// Closes a form whose part an undo took away.
    pub(crate) fn follow_material_forms(&mut self) {
        for document in &mut self.documents {
            let gone = document
                .material_form
                .as_ref()
                .is_some_and(|f| !document.replica.document().parts().contains_key(&f.part));
            if gone {
                document.material_form = None;
            }
        }
    }

    fn material_form_mut(&mut self) -> Option<&mut MaterialForm> {
        self.documents.get_mut(self.active)?.material_form.as_mut()
    }

    /// What the user did in the open material form.
    pub(crate) fn material_form_intent(&mut self, intent: UiIntent) {
        match intent {
            UiIntent::ChoiceSelected { field, value } if field == MATERIAL => match value {
                Some(value) if value == NEW => self.new_material(None),
                Some(value) => {
                    if let Ok(id) = value.parse::<MaterialId>() {
                        self.assign_material(Some(id));
                    }
                }
                None => self.assign_material(None),
            },
            UiIntent::NameCommitted { text } => {
                let text = text.trim().to_owned();
                if text.is_empty() {
                    return;
                }
                if self.current_material().is_some() {
                    self.edit_material(|m| m.name = text.clone());
                } else {
                    self.new_material(Some(text));
                }
            }
            UiIntent::FieldCommitted { field, text } if field == DENSITY => {
                self.commit_density(text);
            }
            UiIntent::FieldCommitted { field, text } if field == COLOUR => {
                self.commit_colour(text);
            }
            UiIntent::FormOk | UiIntent::FormCancelled => {
                if let Some(document) = self.documents.get_mut(self.active) {
                    document.material_form = None;
                    document.refusal = None;
                }
            }
            _ => {}
        }
    }

    fn current_material(&self) -> Option<Material> {
        let document = self.active_document()?;
        let form = document.material_form.as_ref()?;
        form.current(document.replica.document()).cloned()
    }

    fn submit_material(&mut self, command: Command) {
        let Some(document) = self.documents.get_mut(self.active) else {
            return;
        };
        match document.submit(command) {
            Ok(_) => {
                if let Some(form) = document.material_form.as_mut() {
                    form.texts.clear();
                    form.refused.clear();
                }
            }
            Err(reason) => self.session.notice = Some(reason),
        }
    }

    fn assign_material(&mut self, material: Option<MaterialId>) {
        let Some(part) = self.material_form_mut().map(|f| f.part) else {
            return;
        };
        self.submit_material(Command::AssignMaterial {
            part,
            body: None,
            material,
        });
    }

    /// A material made and assigned in one command: named `name`, else the
    /// next free "Material N", grey, with no density yet.
    fn new_material(&mut self, name: Option<String>) {
        let Some(document) = self.documents.get(self.active) else {
            return;
        };
        let Some(part) = document.material_form.as_ref().map(|f| f.part) else {
            return;
        };
        let name = name.unwrap_or_else(|| free_name(document.replica.document(), "Material"));
        let id: MaterialId = self.ids.mint();
        self.submit_material(Command::Group {
            label: "New material".into(),
            commands: vec![
                Command::AddMaterial {
                    material: Material {
                        id,
                        name,
                        density: None,
                        appearance: Rgb(NEW_COLOUR),
                    },
                },
                Command::AssignMaterial {
                    part,
                    body: None,
                    material: Some(id),
                },
            ],
        });
    }

    /// One `EditMaterial` of the part's material, changed by `change`; none
    /// when it changes nothing.
    fn edit_material(&mut self, change: impl FnOnce(&mut Material)) {
        let Some(record) = self.current_material() else {
            return;
        };
        let mut material = record.clone();
        change(&mut material);
        if material != record {
            self.submit_material(Command::EditMaterial { material });
        }
    }

    /// The density text was committed: checked as a density; empty clears
    /// it; a valid one is one `EditMaterial`.
    fn commit_density(&mut self, text: String) {
        let text = text.trim().to_owned();
        // The field shows its unit, so a bare number is in it.
        let full = if text.parse::<f64>().is_ok() {
            format!("{text} kg/m3")
        } else {
            text.clone()
        };
        let verdict = if text.is_empty() {
            Ok(None)
        } else {
            self.evaluate_text(&full, QuantityKind::Density)
                .map(|_| Expr::parse(&full).ok())
                .map_err(|e| format!("{}: {e}", e.code()))
        };
        let Some(form) = self.material_form_mut() else {
            return;
        };
        form.texts.insert(DENSITY.into(), text);
        match verdict {
            Ok(density) => {
                form.refused.remove(DENSITY);
                self.edit_material(|m| m.density = density);
            }
            Err(why) => {
                form.refused.insert(DENSITY.into(), why);
            }
        }
    }

    /// The colour text was committed: `#rrggbb`, one `EditMaterial`.
    fn commit_colour(&mut self, text: String) {
        let Some(form) = self.material_form_mut() else {
            return;
        };
        match parse_hex(&text) {
            Ok(colour) => {
                form.refused.remove(COLOUR);
                form.texts.remove(COLOUR);
                self.edit_material(|m| m.appearance = Rgb(colour));
            }
            Err(why) => {
                form.texts.insert(COLOUR.into(), text);
                form.refused.insert(COLOUR.into(), why);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_colour_round_trips_through_its_hex() {
        let c = parse_hex("#ccccd9").unwrap();
        assert_eq!(hex_of(c), "#ccccd9");
        assert_eq!(parse_hex("0a1b2c").map(hex_of), Ok("#0a1b2c".to_owned()));
        assert!(parse_hex("#12345").is_err());
        assert!(parse_hex("#12345g").is_err());
    }
}
