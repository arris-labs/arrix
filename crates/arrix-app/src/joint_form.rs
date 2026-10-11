//! The joint's form (ADR-0030 §5): its kind, the two anchors (a face or an
//! edge of a component, and an optional origin face), the value, offset and
//! limits, and flip. A joint has no feature spec, so the app builds the
//! `FormView` the one renderer draws, as for a component. A create form is
//! a draft until OK (one `AddJoint`); an edit form is read from the record
//! each frame and commits one `EditJoint` per field. A lost anchor offers
//! its candidates and is replaced by a pick of the user's, never re-bound.

use std::collections::BTreeMap;

use arrix_core::{
    AssemblyId, ComponentId, Diagnostic, Id, IdMinter, JointId, PersistentName, QuantityKind, Ref,
    TopoKind,
};
use arrix_doc::expr::Expr;
use arrix_doc::{Anchor, Document, Joint, JointKind};
use arrix_ui::{
    ChoiceOptionView, FieldValue, FormDiagnostic, FormField, FormView, ReferenceValueView,
};

/// How many candidates a lost anchor lists.
pub const CANDIDATES_SHOWN: usize = 5;

const YES: &str = "yes";
const NO: &str = "no";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Parent,
    Child,
}

/// An anchor's two fields: the axis, and the optional origin face.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Axis(Side),
    Origin(Side),
}

impl Slot {
    pub const ALL: [Slot; 4] = [
        Slot::Axis(Side::Parent),
        Slot::Origin(Side::Parent),
        Slot::Axis(Side::Child),
        Slot::Origin(Side::Child),
    ];

    pub fn field(self) -> &'static str {
        match self {
            Slot::Axis(Side::Parent) => "parent",
            Slot::Origin(Side::Parent) => "parent_origin",
            Slot::Axis(Side::Child) => "child",
            Slot::Origin(Side::Child) => "child_origin",
        }
    }

    pub fn of_field(field: &str) -> Option<Slot> {
        Slot::ALL.into_iter().find(|s| s.field() == field)
    }

    pub fn side(self) -> Side {
        match self {
            Slot::Axis(s) | Slot::Origin(s) => s,
        }
    }

    fn title(self) -> &'static str {
        match self {
            Slot::Axis(Side::Parent) => "Parent",
            Slot::Origin(Side::Parent) => "Parent origin",
            Slot::Axis(Side::Child) => "Child",
            Slot::Origin(Side::Child) => "Child origin",
        }
    }

    pub fn takes(self) -> &'static str {
        match self {
            Slot::Axis(_) => "face or edge of a component",
            Slot::Origin(_) => "face of the same component",
        }
    }

    /// Whether a pick of this kind fits the slot.
    pub fn fits(self, kind: TopoKind) -> bool {
        match self {
            Slot::Axis(_) => kind != TopoKind::Vertex,
            Slot::Origin(_) => kind == TopoKind::Face,
        }
    }
}

/// The value fields: name, title.
const NUMBERS: [(&str, &str); 4] = [
    ("value", "Value"),
    ("offset", "Offset"),
    ("lower", "Lower limit"),
    ("upper", "Upper limit"),
];

/// The command for a kind: `fixed`, `revolute` or `prismatic`.
pub fn kind_word(kind: JointKind) -> &'static str {
    match kind {
        JointKind::Fixed => "fixed",
        JointKind::Revolute => "revolute",
        JointKind::Prismatic => "prismatic",
    }
}

pub fn kind_of_word(word: &str) -> Option<JointKind> {
    [JointKind::Fixed, JointKind::Revolute, JointKind::Prismatic]
        .into_iter()
        .find(|k| kind_word(*k) == word)
}

/// What a joint's numbers are measured in.
pub fn quantity(kind: JointKind) -> Option<(QuantityKind, &'static str)> {
    match kind {
        JointKind::Fixed => None,
        JointKind::Revolute => Some((QuantityKind::Angle, "deg")),
        JointKind::Prismatic => Some((QuantityKind::Length, "mm")),
    }
}

/// A joint's form: a create form's draft, or an edit form over a record.
#[derive(Clone, Debug)]
pub struct JointForm {
    pub(crate) assembly: AssemblyId,
    /// The joint an edit form edits; none for a create form.
    pub(crate) edit: Option<JointId>,
    id: String,
    pub(crate) name: String,
    /// The user typed the name.
    pub(crate) named: bool,
    /// A create form's kind.
    pub(crate) kind: JointKind,
    /// A create form's anchors, as far as they are picked.
    pub(crate) parent: Option<Anchor>,
    pub(crate) child: Option<Anchor>,
    pub(crate) flip: bool,
    /// The field a pick fills.
    pub(crate) active: Option<Slot>,
    /// Typed texts the record does not say yet.
    pub(crate) texts: BTreeMap<String, String>,
    pub(crate) refused: BTreeMap<String, String>,
    /// The lost anchors an edit form has already asked to re-pick.
    pub(crate) lost_seen: Vec<Ref>,
    minted: Option<Id>,
}

/// A reference in words: its kind, and the component it is of.
pub fn anchor_label(
    name: &PersistentName,
    path: &[ComponentId],
    component: &dyn Fn(&[ComponentId]) -> Option<String>,
) -> String {
    let what = match name.kind {
        TopoKind::Face => "Face",
        TopoKind::Edge => "Edge",
        TopoKind::Vertex => "Vertex",
    };
    match component(path) {
        Some(c) => format!("{what} of {c}"),
        None => what.to_owned(),
    }
}

impl JointForm {
    pub fn create(id: String, assembly: AssemblyId, name: String, kind: JointKind) -> Self {
        Self {
            assembly,
            edit: None,
            id,
            name,
            named: false,
            kind,
            parent: None,
            child: None,
            flip: false,
            active: Some(Slot::Axis(Side::Parent)),
            texts: BTreeMap::new(),
            refused: BTreeMap::new(),
            lost_seen: Vec::new(),
            minted: None,
        }
    }

    pub fn edit(assembly: AssemblyId, joint: JointId) -> Self {
        Self {
            edit: Some(joint),
            id: format!("edit:joint:{joint}"),
            named: true,
            active: None,
            ..Self::create(String::new(), assembly, String::new(), JointKind::Fixed)
        }
    }

    pub fn record<'a>(&self, doc: &'a Document) -> Option<&'a Joint> {
        doc.assemblies()
            .get(&self.assembly)?
            .joints
            .get(&self.edit?)
    }

    pub fn kind_now(&self, record: Option<&Joint>) -> JointKind {
        record.map_or(self.kind, |r| r.kind)
    }

    pub fn flip_now(&self, record: Option<&Joint>) -> bool {
        record.map_or(self.flip, |r| r.flip)
    }

    pub fn anchor<'a>(&'a self, record: Option<&'a Joint>, side: Side) -> Option<&'a Anchor> {
        match (record, side) {
            (Some(r), Side::Parent) => Some(&r.parent),
            (Some(r), Side::Child) => Some(&r.child),
            (None, Side::Parent) => self.parent.as_ref(),
            (None, Side::Child) => self.child.as_ref(),
        }
    }

    /// The name `slot` holds.
    pub fn held<'a>(&'a self, record: Option<&'a Joint>, slot: Slot) -> Option<&'a PersistentName> {
        let anchor = self.anchor(record, slot.side())?;
        match slot {
            Slot::Axis(_) => Some(&anchor.axis),
            Slot::Origin(_) => anchor.origin.as_ref(),
        }
    }

    /// The text of number `field`: typed and not yet in the record, else
    /// the record's, else the default.
    pub fn text(&self, record: Option<&Joint>, field: &str) -> String {
        match self.texts.get(field) {
            Some(text) => text.clone(),
            None => Self::stored(record, field),
        }
    }

    /// The text of number `field` as `record` says it, else the default.
    fn stored(record: Option<&Joint>, field: &str) -> String {
        let from_record = |j: &Joint| match field {
            "value" => Some(j.value.text().to_owned()),
            "offset" => Some(j.offset.text().to_owned()),
            "lower" => j.limits.as_ref().map(|l| l[0].text().to_owned()),
            "upper" => j.limits.as_ref().map(|l| l[1].text().to_owned()),
            _ => None,
        };
        match record {
            Some(j) => from_record(j).unwrap_or_default(),
            None if field == "value" || field == "offset" => "0".to_owned(),
            None => String::new(),
        }
    }

    /// The lost references an edit form's joint failed on.
    pub fn lost(lost: Option<&Diagnostic>) -> Vec<Ref> {
        lost.filter(|d| d.code.to_string().starts_with("ref.lost"))
            .map(|d| d.refs.clone())
            .unwrap_or_default()
    }

    /// The form as the card draws it. `lost` is the failure of the joint's
    /// last solve, `component` names a path.
    pub fn view(
        &self,
        doc: &Document,
        failure: Option<&Diagnostic>,
        refusal: Option<&Diagnostic>,
        component: &dyn Fn(&[ComponentId]) -> Option<String>,
    ) -> Option<FormView> {
        let record = match self.edit {
            Some(_) => Some(self.record(doc)?),
            None => None,
        };
        let kind = self.kind_now(record);
        let lost = Self::lost(failure);
        let mut fields = vec![FormField {
            name: "kind".into(),
            title: "Kind".into(),
            value: FieldValue::Choice {
                selected: Some(kind_word(kind).to_owned()),
                options: [JointKind::Fixed, JointKind::Revolute, JointKind::Prismatic]
                    .map(|k| ChoiceOptionView {
                        name: kind_word(k).into(),
                        title: kind_word(k).into(),
                    })
                    .to_vec(),
                clearable: false,
            },
            message: None,
        }];
        for slot in Slot::ALL {
            fields.push(self.reference_field(record, slot, &lost, failure, component));
        }
        if let Some((_, unit)) = quantity(kind) {
            for (name, title) in NUMBERS {
                fields.push(FormField {
                    name: name.into(),
                    title: title.into(),
                    value: FieldValue::Quantity {
                        text: self.text(record, name),
                        unit: unit.into(),
                    },
                    message: self.refused.get(name).cloned(),
                });
            }
        }
        fields.push(FormField {
            name: "flip".into(),
            title: "Flip".into(),
            value: FieldValue::Choice {
                selected: Some(if self.flip_now(record) { YES } else { NO }.to_owned()),
                options: [(YES, "Half a turn about x"), (NO, "As anchored")]
                    .map(|(name, title)| ChoiceOptionView {
                        name: name.into(),
                        title: title.into(),
                    })
                    .to_vec(),
                clearable: false,
            },
            message: None,
        });
        Some(FormView {
            id: self.id.clone(),
            title: "Joint".into(),
            create: record.is_none(),
            name: match record {
                Some(r) if self.name.is_empty() => r.name.clone(),
                _ => self.name.clone(),
            },
            state: None,
            diagnostic: failure.or(refusal).map(|d| FormDiagnostic {
                code: d.code.to_string(),
                message: d.message.clone(),
            }),
            read_only: None,
            fields,
        })
    }

    fn reference_field(
        &self,
        record: Option<&Joint>,
        slot: Slot,
        lost: &[Ref],
        failure: Option<&Diagnostic>,
        component: &dyn Fn(&[ComponentId]) -> Option<String>,
    ) -> FormField {
        let anchor = self.anchor(record, slot.side());
        let held = self.held(record, slot);
        let is_lost = held.is_some_and(|n| lost.contains(&Ref::Topo(n.clone())));
        let values = match (held, anchor) {
            (Some(name), Some(a)) => vec![ReferenceValueView {
                label: anchor_label(name, &a.path, component),
                lost: is_lost,
            }],
            _ => Vec::new(),
        };
        let candidates = match failure {
            Some(d) if is_lost => d
                .candidates
                .iter()
                .take(CANDIDATES_SHOWN)
                .enumerate()
                .filter_map(|(i, r)| match (r, anchor) {
                    (Ref::Topo(n), Some(a)) => Some(format!(
                        "{}. {}",
                        i + 1,
                        anchor_label(n, &a.path, component)
                    )),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        };
        FormField {
            name: slot.field().into(),
            title: slot.title().into(),
            value: FieldValue::Reference {
                takes: slot.takes().into(),
                values,
                list: false,
                active: self.active == Some(slot),
                candidates,
            },
            message: None,
        }
    }

    /// The joint OK adds, or why it cannot be.
    pub fn joint(&mut self, ids: &mut IdMinter) -> Result<Joint, String> {
        if let Some((field, why)) = self.refused.iter().next() {
            return Err(format!("Joint: fix {field} first ({why})"));
        }
        let parent = self.parent.clone().ok_or("Joint: pick the parent anchor")?;
        let child = self.child.clone().ok_or("Joint: pick the child anchor")?;
        let id = JointId(*self.minted.get_or_insert_with(|| ids.next_id()));
        let mut joint = Joint {
            id,
            name: self.name.clone(),
            kind: self.kind,
            parent,
            child,
            value: zero(),
            offset: zero(),
            limits: None,
            flip: self.flip,
        };
        self.numbers(&mut joint)?;
        Ok(joint)
    }

    /// `joint`'s value, offset and limits from the texts over its own.
    pub fn numbers(&self, joint: &mut Joint) -> Result<(), String> {
        if quantity(joint.kind).is_none() {
            (joint.value, joint.offset, joint.limits) = (zero(), zero(), None);
            return Ok(());
        }
        let held = joint.clone();
        // A text the form refused is not what the record becomes.
        let text = |field: &str| match self.refused.contains_key(field) {
            true => Self::stored(Some(&held), field),
            false => self.text(Some(&held), field),
        };
        let parse =
            |field: &str| Expr::parse(&text(field)).map_err(|e| format!("Joint: {field}: {e}"));
        joint.value = parse("value")?;
        joint.offset = parse("offset")?;
        let given = |field: &str| !text(field).trim().is_empty();
        joint.limits = match (given("lower"), given("upper")) {
            (false, false) => None,
            (true, true) => Some([parse("lower")?, parse("upper")?]),
            _ => return Err("Joint: give both limits, or neither".into()),
        };
        Ok(())
    }
}

pub fn zero() -> Expr {
    Expr::parse("0").expect("a literal expression")
}

/// A joint name free among `assembly`'s, from `base`.
pub fn free_name(doc: &Document, assembly: AssemblyId, base: &str) -> String {
    let used = |n: &str| {
        doc.assemblies()
            .get(&assembly)
            .is_some_and(|a| a.joints.values().any(|j| j.name == n))
    };
    std::iter::once(base.to_owned())
        .chain((2..).map(|i| format!("{base} {i}")))
        .find(|n| !used(n))
        .expect("an unbounded range has a free name")
}
