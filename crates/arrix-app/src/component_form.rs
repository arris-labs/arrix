//! The component's form (ADR-0030 §5): what a component instances, whether
//! a pose grounds it, and the pose's six quantities. A component has no
//! feature spec, so the app builds the `FormView` the one renderer draws
//! from the record. A create form is a draft until OK (one `AddComponent`);
//! an edit form is read from the record each frame and commits one
//! `EditComponent` per field.

use std::collections::BTreeMap;

use arrix_core::{AssemblyId, ComponentId, Diagnostic, Id, QuantityKind};
use arrix_doc::expr::Expr;
use arrix_doc::{Command, Component, ComponentOf, Document, Pose};
use arrix_ui::{ChoiceOptionView, FieldValue, FormDiagnostic, FormField, FormView, UiIntent};

use crate::ArrixApp;
use crate::context::Context;

/// The pose's fields: name, title, kind, and the unit a bare number is in.
const POSE: [(&str, &str, QuantityKind, &str); 6] = [
    ("x", "X", QuantityKind::Length, "mm"),
    ("y", "Y", QuantityKind::Length, "mm"),
    ("z", "Z", QuantityKind::Length, "mm"),
    ("roll", "Roll", QuantityKind::Angle, "deg"),
    ("pitch", "Pitch", QuantityKind::Angle, "deg"),
    ("yaw", "Yaw", QuantityKind::Angle, "deg"),
];

const GROUNDED: &str = "grounded";
const YES: &str = "yes";
const NO: &str = "no";

/// A component's form: a create form's draft, or an edit form over a
/// component's record.
#[derive(Clone, Debug)]
pub struct ComponentForm {
    assembly: AssemblyId,
    /// The component an edit form edits; none for a create form.
    edit: Option<ComponentId>,
    /// Names the form for the renderer's text buffers.
    id: String,
    /// A create form's name; an edit form's, until the record says it.
    name: String,
    /// The user typed the name: a later choice of what is instanced leaves
    /// it.
    named: bool,
    /// A create form's choice of what the component instances.
    of: Option<ComponentOf>,
    /// A create form's: a pose grounds the component.
    grounded: bool,
    /// A create form's pose texts; an edit form's, the ones the record
    /// does not say yet.
    texts: BTreeMap<String, String>,
    refused: BTreeMap<String, String>,
    /// A create form's component id, minted when first needed.
    minted: Option<Id>,
}

/// `part:<id>` or `assembly:<id>`: how the choice names what is instanced.
fn of_text(of: ComponentOf) -> String {
    match of {
        ComponentOf::Part(p) => format!("part:{p}"),
        ComponentOf::Assembly(a) => format!("assembly:{a}"),
    }
}

fn of_parse(text: &str) -> Option<ComponentOf> {
    let (kind, id) = text.split_once(':')?;
    match kind {
        "part" => id.parse().ok().map(ComponentOf::Part),
        "assembly" => id.parse().ok().map(ComponentOf::Assembly),
        _ => None,
    }
}

/// The part or assembly a component instances, by its name.
fn instanced_name(doc: &Document, of: ComponentOf) -> Option<String> {
    match of {
        ComponentOf::Part(p) => doc.parts().get(&p).map(|p| p.name.clone()),
        ComponentOf::Assembly(a) => doc.assemblies().get(&a).map(|a| a.name.clone()),
    }
}

fn identity() -> Pose {
    let zero = |text: &str| Expr::parse(text).expect("a literal expression");
    Pose {
        position: [zero("0 mm"), zero("0 mm"), zero("0 mm")],
        rpy: [zero("0 deg"), zero("0 deg"), zero("0 deg")],
    }
}

impl ComponentForm {
    pub fn assembly(&self) -> AssemblyId {
        self.assembly
    }

    fn create(id: String, assembly: AssemblyId, name: String, of: Option<ComponentOf>) -> Self {
        Self {
            assembly,
            edit: None,
            id,
            name,
            named: false,
            of,
            grounded: true,
            texts: BTreeMap::new(),
            refused: BTreeMap::new(),
            minted: None,
        }
    }

    fn edit(assembly: AssemblyId, component: ComponentId) -> Self {
        Self {
            assembly,
            edit: Some(component),
            id: format!("edit:component:{component}"),
            name: String::new(),
            named: true,
            of: None,
            grounded: true,
            texts: BTreeMap::new(),
            refused: BTreeMap::new(),
            minted: None,
        }
    }

    /// The record an edit form edits.
    fn record<'a>(&self, doc: &'a Document) -> Option<&'a Component> {
        doc.assemblies()
            .get(&self.assembly)?
            .components
            .get(&self.edit?)
    }

    fn of_now(&self, record: Option<&Component>) -> Option<ComponentOf> {
        record.map_or(self.of, |r| Some(r.of))
    }

    fn grounded_now(&self, record: Option<&Component>) -> bool {
        record.map_or(self.grounded, |r| r.pose.is_some())
    }

    /// The text of pose field `n` (an index of [`POSE`]): typed and not yet
    /// in the record, else the record's, else zero.
    fn text(&self, record: Option<&Component>, n: usize) -> String {
        let (name, _, _, _) = POSE[n];
        if let Some(text) = self.texts.get(name) {
            return text.clone();
        }
        let from_pose = |pose: &Pose| match n {
            0..=2 => pose.position[n].text().to_owned(),
            _ => pose.rpy[n - 3].text().to_owned(),
        };
        record
            .and_then(|r| r.pose.as_ref())
            .map_or_else(|| "0".to_owned(), from_pose)
    }

    /// The form as the card draws it.
    pub fn view(&self, doc: &Document, refusal: Option<&Diagnostic>) -> Option<FormView> {
        let record = match self.edit {
            Some(_) => Some(self.record(doc)?),
            None => None,
        };
        let of = self.of_now(record);
        let mut fields = vec![FormField {
            name: "of".into(),
            title: "Instances".into(),
            value: FieldValue::Choice {
                selected: of
                    .filter(|o| instanced_name(doc, *o).is_some())
                    .map(of_text),
                options: self.options(doc),
                clearable: false,
            },
            message: None,
        }];
        let grounded = self.grounded_now(record);
        fields.push(FormField {
            name: GROUNDED.into(),
            title: "Placed".into(),
            value: FieldValue::Choice {
                selected: Some(if grounded { YES } else { NO }.to_owned()),
                options: vec![
                    ChoiceOptionView {
                        name: YES.into(),
                        title: "By its pose (grounded)".into(),
                    },
                    ChoiceOptionView {
                        name: NO.into(),
                        title: "By a joint".into(),
                    },
                ],
                clearable: false,
            },
            message: None,
        });
        if grounded {
            for (n, (name, title, _, unit)) in POSE.iter().enumerate() {
                fields.push(FormField {
                    name: (*name).into(),
                    title: (*title).into(),
                    value: FieldValue::Quantity {
                        text: self.text(record, n),
                        unit: (*unit).into(),
                    },
                    message: self.refused.get(*name).cloned(),
                });
            }
        }
        let dangling = of
            .filter(|o| instanced_name(doc, *o).is_none())
            .map(|_| FormDiagnostic {
                code: "assembly.dangling".into(),
                message: "the part or assembly this component instances is not in the document"
                    .into(),
            });
        Some(FormView {
            id: self.id.clone(),
            title: "Component".into(),
            create: record.is_none(),
            name: match record {
                Some(r) if self.name.is_empty() => r.name.clone(),
                _ => self.name.clone(),
            },
            state: None,
            diagnostic: dangling.or_else(|| {
                refusal.map(|d| FormDiagnostic {
                    code: d.code.to_string(),
                    message: d.message.clone(),
                })
            }),
            read_only: None,
            fields,
        })
    }

    /// What a component of this assembly may instance: every part, and
    /// every assembly but this one.
    fn options(&self, doc: &Document) -> Vec<ChoiceOptionView> {
        let parts = doc.parts().values().map(|p| ChoiceOptionView {
            name: of_text(ComponentOf::Part(p.id)),
            title: format!("Part: {}", p.name),
        });
        let assemblies = doc
            .assemblies()
            .values()
            .filter(|a| a.id != self.assembly)
            .map(|a| ChoiceOptionView {
                name: of_text(ComponentOf::Assembly(a.id)),
                title: format!("Assembly: {}", a.name),
            });
        parts.chain(assemblies).collect()
    }

    /// The component OK adds, or why it cannot be.
    fn component(&mut self, ids: &mut arrix_core::IdMinter) -> Result<Component, String> {
        if let Some((field, why)) = self.refused.iter().next() {
            return Err(format!("Component: fix {field} first ({why})"));
        }
        let of = self.of.ok_or("Component: choose what it instances")?;
        let id = ComponentId(*self.minted.get_or_insert_with(|| ids.next_id()));
        let pose = match self.grounded {
            true => Some(self.pose(None)?),
            false => None,
        };
        Ok(Component {
            id,
            name: self.name.clone(),
            of,
            pose,
        })
    }

    /// The pose the texts say over `record`'s.
    fn pose(&self, record: Option<&Component>) -> Result<Pose, String> {
        let mut pose = identity();
        for (n, (name, _, _, unit)) in POSE.iter().enumerate() {
            // The field shows its unit beside the text, so a bare number
            // (the untouched zero among them) is in it.
            let text = self.text(record, n);
            let text = if text.trim().parse::<f64>().is_ok() {
                format!("{} {unit}", text.trim())
            } else {
                text
            };
            let expr = Expr::parse(&text).map_err(|e| format!("Component: {name}: {e}"))?;
            match n {
                0..=2 => pose.position[n] = expr,
                _ => pose.rpy[n - 3] = expr,
            }
        }
        Ok(pose)
    }
}

/// A name free among `assembly`'s components, from `base`.
fn free_name(doc: &Document, assembly: AssemblyId, base: &str) -> String {
    let used = |n: &str| {
        doc.assemblies()
            .get(&assembly)
            .is_some_and(|a| a.components.values().any(|c| c.name == n))
    };
    std::iter::once(base.to_owned())
        .chain((2..).map(|i| format!("{base} {i}")))
        .find(|n| !used(n))
        .expect("an unbounded range has a free name")
}

/// The assembly that holds the component at `path` from `root`, and its id
/// there.
fn owner(
    doc: &Document,
    root: AssemblyId,
    path: &[ComponentId],
) -> Option<(AssemblyId, ComponentId)> {
    let (last, above) = path.split_last()?;
    let mut assembly = root;
    for id in above {
        match doc.assemblies().get(&assembly)?.components.get(id)?.of {
            ComponentOf::Assembly(a) => assembly = a,
            ComponentOf::Part(_) => return None,
        }
    }
    doc.assemblies()
        .get(&assembly)?
        .components
        .contains_key(last)
        .then_some((assembly, *last))
}

impl ArrixApp {
    /// `core.assembly.component.create`: opens a draft of a component of
    /// the assembly in view, grounded at the identity. No command.
    pub(crate) fn open_component_create(&mut self) {
        let Some(document) = self.documents.get(self.active) else {
            return;
        };
        let Some(Context::Assembly(assembly)) = document.context() else {
            return;
        };
        let doc = document.replica.document();
        let of = doc
            .parts()
            .keys()
            .next()
            .map(|p| ComponentOf::Part(*p))
            .or_else(|| {
                doc.assemblies()
                    .keys()
                    .find(|a| **a != assembly)
                    .map(|a| ComponentOf::Assembly(*a))
            });
        let base = of
            .and_then(|o| instanced_name(doc, o))
            .map_or_else(|| "component".to_owned(), |n| n.to_lowercase());
        let name = free_name(doc, assembly, &base);
        self.session.forms_opened += 1;
        let id = format!("new:component:{}", self.session.forms_opened);
        if let Some(document) = self.documents.get_mut(self.active) {
            document.form = None;
            document.joint_form = None;
            document.clear_preview();
            document.component_form = Some(ComponentForm::create(id, assembly, name, of));
        }
    }

    /// A double-click on a component row: its form from the record. No
    /// command.
    pub(crate) fn open_component_edit(&mut self, assembly: &str, path: &[String]) {
        let (Ok(assembly), Ok(path)) = (
            assembly.parse::<AssemblyId>(),
            path.iter()
                .map(|c| c.parse())
                .collect::<Result<Vec<ComponentId>, _>>(),
        ) else {
            return;
        };
        let Some(document) = self.documents.get_mut(self.active) else {
            return;
        };
        let Some((owner, component)) = owner(document.replica.document(), assembly, &path) else {
            return;
        };
        document.form = None;
        document.joint_form = None;
        document.clear_preview();
        document.component_form = Some(ComponentForm::edit(owner, component));
    }

    pub(crate) fn component_form_open(&self) -> bool {
        self.active_document()
            .is_some_and(|d| d.component_form.is_some())
    }

    /// The component form the property card draws, if one is open.
    pub(crate) fn component_form_view(&self) -> Option<FormView> {
        let document = self.active_document()?;
        document
            .component_form
            .as_ref()?
            .view(document.replica.document(), document.refusal.as_ref())
    }

    /// Closes an edit form whose component an undo took away.
    pub(crate) fn follow_component_forms(&mut self) {
        for document in &mut self.documents {
            let gone = document.component_form.as_ref().is_some_and(|f| {
                f.edit.is_some() && f.record(document.replica.document()).is_none()
            });
            if gone {
                document.component_form = None;
            }
        }
    }

    /// What the user did in the open component form.
    pub(crate) fn component_form_intent(&mut self, intent: UiIntent) {
        let editing = self
            .active_document()
            .and_then(|d| d.component_form.as_ref())
            .is_some_and(|f| f.edit.is_some());
        match intent {
            UiIntent::FieldEdited { field, text } if !editing => {
                if let Some(form) = self.component_form_mut() {
                    form.texts.insert(field, text);
                }
            }
            UiIntent::FieldCommitted { field, text } => self.commit_pose(&field, text),
            UiIntent::NameCommitted { text } => {
                let text = text.trim().to_owned();
                if text.is_empty() {
                    return;
                }
                if editing {
                    self.edit_component(|c| c.name = text.clone());
                } else if let Some(form) = self.component_form_mut() {
                    form.name = text;
                    form.named = true;
                }
            }
            UiIntent::ChoiceSelected {
                field,
                value: Some(value),
            } => self.choose(&field, &value),
            UiIntent::FormOk => self.component_form_ok(),
            UiIntent::FormCancelled => self.component_form_cancel(),
            _ => {}
        }
    }

    fn component_form_mut(&mut self) -> Option<&mut ComponentForm> {
        self.documents.get_mut(self.active)?.component_form.as_mut()
    }

    /// A choice: what is instanced, or whether a pose grounds.
    fn choose(&mut self, field: &str, value: &str) {
        let editing = self
            .active_document()
            .and_then(|d| d.component_form.as_ref())
            .is_some_and(|f| f.edit.is_some());
        match (field, editing) {
            ("of", false) => {
                let of = of_parse(value);
                let Some(document) = self.documents.get_mut(self.active) else {
                    return;
                };
                let doc = document.replica.document();
                let Some(form) = document.component_form.as_mut() else {
                    return;
                };
                form.of = of;
                if !form.named
                    && let Some(base) = of.and_then(|o| instanced_name(doc, o))
                {
                    form.name = free_name(doc, form.assembly, &base.to_lowercase());
                }
            }
            ("of", true) => {
                if let Some(of) = of_parse(value) {
                    self.edit_component(|c| c.of = of);
                }
            }
            (GROUNDED, false) => {
                if let Some(form) = self.component_form_mut() {
                    form.grounded = value == YES;
                }
            }
            (GROUNDED, true) => {
                let grounded = value == YES;
                self.edit_component(|c| match (grounded, c.pose.is_some()) {
                    (true, false) => c.pose = Some(identity()),
                    (false, true) => c.pose = None,
                    _ => {}
                });
                if let Some(form) = self.component_form_mut() {
                    form.texts.clear();
                    form.refused.clear();
                }
            }
            _ => {}
        }
    }

    /// A pose field's text was committed: checked as a quantity of its
    /// kind; an edit form's valid one is one `EditComponent`.
    fn commit_pose(&mut self, field: &str, text: String) {
        let Some(n) = POSE.iter().position(|p| p.0 == field) else {
            return;
        };
        let kind = POSE[n].2;
        let verdict = self
            .evaluate_text(&text, kind)
            .map(|_| ())
            .map_err(|e| format!("{}: {e}", e.code()));
        let editing = self
            .active_document()
            .and_then(|d| d.component_form.as_ref())
            .is_some_and(|f| f.edit.is_some());
        let Some(form) = self.component_form_mut() else {
            return;
        };
        match &verdict {
            Ok(()) => form.refused.remove(field),
            Err(why) => form.refused.insert(field.to_owned(), why.clone()),
        };
        form.texts.insert(field.to_owned(), text.clone());
        if editing
            && verdict.is_ok()
            && let Ok(expr) = Expr::parse(&text)
        {
            self.edit_component(|c| {
                if let Some(pose) = &mut c.pose {
                    match n {
                        0..=2 => pose.position[n] = expr,
                        _ => pose.rpy[n - 3] = expr,
                    }
                }
            });
        }
    }

    /// One `EditComponent` of the open edit form's component, changed by
    /// `change`; none when it changes nothing. The form's typed texts are
    /// dropped once the record says them.
    fn edit_component(&mut self, change: impl FnOnce(&mut Component)) {
        let Some(document) = self.documents.get_mut(self.active) else {
            return;
        };
        let Some(form) = document.component_form.as_ref() else {
            return;
        };
        let Some(record) = form.record(document.replica.document()) else {
            return;
        };
        let assembly = form.assembly;
        let mut component = record.clone();
        change(&mut component);
        if component == *record {
            return;
        }
        let command = Command::EditComponent {
            assembly,
            component: Box::new(component),
        };
        match document.submit(command) {
            Ok(_) => {
                if let Some(form) = document.component_form.as_mut() {
                    form.texts.clear();
                }
            }
            Err(reason) => self.session.notice = Some(reason),
        }
    }

    /// OK: the draft becomes one `AddComponent`, which the tree then
    /// selects. An edit form commits as it goes, so Enter only closes it.
    pub(crate) fn component_form_ok(&mut self) {
        let Some(document) = self.documents.get_mut(self.active) else {
            return;
        };
        let Some(form) = document.component_form.as_mut() else {
            return;
        };
        if form.edit.is_some() {
            document.component_form = None;
            return;
        }
        let assembly = form.assembly;
        let component = match form.component(&mut self.ids) {
            Ok(component) => component,
            Err(reason) => {
                self.session.notice = Some(reason);
                return;
            }
        };
        let id = component.id;
        let command = Command::AddComponent {
            assembly,
            component: Box::new(component),
        };
        match document.submit(command) {
            Ok(_) => {
                document.component_form = None;
                document.view.component_selection = Some((assembly, vec![id]));
            }
            Err(reason) => self.session.notice = Some(reason),
        }
    }

    pub(crate) fn component_form_cancel(&mut self) {
        if let Some(document) = self.documents.get_mut(self.active) {
            document.component_form = None;
            document.refusal = None;
        }
    }
}
