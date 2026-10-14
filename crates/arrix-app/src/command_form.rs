//! A plugin command's form in the property card (ADR-0038 §10, docs/
//! UI-RENDERING.md §Declarative plugin UI): the same renderer over the
//! command's spec. The form is a draft, session state on the document's tab.
//! Its fields change the draft and issue nothing; OK sends `Expand` and, when
//! the group comes back, submits it as one gesture; a diagnostic from either
//! shows under the form and keeps the draft; Cancel and Escape drop it.
//! Picks in an assembly's view carry their component path.

use std::collections::BTreeMap;

use arrix_core::{Diagnostic, Id};
use arrix_doc::{Command, EdgePick, ExpandContext, ExpandRequest, Generation};
use arrix_plugin_api::{CommandSpec, InputKind};
use arrix_ui::{
    ChoiceOptionView, FieldValue, FormDiagnostic, FormField, FormView, ReferenceValueView, UiIntent,
};
use arrix_viewport::Hit;

use crate::ArrixApp;
use crate::context::Context;
use crate::documents::OpenDocument;
use crate::form::unit_hint;

/// How many ids an expansion is given when its spec names no `max-new`.
const DEFAULT_IDS: u32 = 64;

/// A command form being filled.
#[derive(Clone, Debug)]
pub struct CommandForm {
    /// Names the form for the renderer's text buffers.
    id: String,
    pub(crate) spec: CommandSpec,
    context: ExpandContext,
    /// The parameters' texts the user typed; the rest hold the spec's
    /// default, which the plugin's host applies.
    texts: BTreeMap<String, String>,
    choices: BTreeMap<String, String>,
    /// The picks, by input, in the order made.
    pub(crate) picks: BTreeMap<String, Vec<EdgePick>>,
    /// The input field a pick fills.
    pub(crate) active: Option<String>,
    /// The generation OK expanded on, while the answer is awaited.
    pub(crate) expanding: Option<Generation>,
    /// Why the last expansion or submit was refused.
    failure: Option<Diagnostic>,
}

impl CommandForm {
    pub fn new(id: String, spec: &CommandSpec, context: ExpandContext) -> Self {
        Self {
            id,
            spec: spec.clone(),
            context,
            texts: BTreeMap::new(),
            choices: BTreeMap::new(),
            picks: BTreeMap::new(),
            active: spec.inputs.first().map(|i| i.name.clone()),
            expanding: None,
            failure: None,
        }
    }

    /// Whether the form's context is `context`.
    pub(crate) fn runs_in(&self, context: Context) -> bool {
        match (self.context, context) {
            (ExpandContext::Assembly(a), Context::Assembly(b)) => a == b,
            (ExpandContext::Part(a), Context::Part(b)) => a == b,
            _ => false,
        }
    }

    /// Every pick the form holds, as the viewport marks them.
    pub(crate) fn marks(&self) -> impl Iterator<Item = &EdgePick> {
        self.picks.values().flatten()
    }

    /// `hit` into the active input: a list input takes it (or, when it is
    /// there, drops it), a single one is replaced.
    fn pick(&mut self, hit: Hit, extend: bool) -> Result<(), String> {
        let Some(field) = self.active.clone() else {
            return Ok(());
        };
        let Some(input) = self.spec.inputs.iter().find(|i| i.name == field) else {
            return Ok(());
        };
        if hit.name.kind != arrix_core::TopoKind::Edge {
            return Err(format!(
                "{}: the {} field takes an edge.",
                self.spec.title, input.title
            ));
        }
        let pick = EdgePick {
            path: hit.path,
            edge: hit.name,
        };
        let held = self.picks.entry(field).or_default();
        match held.iter().position(|p| *p == pick) {
            Some(at) if input.list => {
                held.remove(at);
            }
            _ if input.list => {
                if !extend {
                    held.clear();
                }
                held.push(pick);
            }
            _ => *held = vec![pick],
        }
        self.failure = None;
        Ok(())
    }

    fn view(&self, document: &OpenDocument) -> FormView {
        let doc = document.replica.document();
        let mut fields = Vec::new();
        for c in &self.spec.choices {
            let selected = self
                .choices
                .get(&c.name)
                .cloned()
                .or_else(|| c.default.clone());
            fields.push(FormField {
                name: c.name.clone(),
                title: c.title.clone(),
                value: FieldValue::Choice {
                    selected,
                    options: c
                        .options
                        .iter()
                        .map(|o| ChoiceOptionView {
                            name: o.name.clone(),
                            title: o.title.clone(),
                        })
                        .collect(),
                    clearable: false,
                },
                message: None,
            });
        }
        for i in &self.spec.inputs {
            let values = self
                .picks
                .get(&i.name)
                .into_iter()
                .flatten()
                .map(|p| ReferenceValueView {
                    label: pick_label(document, doc, p),
                    lost: false,
                })
                .collect();
            fields.push(FormField {
                name: i.name.clone(),
                title: i.title.clone(),
                value: FieldValue::Reference {
                    takes: crate::form::takes(InputKind::Edge).to_owned(),
                    values,
                    list: i.list,
                    active: self.active.as_deref() == Some(i.name.as_str()),
                    candidates: Vec::new(),
                },
                message: None,
            });
        }
        for p in &self.spec.params {
            fields.push(FormField {
                name: p.name.clone(),
                title: p.title.clone(),
                value: FieldValue::Quantity {
                    text: self
                        .texts
                        .get(&p.name)
                        .cloned()
                        .unwrap_or_else(|| p.default.clone()),
                    unit: unit_hint(p.kind).to_owned(),
                },
                message: None,
            });
        }
        FormView {
            id: self.id.clone(),
            title: self.spec.title.clone(),
            create: true,
            name: String::new(),
            named: false,
            state: None,
            diagnostic: self.failure.as_ref().map(|d| FormDiagnostic {
                code: d.code.to_string(),
                message: d.message.clone(),
            }),
            read_only: None,
            fields,
        }
    }
}

/// A pick in words: the edge, and the component it is of in an assembly.
fn pick_label(document: &OpenDocument, doc: &arrix_doc::Document, pick: &EdgePick) -> String {
    let edge = crate::form_pick::label(doc, &arrix_core::Ref::Topo(pick.edge.clone()));
    match document.component_label(&pick.path) {
        Some(component) => format!("{edge}, {component}"),
        None => edge,
    }
}

impl ArrixApp {
    /// A plugin's command (`<plugin>.<name>`): opens its form in the
    /// property card, the selected edge seeding the first input it fits.
    /// No command.
    pub(crate) fn open_command_form(&mut self, command: &str) {
        if self.active_document().is_some_and(|d| d.sketch.is_some()) {
            self.session.notice = Some("Finish the sketch first.".into());
            return;
        }
        let Some(spec) = self.plugin_commands.iter().find(|c| c.id == command) else {
            return;
        };
        let Some(document) = self.documents.get(self.active) else {
            return;
        };
        let context = match document.context() {
            Some(Context::Assembly(assembly)) => ExpandContext::Assembly(assembly),
            Some(Context::Part(part)) => ExpandContext::Part(part),
            None => return,
        };
        self.session.forms_opened += 1;
        let id = format!("command:{command}:{}", self.session.forms_opened);
        let mut form = CommandForm::new(id, spec, context);
        if let Some(hit) = document.view.selection.clone() {
            // The selection is the first pick; the next is asked for
            // after a single input is filled.
            let _ = form.pick(hit, true);
            if form.spec.inputs.first().is_some_and(|i| !i.list) {
                form.active = None;
            }
        }
        if let Some(document) = self.documents.get_mut(self.active) {
            document.form = None;
            document.component_form = None;
            document.joint_form = None;
            document.material_form = None;
            document.clear_preview();
            document.command_form = Some(form);
        }
    }

    pub(crate) fn command_form_open(&self) -> bool {
        self.active_document()
            .is_some_and(|d| d.command_form.is_some())
    }

    pub(crate) fn command_form_view(&self) -> Option<FormView> {
        let document = self.active_document()?;
        Some(document.command_form.as_ref()?.view(document))
    }

    /// Whether the open command form is asking for a pick.
    pub(crate) fn command_field_asks(&self) -> bool {
        self.active_document()
            .and_then(|d| d.command_form.as_ref())
            .is_some_and(|f| f.active.is_some())
    }

    /// A click in the scene while the command form asks. True when it did,
    /// so the click selects nothing.
    pub(crate) fn command_pick(&mut self, hit: Option<Hit>) -> bool {
        let Some(document) = self.documents.get_mut(self.active) else {
            return false;
        };
        let extend = document.view.click_extend;
        let Some(form) = document.command_form.as_mut() else {
            return false;
        };
        if let Some(hit) = hit
            && let Err(reason) = form.pick(hit, extend)
        {
            self.session.notice = Some(reason);
        }
        true
    }

    /// Everything the open command form holds that is a name in the scene.
    pub(crate) fn command_marks(&self) -> Vec<arrix_core::PersistentName> {
        self.active_document()
            .and_then(|d| d.command_form.as_ref())
            .map(|f| f.marks().map(|p| p.edge.clone()).collect())
            .unwrap_or_default()
    }

    /// What the user did in the open command form.
    pub(crate) fn command_form_intent(&mut self, intent: UiIntent) {
        let Some(form) = self
            .documents
            .get_mut(self.active)
            .and_then(|d| d.command_form.as_mut())
        else {
            return;
        };
        match intent {
            UiIntent::FieldEdited { field, text } | UiIntent::FieldCommitted { field, text } => {
                form.texts.insert(field, text);
            }
            UiIntent::ChoiceSelected { field, value } => {
                match value {
                    Some(value) => form.choices.insert(field, value),
                    None => form.choices.remove(&field),
                };
            }
            UiIntent::ReferenceFieldActivated { field } => form.active = Some(field),
            UiIntent::ReferenceRemoved { field, index } => {
                if let Some(held) = form.picks.get_mut(&field)
                    && index < held.len()
                {
                    held.remove(index);
                }
            }
            UiIntent::FormOk => self.command_form_ok(),
            UiIntent::FormCancelled => self.command_form_cancel(),
            _ => {}
        }
    }

    /// OK: asks the session to expand the command on the newest
    /// generation. The group comes back as an event, and
    /// [`Self::follow_expansions`] submits it.
    pub(crate) fn command_form_ok(&mut self) {
        let Some(document) = self.documents.get_mut(self.active) else {
            return;
        };
        let base = document.replica.generation();
        let Some(form) = document.command_form.as_mut() else {
            return;
        };
        if form.expanding.is_some() {
            return;
        }
        let ids = (0..form.spec.max_new.unwrap_or(DEFAULT_IDS))
            .map(|_| -> Id { self.ids.next_id() })
            .collect();
        let request = ExpandRequest {
            command: form.spec.id.clone(),
            context: form.context,
            params: form.texts.clone(),
            choices: form.choices.clone(),
            inputs: form
                .picks
                .iter()
                .map(|(name, picks)| (name.clone(), picks.clone()))
                .collect(),
            ids,
            base,
        };
        match document.expand(request) {
            Ok(()) => {
                if let Some(form) = document.command_form.as_mut() {
                    form.expanding = Some(base);
                    form.failure = None;
                }
            }
            Err(reason) => self.session.notice = Some(reason),
        }
    }

    pub(crate) fn command_form_cancel(&mut self) {
        if let Some(document) = self.documents.get_mut(self.active) {
            document.command_form = None;
            document.refusal = None;
        }
    }

    /// Takes in the groups the sessions expanded since the last frame: a
    /// group is submitted as one gesture and closes its form; a refusal
    /// stays under the form, which keeps its draft.
    pub(crate) fn follow_expansions(&mut self) {
        for index in 0..self.documents.len() {
            let document = &mut self.documents[index];
            let Some(answer) = document.take_expanded() else {
                continue;
            };
            let Some(form) = document
                .command_form
                .as_mut()
                .filter(|f| f.expanding.is_some())
            else {
                continue;
            };
            form.expanding = None;
            match answer {
                Err(diagnostic) => form.failure = Some(diagnostic),
                Ok(command) => {
                    let previous = std::mem::replace(&mut self.active, index);
                    self.submit_expanded(command);
                    self.active = previous;
                }
            }
        }
    }

    /// The expanded group, submitted as one gesture.
    fn submit_expanded(&mut self, command: Command) {
        let what = "command submit";
        self.gesture(what, |app| {
            let Some(document) = app.documents.get_mut(app.active) else {
                return;
            };
            match document.submit(command) {
                Ok(_) => document.command_form = None,
                Err(reason) => {
                    let refusal = document.refusal.clone();
                    if let Some(form) = document.command_form.as_mut() {
                        form.failure = refusal;
                    }
                    app.session.notice = Some(reason);
                }
            }
        });
    }
}
