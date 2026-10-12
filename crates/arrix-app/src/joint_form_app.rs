//! The app's side of the joint form (ADR-0030 §5): opening it, the picks
//! that fill its anchors, and the commands it commits. The form itself is
//! `joint_form.rs`.

use arrix_core::{AssemblyId, ComponentId, Diagnostic, JointId, PersistentName, Ref};
use arrix_doc::{Anchor, Command, Joint, JointKind, JointOutcome};
use arrix_ui::{FormView, UiIntent};
use arrix_viewport::Hit;

use crate::ArrixApp;
use crate::context::Context;
use crate::documents::OpenDocument;
use crate::joint_form::{
    CANDIDATES_SHOWN, JointForm, Side, Slot, free_name, kind_of_word, kind_word, quantity,
};

/// The failure of a joint's last solve.
fn failure(document: &OpenDocument, form: &JointForm) -> Option<Diagnostic> {
    let joint = form.edit?;
    let (_, outcome) = document.replica.assembly(form.assembly)?;
    match outcome.joints.get(&joint)? {
        JointOutcome::Failed { diagnostic } => Some(diagnostic.clone()),
        JointOutcome::Ok { .. } => None,
    }
}

/// `anchor` with `name` at `slot`, on the component at `path`.
fn placed(
    old: Option<&Anchor>,
    slot: Slot,
    path: Vec<ComponentId>,
    name: PersistentName,
) -> Result<Anchor, String> {
    match slot {
        Slot::Axis(_) => Ok(Anchor {
            origin: old
                .filter(|a| a.path == path)
                .and_then(|a| a.origin.clone()),
            path,
            axis: name,
        }),
        Slot::Origin(_) => {
            let old = old.ok_or("pick the axis first")?;
            if old.path != path {
                return Err("the origin is on the same component as the axis".into());
            }
            Ok(Anchor {
                origin: Some(name),
                ..old.clone()
            })
        }
    }
}

impl ArrixApp {
    /// `core.assembly.joint.<kind>`: opens a draft of a joint in the
    /// assembly in view; what is selected in the scene is its parent. No
    /// command.
    pub(crate) fn open_joint_create(&mut self, kind: JointKind) {
        let Some(document) = self.documents.get(self.active) else {
            return;
        };
        let Some(Context::Assembly(assembly)) = document.context() else {
            return;
        };
        let name = free_name(document.replica.document(), assembly, kind_word(kind));
        self.session.forms_opened += 1;
        let id = format!("new:joint:{}", self.session.forms_opened);
        let mut form = JointForm::create(id, assembly, name, kind);
        if let Some(hit) = document.view.selection.clone()
            && !hit.path.is_empty()
            && Slot::Axis(Side::Parent).fits(hit.name.kind)
        {
            form.parent = placed(None, Slot::Axis(Side::Parent), hit.path, hit.name).ok();
            form.active = Some(Slot::Axis(Side::Child));
        }
        if let Some(document) = self.documents.get_mut(self.active) {
            document.form = None;
            document.component_form = None;
            document.material_form = None;
            document.clear_preview();
            document.joint_form = Some(form);
        }
    }

    /// A double-click on a joint row: its form from the record. No command.
    pub(crate) fn open_joint_edit(&mut self, assembly: &str, joint: &str) {
        let (Ok(assembly), Ok(joint)) = (assembly.parse::<AssemblyId>(), joint.parse::<JointId>())
        else {
            return;
        };
        let Some(document) = self.documents.get_mut(self.active) else {
            return;
        };
        let known = document
            .replica
            .document()
            .assemblies()
            .get(&assembly)
            .is_some_and(|a| a.joints.contains_key(&joint));
        if known {
            document.form = None;
            document.component_form = None;
            document.material_form = None;
            document.clear_preview();
            document.joint_form = Some(JointForm::edit(assembly, joint));
        }
    }

    pub(crate) fn joint_form_open(&self) -> bool {
        self.active_document()
            .is_some_and(|d| d.joint_form.is_some())
    }

    /// The joint form the property card draws, if one is open.
    pub(crate) fn joint_form_view(&self) -> Option<FormView> {
        let document = self.active_document()?;
        let form = document.joint_form.as_ref()?;
        let component = |path: &[ComponentId]| document.component_label(path);
        form.view(
            document.replica.document(),
            failure(document, form).as_ref(),
            document.refusal.as_ref(),
            &component,
        )
    }

    /// Keeps each tab's joint form in step with its document: an edit form
    /// whose joint an undo took away closes, and one whose anchor has just
    /// been lost asks for it to be picked again.
    pub(crate) fn follow_joint_forms(&mut self) {
        for document in &mut self.documents {
            let Some(form) = document.joint_form.as_ref() else {
                continue;
            };
            if form.edit.is_some() && form.record(document.replica.document()).is_none() {
                document.joint_form = None;
                continue;
            }
            let lost = JointForm::lost(failure(document, form).as_ref());
            let record = form.record(document.replica.document()).cloned();
            let Some(form) = document.joint_form.as_mut() else {
                continue;
            };
            if form.lost_seen == lost {
                continue;
            }
            let slot = Slot::ALL.into_iter().find(|s| {
                form.held(record.as_ref(), *s)
                    .is_some_and(|n| lost.contains(&Ref::Topo(n.clone())))
            });
            if slot.is_some() {
                form.active = slot;
            }
            form.lost_seen = lost;
        }
    }

    fn joint_form_mut(&mut self) -> Option<&mut JointForm> {
        self.documents.get_mut(self.active)?.joint_form.as_mut()
    }

    fn joint_editing(&self) -> bool {
        self.active_document()
            .and_then(|d| d.joint_form.as_ref())
            .is_some_and(|f| f.edit.is_some())
    }

    /// Whether a field of the open joint form asks for a pick.
    pub(crate) fn joint_field_asks(&self) -> bool {
        self.active_document()
            .and_then(|d| d.joint_form.as_ref())
            .is_some_and(|f| f.active.is_some())
    }

    /// What the user did in the open joint form.
    pub(crate) fn joint_form_intent(&mut self, intent: UiIntent) {
        let editing = self.joint_editing();
        match intent {
            UiIntent::FieldEdited { field, text } if !editing => {
                if let Some(form) = self.joint_form_mut() {
                    form.texts.insert(field, text);
                }
            }
            UiIntent::FieldCommitted { field, text } => self.commit_number(&field, text),
            UiIntent::NameCommitted { text } => {
                let text = text.trim().to_owned();
                if text.is_empty() {
                    return;
                }
                if editing {
                    self.edit_joint(|j| j.name = text.clone());
                } else if let Some(form) = self.joint_form_mut() {
                    form.name = text;
                    form.named = true;
                }
            }
            UiIntent::ChoiceSelected {
                field,
                value: Some(value),
            } => self.choose_joint(&field, &value),
            UiIntent::ReferenceFieldActivated { field } => {
                if let (Some(slot), Some(form)) = (Slot::of_field(&field), self.joint_form_mut()) {
                    form.active = Some(slot);
                }
            }
            UiIntent::ReferenceRemoved { field, .. } => self.remove_origin(&field),
            UiIntent::CandidatePicked { field, index } => self.take_candidate(&field, index),
            UiIntent::FormOk => self.joint_form_ok(),
            UiIntent::FormCancelled => self.joint_form_cancel(),
            _ => {}
        }
    }

    /// A choice: the kind, or flip.
    fn choose_joint(&mut self, field: &str, value: &str) {
        let editing = self.joint_editing();
        match field {
            "kind" => {
                let Some(kind) = kind_of_word(value) else {
                    return;
                };
                if editing {
                    self.edit_joint(|j| j.kind = kind);
                } else if let Some(form) = self.joint_form_mut() {
                    form.kind = kind;
                    form.texts.clear();
                    form.refused.clear();
                    if !form.named {
                        form.name = String::new();
                    }
                }
                self.rename_unnamed();
            }
            "flip" => {
                let flip = value == "yes";
                if editing {
                    self.edit_joint(|j| j.flip = flip);
                } else if let Some(form) = self.joint_form_mut() {
                    form.flip = flip;
                }
            }
            _ => {}
        }
    }

    /// A create form whose name the user did not type follows its kind.
    fn rename_unnamed(&mut self) {
        let Some(document) = self.documents.get_mut(self.active) else {
            return;
        };
        let Some(form) = document.joint_form.as_mut() else {
            return;
        };
        if !form.named && form.edit.is_none() {
            form.name = free_name(
                document.replica.document(),
                form.assembly,
                kind_word(form.kind),
            );
        }
    }

    /// A number's text was committed: checked as a quantity of the joint's
    /// kind (a limit may be left empty); an edit form's valid one is one
    /// `EditJoint`.
    fn commit_number(&mut self, field: &str, text: String) {
        let Some(qkind) = self.number_kind() else {
            return;
        };
        let empty = text.trim().is_empty() && matches!(field, "lower" | "upper");
        let verdict = if empty {
            Ok(())
        } else {
            self.evaluate_text(&text, qkind)
                .map(|_| ())
                .map_err(|e| format!("{}: {e}", e.code()))
        };
        let editing = self.joint_editing();
        let Some(form) = self.joint_form_mut() else {
            return;
        };
        match &verdict {
            Ok(()) => form.refused.remove(field),
            Err(why) => form.refused.insert(field.to_owned(), why.clone()),
        };
        form.texts.insert(field.to_owned(), text);
        if editing && verdict.is_ok() {
            self.commit_numbers();
        }
    }

    /// The quantity kind the open form's numbers are in: an edit form's
    /// joint's, else the draft's.
    fn number_kind(&self) -> Option<arrix_core::QuantityKind> {
        let document = self.active_document()?;
        let form = document.joint_form.as_ref()?;
        let kind = form.kind_now(form.record(document.replica.document()));
        quantity(kind).map(|q| q.0)
    }

    /// An edit form's numbers, as the texts say them: one `EditJoint`.
    fn commit_numbers(&mut self) {
        let Some(document) = self.active_document() else {
            return;
        };
        let Some(form) = document.joint_form.as_ref() else {
            return;
        };
        let Some(record) = form.record(document.replica.document()) else {
            return;
        };
        let mut joint = record.clone();
        match form.numbers(&mut joint) {
            Ok(()) => self.edit_joint(|j| {
                j.value = joint.value.clone();
                j.offset = joint.offset.clone();
                j.limits = joint.limits.clone();
            }),
            Err(reason) => self.session.notice = Some(reason),
        }
    }

    /// One `EditJoint` of the open edit form's joint, changed by `change`;
    /// none when it changes nothing. The typed texts are dropped once the
    /// record says them.
    fn edit_joint(&mut self, change: impl FnOnce(&mut Joint)) {
        let Some(document) = self.documents.get_mut(self.active) else {
            return;
        };
        let Some(form) = document.joint_form.as_ref() else {
            return;
        };
        let Some(record) = form.record(document.replica.document()) else {
            return;
        };
        let assembly = form.assembly;
        let mut joint = record.clone();
        change(&mut joint);
        if joint == *record {
            return;
        }
        let command = Command::EditJoint {
            assembly,
            joint: Box::new(joint),
        };
        match document.submit(command) {
            Ok(_) => {
                if let Some(form) = document.joint_form.as_mut() {
                    form.texts.clear();
                    form.refused.clear();
                }
            }
            Err(reason) => self.session.notice = Some(reason),
        }
    }

    /// A click in the scene while a joint field asks: the entity under it,
    /// with the component it is of, goes in the field, or the status line
    /// says what the field takes. True when a field was asking.
    pub(crate) fn joint_pick(&mut self, hit: Option<Hit>) -> bool {
        let Some(slot) = self
            .active_document()
            .and_then(|d| d.joint_form.as_ref())
            .and_then(|f| f.active)
        else {
            return false;
        };
        let Some(hit) = hit else {
            return true;
        };
        if hit.path.is_empty() || !slot.fits(hit.name.kind) {
            self.session.notice = Some(format!(
                "Joint: the {} field takes a {}.",
                slot.field(),
                slot.takes()
            ));
            return true;
        }
        self.put_anchor(slot, hit.path, hit.name);
        true
    }

    /// `name` of the component at `path` into `slot`: the draft's, or one
    /// `EditJoint`. A create form's pick moves on to the child, then ends.
    fn put_anchor(&mut self, slot: Slot, path: Vec<ComponentId>, name: PersistentName) {
        let editing = self.joint_editing();
        let Some(document) = self.documents.get(self.active) else {
            return;
        };
        let Some(form) = document.joint_form.as_ref() else {
            return;
        };
        let record = form.record(document.replica.document());
        let old = form.anchor(record, slot.side()).cloned();
        let anchor = match placed(old.as_ref(), slot, path, name) {
            Ok(anchor) => anchor,
            Err(reason) => {
                self.session.notice = Some(format!("Joint: {reason}."));
                return;
            }
        };
        let side = slot.side();
        if editing {
            let held = anchor.clone();
            self.edit_joint(|j| match side {
                Side::Parent => j.parent = held,
                Side::Child => j.child = held,
            });
        }
        let Some(form) = self.joint_form_mut() else {
            return;
        };
        if form.edit.is_none() {
            match side {
                Side::Parent => form.parent = Some(anchor),
                Side::Child => form.child = Some(anchor),
            }
        }
        form.active = match slot {
            Slot::Axis(Side::Parent) if form.edit.is_none() => Some(Slot::Axis(Side::Child)),
            _ => None,
        };
    }

    /// The candidate at `index` of a lost anchor was taken: one
    /// `EditJoint`, the user's choice and no one else's.
    fn take_candidate(&mut self, field: &str, index: usize) {
        let Some(slot) = Slot::of_field(field) else {
            return;
        };
        let Some(document) = self.active_document() else {
            return;
        };
        let Some(form) = document.joint_form.as_ref() else {
            return;
        };
        let record = form.record(document.replica.document());
        let Some(anchor) = form.anchor(record, slot.side()) else {
            return;
        };
        let failed = failure(document, form);
        let lost = JointForm::lost(failed.as_ref());
        let holds_lost = form
            .held(record, slot)
            .is_some_and(|n| lost.contains(&Ref::Topo(n.clone())));
        let Some(Ref::Topo(name)) = failed.and_then(|d| d.candidates.get(index).cloned()) else {
            return;
        };
        if !holds_lost || index >= CANDIDATES_SHOWN {
            return;
        }
        let path = anchor.path.clone();
        self.put_anchor(slot, path, name);
    }

    /// The origin of an anchor was removed; an axis cannot be.
    fn remove_origin(&mut self, field: &str) {
        let Some(Slot::Origin(side)) = Slot::of_field(field) else {
            return;
        };
        let editing = self.joint_editing();
        let clear = |a: &mut Anchor| a.origin = None;
        if editing {
            self.edit_joint(|j| match side {
                Side::Parent => clear(&mut j.parent),
                Side::Child => clear(&mut j.child),
            });
        } else if let Some(form) = self.joint_form_mut() {
            match side {
                Side::Parent => form.parent.iter_mut().for_each(clear),
                Side::Child => form.child.iter_mut().for_each(clear),
            }
        }
    }

    /// OK: the draft becomes one `AddJoint`. An edit form commits as it
    /// goes, so Enter only closes it.
    pub(crate) fn joint_form_ok(&mut self) {
        let Some(document) = self.documents.get_mut(self.active) else {
            return;
        };
        let Some(form) = document.joint_form.as_mut() else {
            return;
        };
        if form.edit.is_some() {
            document.joint_form = None;
            return;
        }
        let assembly = form.assembly;
        let joint = match form.joint(&mut self.ids) {
            Ok(joint) => joint,
            Err(reason) => {
                self.session.notice = Some(reason);
                return;
            }
        };
        let command = Command::AddJoint {
            assembly,
            joint: Box::new(joint),
        };
        match document.submit(command) {
            Ok(_) => document.joint_form = None,
            Err(reason) => self.session.notice = Some(reason),
        }
    }

    pub(crate) fn joint_form_cancel(&mut self) {
        if let Some(document) = self.documents.get_mut(self.active) {
            document.joint_form = None;
            document.refusal = None;
        }
    }
}
