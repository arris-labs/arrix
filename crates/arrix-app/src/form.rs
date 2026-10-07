//! The property card's forms, from the app's side (docs/UI-RENDERING.md
//! §Declarative plugin UI, ADR-0018). A form is derived from a feature
//! type's spec and nothing else. A create form is a draft, session state
//! on the document's tab: its fields change the draft and issue nothing,
//! OK is one `AddFeature` (in a `Group` with `AddPart` when the document
//! has no part), and Cancel issues none.

use std::collections::BTreeMap;

use arrix_core::{FeatureId, PartId, QuantityKind};
use arrix_doc::expr::Expr;
use arrix_doc::{Command, FeatureRecord, FeatureTypeId, Part};
use arrix_plugin_api::{FeatureTypeSpec, InputKind, ShownWhen};
use arrix_ui::{ChoiceOptionView, FieldValue, FormField, FormView, ReferenceValueView, UiIntent};

use crate::ArrixApp;

/// A form being filled: a create form's draft.
#[derive(Clone, Debug)]
pub struct Form {
    /// Names the form for the renderer's text buffers; a new form has a
    /// new one.
    id: String,
    spec: FeatureTypeSpec,
    name: String,
    /// Every parameter's text, as typed: a refused one keeps it.
    texts: BTreeMap<String, String>,
    /// Why a parameter's text was refused, by parameter.
    refused: BTreeMap<String, String>,
    /// The choices the user made; the rest hold the spec's default.
    choices: BTreeMap<String, String>,
    /// The reference field a pick fills.
    active: Option<String>,
}

/// What a quantity of `kind` is measured in when its text names no unit.
fn unit_hint(kind: QuantityKind) -> &'static str {
    match kind {
        QuantityKind::Length => "mm",
        QuantityKind::Angle => "deg",
        QuantityKind::Mass => "kg",
        QuantityKind::Count | QuantityKind::Ratio => "",
    }
}

/// What an input takes, in words, for a field that is asking.
fn takes(kind: InputKind) -> &'static str {
    match kind {
        InputKind::Plane => "plane or planar face",
        InputKind::Region => "sketch region",
        InputKind::Body => "body",
        InputKind::Edge => "edge",
        InputKind::Point => "point",
        InputKind::Axis => "axis or straight edge",
        InputKind::Tool => "feature",
    }
}

impl Form {
    /// A draft of a new feature of `spec`, named `name`, its parameters
    /// seeded with `params` (the user's defaults over the type's).
    pub fn create(
        id: String,
        spec: &FeatureTypeSpec,
        name: String,
        params: Vec<(String, String)>,
    ) -> Self {
        Self {
            id,
            spec: spec.clone(),
            name,
            texts: params.into_iter().collect(),
            refused: BTreeMap::new(),
            choices: BTreeMap::new(),
            active: None,
        }
    }

    /// What choice `name` holds: the user's pick, else the spec's default.
    fn choice(&self, name: &str) -> Option<&str> {
        self.choices.get(name).map(String::as_str).or_else(|| {
            self.spec
                .choices
                .iter()
                .find(|c| c.name == name)?
                .default
                .as_deref()
        })
    }

    /// Whether a field with this `shown_when` shows under the choices now.
    fn shown(&self, when: &Option<ShownWhen>) -> bool {
        when.as_ref().is_none_or(|w| {
            self.choice(&w.choice)
                .is_some_and(|held| w.values.iter().any(|v| v == held))
        })
    }

    /// The form as the card draws it: the choices first (they decide what
    /// else shows), then the inputs, then the parameters, each in the
    /// spec's order.
    pub fn view(&self) -> FormView {
        let mut fields = Vec::new();
        for c in &self.spec.choices {
            fields.push(FormField {
                name: c.name.clone(),
                title: c.title.clone(),
                value: FieldValue::Choice {
                    selected: self.choice(&c.name).map(str::to_owned),
                    options: c
                        .options
                        .iter()
                        .map(|o| ChoiceOptionView {
                            name: o.name.clone(),
                            title: o.title.clone(),
                        })
                        .collect(),
                    clearable: c.default.is_none(),
                },
                message: None,
            });
        }
        for i in self
            .spec
            .inputs
            .iter()
            .filter(|i| self.shown(&i.shown_when))
        {
            fields.push(FormField {
                name: i.name.clone(),
                title: i.title.clone(),
                value: FieldValue::Reference {
                    takes: takes(i.kind).to_owned(),
                    values: Vec::<ReferenceValueView>::new(),
                    list: i.list,
                    active: self.active.as_deref() == Some(i.name.as_str()),
                },
                message: None,
            });
        }
        for p in self
            .spec
            .params
            .iter()
            .filter(|p| self.shown(&p.shown_when))
        {
            fields.push(FormField {
                name: p.name.clone(),
                title: p.title.clone(),
                value: FieldValue::Quantity {
                    text: self.texts.get(&p.name).cloned().unwrap_or_default(),
                    unit: unit_hint(p.kind).to_owned(),
                },
                message: self.refused.get(&p.name).cloned(),
            });
        }
        FormView {
            id: self.id.clone(),
            title: self.spec.title.clone(),
            create: true,
            name: self.name.clone(),
            state: None,
            diagnostic: None,
            read_only: None,
            fields,
        }
    }

    /// The record OK adds, or why it cannot: a refused text, or one that
    /// does not parse, is named.
    fn record(&self, id: FeatureId) -> Result<FeatureRecord, String> {
        if let Some((field, why)) = self.refused.iter().next() {
            return Err(format!("{}: fix {field} first ({why})", self.spec.title));
        }
        let mut params = BTreeMap::new();
        for p in &self.spec.params {
            let text = self.texts.get(&p.name).map_or("", String::as_str);
            let expr =
                Expr::parse(text).map_err(|e| format!("{}: {}: {e}", self.spec.title, p.name))?;
            params.insert(p.name.clone(), expr);
        }
        Ok(FeatureRecord {
            id,
            type_id: FeatureTypeId::new(self.spec.id.clone()).map_err(|e| e.to_string())?,
            type_version: self.spec.version,
            name: self.name.clone(),
            params,
            choices: self.choices.clone(),
            inputs: BTreeMap::new(),
            suppressed: false,
            sketch: None,
            frozen: None,
        })
    }
}

impl ArrixApp {
    /// `<type>.create`: opens a draft of a new feature of `type_id` in the
    /// property card. No command. A draft already open is replaced.
    pub(crate) fn open_create_form(&mut self, type_id: &str) {
        if self.active_document().is_some_and(|d| d.sketch.is_some()) {
            self.session.notice = Some("Finish the sketch first.".into());
            return;
        }
        let Some(spec) = self.feature_types.iter().find(|t| t.id == type_id) else {
            return;
        };
        let params = self.defaults.effective_params(spec);
        let Some(document) = self.documents.get(self.active) else {
            return;
        };
        let used = |n: &str| {
            document
                .replica
                .document()
                .parts()
                .values()
                .next()
                .is_some_and(|p| p.features.values().any(|r| r.name == n))
        };
        let name = (1..)
            .map(|i| format!("{} {i}", spec.title))
            .find(|n| !used(n))
            .expect("an unbounded range has a free name");
        self.session.forms_opened += 1;
        let id = format!("new:{}:{}", spec.id, self.session.forms_opened);
        let form = Form::create(id, spec, name, params);
        if let Some(document) = self.documents.get_mut(self.active) {
            document.form = Some(form);
        }
    }

    /// The form the property card draws, if one is open.
    pub(crate) fn form_view(&self) -> Option<FormView> {
        Some(self.active_document()?.form.as_ref()?.view())
    }

    pub(crate) fn form_open(&self) -> bool {
        self.active_document().is_some_and(|d| d.form.is_some())
    }

    /// What the user did in the open form.
    pub(crate) fn form_intent(&mut self, intent: UiIntent) {
        match intent {
            UiIntent::FieldEdited { field, text } => {
                if let Some(form) = self.form_mut() {
                    form.texts.insert(field, text);
                }
            }
            UiIntent::FieldCommitted { field, text } => self.commit_field(&field, text),
            UiIntent::NameCommitted { text } => {
                let text = text.trim();
                if let Some(form) = self.form_mut()
                    && !text.is_empty()
                {
                    form.name = text.to_owned();
                }
            }
            UiIntent::ChoiceSelected { field, value } => {
                if let Some(form) = self.form_mut() {
                    match value {
                        Some(value) => form.choices.insert(field, value),
                        None => form.choices.remove(&field),
                    };
                }
            }
            UiIntent::ReferenceFieldActivated { field } => {
                if let Some(form) = self.form_mut() {
                    form.active = Some(field);
                }
            }
            // Nothing picks into a field yet, so nothing is there to remove.
            UiIntent::ReferenceRemoved { .. } => {}
            UiIntent::FormOk => self.form_ok(),
            UiIntent::FormCancelled => self.form_cancel(),
            _ => {}
        }
    }

    fn form_mut(&mut self) -> Option<&mut Form> {
        self.documents.get_mut(self.active)?.form.as_mut()
    }

    /// A parameter's text was committed: it is kept as typed, and checked as
    /// the form can check it, by its parse and its quantity kind against the
    /// document's parameters. A refusal stays with the field.
    fn commit_field(&mut self, field: &str, text: String) {
        let Some(kind) = self
            .active_document()
            .and_then(|d| d.form.as_ref())
            .and_then(|f| f.spec.params.iter().find(|p| p.name == field))
            .map(|p| p.kind)
        else {
            return;
        };
        let verdict = self
            .evaluate_text(&text, kind)
            .map(|_| ())
            .map_err(|e| format!("{}: {e}", e.code()));
        if let Some(form) = self.form_mut() {
            form.texts.insert(field.to_owned(), text);
            match verdict {
                Ok(()) => form.refused.remove(field),
                Err(why) => form.refused.insert(field.to_owned(), why),
            };
        }
    }

    /// OK: the draft becomes one `AddFeature`, in a new part when the
    /// document has none (`Group`, one undo entry), at the end of the part
    /// as a sketch is placed. A draft that cannot be added stays open, and
    /// the status line says why.
    pub(crate) fn form_ok(&mut self) {
        let Some(form) = self.active_document().and_then(|d| d.form.clone()) else {
            return;
        };
        let feature = FeatureId(self.ids.next_id());
        let record = match form.record(feature) {
            Ok(record) => record,
            Err(reason) => {
                self.session.notice = Some(reason);
                return;
            }
        };
        let Some(document) = self.documents.get(self.active) else {
            return;
        };
        let command = match document.replica.document().parts().values().next() {
            Some(part) => Command::AddFeature {
                part: part.id,
                at: part.history.len(),
                record,
            },
            None => {
                let part = PartId(self.ids.next_id());
                Command::Group {
                    label: format!("Create {}", form.spec.title.to_lowercase()),
                    commands: vec![
                        Command::AddPart {
                            part: Part {
                                id: part,
                                name: "Part 1".into(),
                                history: Vec::new(),
                                features: BTreeMap::new(),
                                rollback: None,
                            },
                        },
                        Command::AddFeature {
                            part,
                            at: 0,
                            record,
                        },
                    ],
                }
            }
        };
        let Some(document) = self.documents.get_mut(self.active) else {
            return;
        };
        match document.submit(command) {
            Ok(_) => {
                document.form = None;
                document.view.feature_selection = Some(feature);
            }
            Err(reason) => self.session.notice = Some(reason),
        }
    }

    /// Cancel, or Escape: the draft goes. No command.
    pub(crate) fn form_cancel(&mut self) {
        if let Some(document) = self.documents.get_mut(self.active) {
            document.form = None;
        }
    }
}
