//! The property card's forms, from the app's side (docs/UI-RENDERING.md
//! §Declarative plugin UI, ADR-0018). A form is derived from a feature
//! type's spec and nothing else. A create form is a draft, session state
//! on the document's tab: its fields change the draft and issue nothing,
//! OK is one `AddFeature` (in a `Group` with `AddPart` when the document
//! has no part), and Cancel issues none. An edit form is the same fields
//! over an existing feature's record: it is read from the replica each
//! frame, and a field commit or a choice is one `EditFeature`.

use std::collections::BTreeMap;

use arrix_core::{FeatureId, PartId, QuantityKind};
use arrix_doc::expr::Expr;
use arrix_doc::{Command, FeatureEdit, FeatureRecord, FeatureTypeId, Part};
use arrix_plugin_api::{FeatureTypeSpec, InputKind, ShownWhen};
use arrix_ui::{ChoiceOptionView, FieldValue, FormField, FormView, ReferenceValueView, UiIntent};

use crate::ArrixApp;
use crate::documents::OpenDocument;

/// A form being filled: a create form's draft, or an edit form over a
/// feature's record.
#[derive(Clone, Debug)]
pub struct Form {
    /// The feature an edit form edits; none for a create form.
    edit: Option<FeatureId>,
    /// An edit form's name as committed, until the record says it.
    renamed: Option<String>,
    /// Names the form for the renderer's text buffers; a new form has a
    /// new one.
    id: String,
    spec: FeatureTypeSpec,
    name: String,
    /// A create form: every parameter's text, as typed. An edit form: the
    /// texts committed or refused that the record does not say yet; the
    /// rest are the record's.
    texts: BTreeMap<String, String>,
    /// Why a parameter's text was refused, by parameter.
    refused: BTreeMap<String, String>,
    /// A create form's choices the user made; the rest hold the spec's
    /// default. An edit form's are the record's.
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
            edit: None,
            renamed: None,
            id,
            spec: spec.clone(),
            name,
            texts: params.into_iter().collect(),
            refused: BTreeMap::new(),
            choices: BTreeMap::new(),
            active: None,
        }
    }

    /// An edit form over `feature`.
    pub fn edit(spec: &FeatureTypeSpec, feature: FeatureId) -> Self {
        Self {
            edit: Some(feature),
            renamed: None,
            id: format!("edit:{feature}"),
            spec: spec.clone(),
            name: String::new(),
            texts: BTreeMap::new(),
            refused: BTreeMap::new(),
            choices: BTreeMap::new(),
            active: None,
        }
    }

    /// What choice `name` holds: the record's or the user's pick, else the
    /// spec's default.
    fn choice<'a>(&'a self, record: Option<&'a FeatureRecord>, name: &str) -> Option<&'a str> {
        let held = record.map_or(&self.choices, |r| &r.choices);
        held.get(name).map(String::as_str).or_else(|| {
            self.spec
                .choices
                .iter()
                .find(|c| c.name == name)?
                .default
                .as_deref()
        })
    }

    /// Whether a field with this `shown_when` shows under the choices now.
    fn shown(&self, record: Option<&FeatureRecord>, when: &Option<ShownWhen>) -> bool {
        when.as_ref().is_none_or(|w| {
            self.choice(record, &w.choice)
                .is_some_and(|held| w.values.iter().any(|v| v == held))
        })
    }

    /// The form as the card draws it: the choices first (they decide what
    /// else shows), then the inputs, then the parameters, each in the
    /// spec's order.
    pub fn view(&self, record: Option<&FeatureRecord>) -> FormView {
        let mut fields = Vec::new();
        for c in &self.spec.choices {
            fields.push(FormField {
                name: c.name.clone(),
                title: c.title.clone(),
                value: FieldValue::Choice {
                    selected: self.choice(record, &c.name).map(str::to_owned),
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
            .filter(|i| self.shown(record, &i.shown_when))
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
            .filter(|p| self.shown(record, &p.shown_when))
        {
            fields.push(FormField {
                name: p.name.clone(),
                title: p.title.clone(),
                value: FieldValue::Quantity {
                    text: self.text(record, &p.name),
                    unit: unit_hint(p.kind).to_owned(),
                },
                message: self.refused.get(&p.name).cloned(),
            });
        }
        FormView {
            id: self.id.clone(),
            title: self.spec.title.clone(),
            create: record.is_none(),
            name: match record {
                Some(r) => self.renamed.clone().unwrap_or_else(|| r.name.clone()),
                None => self.name.clone(),
            },
            state: None,
            diagnostic: None,
            read_only: None,
            fields,
        }
    }

    /// What parameter `name`'s field shows: what was typed and the record
    /// does not say yet, else the record's.
    fn text(&self, record: Option<&FeatureRecord>, name: &str) -> String {
        if let Some(text) = self.texts.get(name) {
            return text.clone();
        }
        record
            .and_then(|r| r.params.get(name))
            .map(|e| e.text().to_owned())
            .unwrap_or_default()
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

/// The record of `feature` in the document the tab holds.
fn feature_record(document: &OpenDocument, feature: FeatureId) -> Option<&FeatureRecord> {
    document
        .replica
        .document()
        .parts()
        .values()
        .find_map(|p| p.features.get(&feature))
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

    /// `TreeFeatureActivated` on a feature that is neither a sketch nor a
    /// datum plane: opens its edit form from the record. No command; a draft
    /// open is replaced, as another `.create` replaces it.
    pub(crate) fn open_edit_form(&mut self, feature: FeatureId) {
        let Some(document) = self.documents.get(self.active) else {
            return;
        };
        let Some(record) = feature_record(document, feature) else {
            return;
        };
        let Some(spec) = self
            .feature_types
            .iter()
            .find(|t| t.id == record.type_id.as_str())
        else {
            return;
        };
        let form = Form::edit(spec, feature);
        if let Some(document) = self.documents.get_mut(self.active) {
            document.form = Some(form);
        }
    }

    /// The form the property card draws, if one is open.
    pub(crate) fn form_view(&self) -> Option<FormView> {
        let document = self.active_document()?;
        let form = document.form.as_ref()?;
        match form.edit {
            Some(feature) => Some(form.view(Some(feature_record(document, feature)?))),
            None => Some(form.view(None)),
        }
    }

    /// Keeps each tab's edit form in step with its document: one whose
    /// feature an undo took away closes, and a typed text or name the
    /// record now says is dropped, so the record is what shows.
    pub(crate) fn follow_forms(&mut self) {
        for document in &mut self.documents {
            let Some(feature) = document.form.as_ref().and_then(|f| f.edit) else {
                continue;
            };
            let Some(record) = feature_record(document, feature).cloned() else {
                document.form = None;
                continue;
            };
            if let Some(form) = &mut document.form {
                let refused = &form.refused;
                form.texts.retain(|k, t| {
                    refused.contains_key(k) || record.params.get(k).is_none_or(|e| e.text() != t)
                });
                if form.renamed.as_deref() == Some(record.name.as_str()) {
                    form.renamed = None;
                }
            }
        }
    }

    pub(crate) fn form_open(&self) -> bool {
        self.active_document().is_some_and(|d| d.form.is_some())
    }

    /// What the user did in the open form.
    pub(crate) fn form_intent(&mut self, intent: UiIntent) {
        match intent {
            UiIntent::FieldEdited { field, text } => {
                if let Some(form) = self.form_mut()
                    && form.edit.is_none()
                {
                    form.texts.insert(field, text);
                }
            }
            UiIntent::FieldCommitted { field, text } => self.commit_field(&field, text),
            UiIntent::NameCommitted { text } => {
                let text = text.trim();
                if text.is_empty() {
                    return;
                }
                match self.form_mut() {
                    Some(form) if form.edit.is_none() => form.name = text.to_owned(),
                    Some(_) => self.edit_name(text),
                    None => {}
                }
            }
            UiIntent::ChoiceSelected { field, value } => match self.form_mut() {
                Some(form) if form.edit.is_none() => {
                    match value {
                        Some(value) => form.choices.insert(field, value),
                        None => form.choices.remove(&field),
                    };
                }
                Some(_) => self.edit_choice(field, value),
                None => {}
            },
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
        let edits = self.active_document().and_then(|d| d.form.as_ref()?.edit);
        let Some(form) = self.form_mut() else {
            return;
        };
        match &verdict {
            Ok(()) => form.refused.remove(field),
            Err(why) => form.refused.insert(field.to_owned(), why.clone()),
        };
        if let (Some(feature), Ok(())) = (edits, &verdict) {
            form.texts.insert(field.to_owned(), text.clone());
            self.edit_param(feature, field, &text);
        } else {
            form.texts.insert(field.to_owned(), text);
        }
    }

    /// An edit form's valid text: one `EditFeature`, or none when it is
    /// the expression the record holds.
    fn edit_param(&mut self, feature: FeatureId, field: &str, text: &str) {
        let Ok(expr) = Expr::parse(text) else {
            return;
        };
        let held = self
            .active_document()
            .and_then(|d| feature_record(d, feature)?.params.get(field).cloned());
        if held.as_ref() == Some(&expr) {
            if let Some(form) = self.form_mut() {
                form.texts.remove(field);
            }
            return;
        }
        let mut edit = FeatureEdit::default();
        edit.params.insert(field.to_owned(), Some(expr));
        self.edit_feature(feature, edit);
    }

    /// The feature the open edit form edits, with its record's name and
    /// choice for comparing.
    fn editing(&self) -> Option<(FeatureId, &FeatureRecord)> {
        let document = self.active_document()?;
        let feature = document.form.as_ref()?.edit?;
        Some((feature, feature_record(document, feature)?))
    }

    fn edit_name(&mut self, name: &str) {
        let Some((feature, record)) = self.editing() else {
            return;
        };
        if record.name == name {
            return;
        }
        let edit = FeatureEdit {
            name: Some(name.to_owned()),
            ..FeatureEdit::default()
        };
        if self.edit_feature(feature, edit)
            && let Some(form) = self.form_mut()
        {
            form.renamed = Some(name.to_owned());
        }
    }

    fn edit_choice(&mut self, field: String, value: Option<String>) {
        let Some((feature, record)) = self.editing() else {
            return;
        };
        let held = record.choices.get(&field).cloned();
        let default = self
            .feature_types
            .iter()
            .find(|t| t.id == record.type_id.as_str())
            .and_then(|t| t.choices.iter().find(|c| c.name == field)?.default.clone());
        if held == value || (held.is_none() && value == default) {
            return;
        }
        let mut edit = FeatureEdit::default();
        edit.choices.insert(field, value);
        self.edit_feature(feature, edit);
    }

    /// One `EditFeature`; a refusal goes to the status line, and the
    /// form keeps showing the record.
    fn edit_feature(&mut self, feature: FeatureId, edit: FeatureEdit) -> bool {
        let Some(document) = self.documents.get_mut(self.active) else {
            return false;
        };
        match document.submit(Command::EditFeature { feature, edit }) {
            Ok(applied) => applied,
            Err(reason) => {
                self.session.notice = Some(reason);
                false
            }
        }
    }

    /// OK: the draft becomes one `AddFeature`, in a new part when the
    /// document has none (`Group`, one undo entry), at the end of the part
    /// as a sketch is placed. A draft that cannot be added stays open, and
    /// the status line says why.
    pub(crate) fn form_ok(&mut self) {
        // An edit form commits as it goes: Enter only closes it.
        if self
            .active_document()
            .is_some_and(|d| d.form.as_ref().is_some_and(|f| f.edit.is_some()))
        {
            self.form_cancel();
            return;
        }
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
