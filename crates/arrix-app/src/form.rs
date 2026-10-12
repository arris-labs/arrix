//! The property card's forms, from the app's side (docs/UI-RENDERING.md
//! §Declarative plugin UI, ADR-0018). A form is derived from a feature
//! type's spec and nothing else. A create form is a draft, session state
//! on the document's tab: its fields change the draft and issue nothing,
//! OK is one `AddFeature` (in a `Group` with `AddPart` when the document
//! has no part), and Cancel issues none. An edit form is the same fields
//! over an existing feature's record: it is read from the replica each
//! frame, and a field commit or a choice is one `EditFeature`.

use std::collections::BTreeMap;

use arrix_core::{Diagnostic, FeatureId, Id, IdMinter, Input, PartId, QuantityKind, Ref};
use arrix_doc::expr::Expr;
use arrix_doc::{Command, Document, FeatureEdit, FeatureRecord, FeatureTypeId, Part};
use arrix_plugin_api::{
    ChoiceOption, ChoiceSpec, FeatureTypeSpec, InputKind, InputSpec, ParamSpec, ShownWhen,
};
use arrix_ui::{
    ChoiceOptionView, FieldValue, FormDiagnostic, FormField, FormView, ReferenceValueView, UiIntent,
};

use crate::ArrixApp;
use crate::documents::{OpenDocument, Standing, standing_of};

/// A form being filled: a create form's draft, or an edit form over a
/// feature's record.
#[derive(Clone, Debug)]
pub struct Form {
    /// The feature an edit form edits; none for a create form.
    pub(crate) edit: Option<FeatureId>,
    /// An edit form's name as committed, until the record says it.
    renamed: Option<String>,
    /// Names the form for the renderer's text buffers; a new form has a
    /// new one.
    id: String,
    pub(crate) spec: FeatureTypeSpec,
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
    /// A create form's references, by input. An edit form's are the
    /// record's.
    pub(crate) inputs: BTreeMap<String, Input>,
    /// The reference field a pick fills.
    pub(crate) active: Option<String>,
    /// A create form's feature id, and its part's when the document has
    /// none, minted when first needed: the preview's and OK's are the same.
    feature: Option<Id>,
    part: Option<Id>,
    /// An edit form's feature type is not available: the form shows the
    /// record's stored values by name, read only.
    absent: bool,
    /// The references the feature's diagnostic last called lost: when they
    /// change, the field holding one becomes the active one.
    lost_seen: Vec<Ref>,
}

/// What a quantity of `kind` is measured in when its text names no unit.
fn unit_hint(kind: QuantityKind) -> &'static str {
    match kind {
        QuantityKind::Length => "mm",
        QuantityKind::Angle => "deg",
        QuantityKind::Mass => "kg",
        QuantityKind::Density => "kg/m3",
        QuantityKind::Count | QuantityKind::Ratio => "",
    }
}

/// The references a lost-reference diagnostic is about; none for any other.
pub(crate) fn lost_refs(d: &Diagnostic) -> &[Ref] {
    if d.code.to_string().starts_with("ref.lost") {
        &d.refs
    } else {
        &[]
    }
}

/// Where the feature stands, with what evaluation said of it.
pub(crate) fn standing(document: &OpenDocument, feature: FeatureId) -> Option<Standing> {
    let doc = document.replica.document();
    doc.parts().values().find_map(|part| {
        let index = part.history.iter().position(|id| *id == feature)?;
        let record = part.features.get(&feature)?;
        Some(standing_of(&document.replica, part, index, record))
    })
}

/// How many of a lost reference's candidates its form lists.
pub(crate) const CANDIDATES_SHOWN: usize = 8;

/// What an input takes, in words, for a field that is asking.
pub(crate) fn takes(kind: InputKind) -> &'static str {
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
            inputs: BTreeMap::new(),
            active: None,
            feature: None,
            part: None,
            absent: false,
            lost_seen: Vec::new(),
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
            inputs: BTreeMap::new(),
            active: None,
            feature: None,
            part: None,
            absent: false,
            lost_seen: Vec::new(),
        }
    }

    /// An edit form over `record` whose type is not available: a spec
    /// made of the names the record stores, so its values show read only.
    pub fn edit_stored(record: &FeatureRecord) -> Self {
        let spec = FeatureTypeSpec {
            id: record.type_id.to_string(),
            version: record.type_version,
            title: record.type_id.to_string(),
            params: record
                .params
                .keys()
                .map(|name| ParamSpec {
                    name: name.clone(),
                    title: name.clone(),
                    kind: QuantityKind::Count,
                    default: String::new(),
                    shown_when: None,
                })
                .collect(),
            choices: record
                .choices
                .iter()
                .map(|(name, value)| ChoiceSpec {
                    name: name.clone(),
                    title: name.clone(),
                    options: vec![ChoiceOption {
                        name: value.clone(),
                        title: value.clone(),
                    }],
                    default: None,
                })
                .collect(),
            inputs: record
                .inputs
                .iter()
                .map(|(name, input)| InputSpec {
                    name: name.clone(),
                    title: name.clone(),
                    kind: InputKind::Tool,
                    list: matches!(input, Input::Many(_)),
                    shown_when: None,
                })
                .collect(),
            outputs: Vec::new(),
        };
        let mut form = Self::edit(&spec, record.id);
        form.absent = true;
        form
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
    pub fn view(
        &self,
        record: Option<&FeatureRecord>,
        label: &dyn Fn(&Ref) -> String,
        standing: Option<&Standing>,
    ) -> FormView {
        let diagnostic = standing.and_then(|s| s.diagnostic.as_ref());
        let lost = diagnostic.map_or(&[][..], lost_refs);
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
            let values: Vec<_> = self
                .held(record)
                .get(&i.name)
                .map(|input| {
                    input
                        .refs()
                        .iter()
                        .map(|r| ReferenceValueView {
                            label: label(r),
                            lost: lost.contains(r),
                        })
                        .collect()
                })
                .unwrap_or_default();
            fields.push(FormField {
                name: i.name.clone(),
                title: i.title.clone(),
                value: FieldValue::Reference {
                    takes: if self.absent {
                        String::new()
                    } else {
                        takes(i.kind).to_owned()
                    },
                    candidates: if values.iter().any(|v| v.lost) {
                        diagnostic.map_or_else(Vec::new, |d| {
                            d.candidates
                                .iter()
                                .take(CANDIDATES_SHOWN)
                                .enumerate()
                                .map(|(n, r)| format!("{}. {}", n + 1, label(r)))
                                .collect()
                        })
                    } else {
                        Vec::new()
                    },
                    values: values.clone(),
                    list: i.list,
                    active: self.active.as_deref() == Some(i.name.as_str()),
                },
                message: values.iter().any(|v| v.lost).then(|| {
                    match diagnostic.map_or(0, |d| d.candidates.len()) {
                        0 => "lost: nothing like it is left; pick another".to_owned(),
                        _ => "lost: pick one of the highlighted candidates".to_owned(),
                    }
                }),
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
            state: standing.map(|s| s.state),
            diagnostic: diagnostic.map(|d| FormDiagnostic {
                code: d.code.to_string(),
                message: d.message.clone(),
            }),
            read_only: self.read_only(record, standing),
            fields,
        }
    }

    /// Why an edit form's values are read only: the feature shows a stored
    /// result, or its type is not available.
    fn read_only(
        &self,
        record: Option<&FeatureRecord>,
        standing: Option<&Standing>,
    ) -> Option<String> {
        let id = &self.spec.id;
        let frozen = standing.and_then(|s| s.frozen);
        let mut why = match frozen {
            Some(info) => crate::documents::frozen_reason(id, &info),
            None if self.absent => format!("Feature type {id} is not available"),
            None => return None,
        };
        why.push_str("; showing the stored result and values, read only.");
        if frozen.is_some_and(|f| f.stale) && record.is_some() {
            why.push_str(" Its inputs differ from the stored result's.");
        }
        Some(why)
    }

    /// The references held: the record's, else the draft's.
    pub(crate) fn held<'a>(
        &'a self,
        record: Option<&'a FeatureRecord>,
    ) -> &'a BTreeMap<String, Input> {
        record.map_or(&self.inputs, |r| &r.inputs)
    }

    /// The reference fields the choices leave shown, in the spec's order.
    pub(crate) fn shown_inputs<'a>(
        &'a self,
        record: Option<&'a FeatureRecord>,
    ) -> impl Iterator<Item = &'a arrix_plugin_api::InputSpec> {
        self.spec
            .inputs
            .iter()
            .filter(move |i| self.shown(record, &i.shown_when))
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
            inputs: self.inputs.clone(),
            suppressed: false,
            sketch: None,
            frozen: None,
            file: None,
        })
    }
}

impl Form {
    /// The draft's feature, once its id is minted.
    pub(crate) fn draft_feature(&self) -> Option<FeatureId> {
        self.feature.map(FeatureId)
    }

    /// The command OK submits on `doc`: one `AddFeature` at the end of the
    /// part in view (`target`; else the first part), or, in a document with
    /// no part, a `Group` that adds one with it. Or why the draft cannot be
    /// added.
    pub(crate) fn command(
        &mut self,
        doc: &Document,
        ids: &mut IdMinter,
        target: Option<PartId>,
    ) -> Result<Command, String> {
        let feature = FeatureId(crate::preview::minted(&mut self.feature, ids));
        let record = self.record(feature)?;
        let part = target
            .and_then(|p| doc.parts().get(&p))
            .or_else(|| doc.parts().values().next());
        Ok(match part {
            Some(part) => Command::AddFeature {
                part: part.id,
                at: part.history.len(),
                record,
            },
            None => {
                let part = PartId(crate::preview::minted(&mut self.part, ids));
                Command::Group {
                    label: format!("Create {}", self.spec.title.to_lowercase()),
                    commands: vec![
                        Command::AddPart {
                            part: Part {
                                id: part,
                                name: "Part 1".into(),
                                history: Vec::new(),
                                features: BTreeMap::new(),
                                rollback: None,
                                material: None,
                                body_materials: Default::default(),
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
        })
    }
}

/// The record of `feature` in the document the tab holds.
pub(crate) fn feature_record(
    document: &OpenDocument,
    feature: FeatureId,
) -> Option<&FeatureRecord> {
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
        let mut form = Form::create(id, spec, name, params);
        let first = form.shown_inputs(None).next().map(|i| i.name.clone());
        form.active = first;
        if let Some(document) = self.documents.get_mut(self.active) {
            document.component_form = None;
            document.joint_form = None;
            document.form = Some(form);
        }
        self.seed_form();
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
        let form = match self
            .feature_types
            .iter()
            .find(|t| t.id == record.type_id.as_str())
        {
            Some(spec) => Form::edit(spec, feature),
            None => Form::edit_stored(record),
        };
        if let Some(document) = self.documents.get_mut(self.active) {
            document.component_form = None;
            document.joint_form = None;
            document.form = Some(form);
        }
    }

    /// The form the property card draws, if one is open.
    pub(crate) fn form_view(&self) -> Option<FormView> {
        if self.component_form_open() {
            return self.component_form_view();
        }
        if self.joint_form_open() {
            return self.joint_form_view();
        }
        let document = self.active_document()?;
        let form = document.form.as_ref()?;
        let doc = document.replica.document();
        let label = |r: &Ref| crate::form_pick::label(doc, r);
        match form.edit {
            Some(feature) => {
                let record = feature_record(document, feature)?;
                Some(form.view(Some(record), &label, standing(document, feature).as_ref()))
            }
            None => {
                let mut view = form.view(None, &label, None);
                if let Some(feature) = form.draft_feature() {
                    (view.state, view.diagnostic) = document.preview.header(feature);
                }
                Some(view)
            }
        }
    }

    /// Keeps each tab's edit form in step with its document: one whose
    /// feature an undo took away closes, and a typed text or name the
    /// record now says is dropped, so the record is what shows.
    pub(crate) fn follow_forms(&mut self) {
        self.follow_component_forms();
        self.follow_joint_forms();
        for document in &mut self.documents {
            let Some(feature) = document.form.as_ref().and_then(|f| f.edit) else {
                continue;
            };
            let Some(record) = feature_record(document, feature).cloned() else {
                document.form = None;
                continue;
            };
            let lost: Vec<Ref> = standing(document, feature)
                .and_then(|s| s.diagnostic)
                .map(|d| lost_refs(&d).to_vec())
                .unwrap_or_default();
            if let Some(form) = &mut document.form {
                // A reference that has just been lost asks to be re-picked:
                // its field becomes the one a pick fills.
                if form.lost_seen != lost {
                    if !form.absent
                        && let Some(field) = record.inputs.iter().find_map(|(name, input)| {
                            input
                                .refs()
                                .iter()
                                .any(|r| lost.contains(r))
                                .then(|| name.clone())
                        })
                    {
                        form.active = Some(field);
                    }
                    form.lost_seen = lost;
                }
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
        self.active_document().is_some_and(|d| {
            d.form.is_some() || d.component_form.is_some() || d.joint_form.is_some()
        })
    }

    /// What the user did in the open form.
    pub(crate) fn form_intent(&mut self, intent: UiIntent) {
        if self.component_form_open() {
            return self.component_form_intent(intent);
        }
        if self.joint_form_open() {
            return self.joint_form_intent(intent);
        }
        // A form that shows a stored result takes no edit; it can be closed.
        if self.form_read_only() && !matches!(intent, UiIntent::FormOk | UiIntent::FormCancelled) {
            return;
        }
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
            UiIntent::ReferenceRemoved { field, index } => self.remove_reference(&field, index),
            UiIntent::CandidatePicked { field, index } => self.pick_candidate(&field, index),
            UiIntent::FormOk => self.form_ok(),
            UiIntent::FormCancelled => self.form_cancel(),
            _ => {}
        }
    }

    /// The open edit form shows a stored result, or a type that is not
    /// available.
    fn form_read_only(&self) -> bool {
        let Some(document) = self.active_document() else {
            return false;
        };
        let Some(form) = document.form.as_ref() else {
            return false;
        };
        form.absent
            || form
                .edit
                .and_then(|f| standing(document, f))
                .is_some_and(|s| s.frozen.is_some())
    }

    pub(crate) fn form_mut(&mut self) -> Option<&mut Form> {
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
    pub(crate) fn edit_feature(&mut self, feature: FeatureId, edit: FeatureEdit) -> bool {
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
        if self.component_form_open() {
            return self.component_form_ok();
        }
        if self.joint_form_open() {
            return self.joint_form_ok();
        }
        // An edit form commits as it goes: Enter only closes it.
        if self
            .active_document()
            .is_some_and(|d| d.form.as_ref().is_some_and(|f| f.edit.is_some()))
        {
            self.form_cancel();
            return;
        }
        let Some(document) = self.documents.get_mut(self.active) else {
            return;
        };
        let target = document.part_in_view();
        let Some(form) = document.form.as_mut() else {
            return;
        };
        let command = match form.command(document.replica.document(), &mut self.ids, target) {
            Ok(command) => command,
            Err(reason) => {
                self.session.notice = Some(reason);
                return;
            }
        };
        let feature = form.draft_feature();
        match document.submit(command) {
            Ok(_) => {
                document.form = None;
                document.clear_preview();
                document.view.feature_selection = feature;
            }
            Err(reason) => self.session.notice = Some(reason),
        }
    }

    /// Cancel, or Escape: the draft goes. No command.
    pub(crate) fn form_cancel(&mut self) {
        if self.component_form_open() {
            return self.component_form_cancel();
        }
        if self.joint_form_open() {
            return self.joint_form_cancel();
        }
        if let Some(document) = self.documents.get_mut(self.active) {
            document.form = None;
            document.clear_preview();
        }
    }
}
