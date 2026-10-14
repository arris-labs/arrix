//! A plugin's command group as a core command (ADR-0038 §5, §6): the
//! records checked, one by one, against the ids the session handed the
//! command, the registry and the document the expansion was made on, then
//! translated into one `Command::Group`. A malformed record refuses the
//! whole group (`command.invalid`), naming it. Applying the group is the
//! core's, refused or stale as any command is.

use std::collections::{BTreeMap, BTreeSet};

use arrix_core::{Diagnostic, Id, Input, PartId, QuantityKind, Severity};
use arrix_plugin_api as api;

use super::Command;
use crate::assembly::{Anchor, Component, ComponentOf, Joint, JointKind, Pose};
use crate::document::{Document, FeatureRecord, FeatureTypeId, Part};
use crate::expr::Expr;
use crate::material::{Material, Rgb, SlotRef};
use crate::registry::Registry;

/// The group `group` as one `Command::Group`. `ids` are what the command
/// was given to mint with; `doc` is the document of the generation the
/// expansion was made on, which a feature added without an index is
/// appended to.
pub(crate) fn translate(
    group: &api::CommandGroup,
    ids: &[Id],
    registry: &Registry,
    doc: &Document,
) -> Result<Command, Diagnostic> {
    let mut t = Translator {
        allowed: ids.iter().copied().collect(),
        used: BTreeSet::new(),
        registry,
        doc,
        lengths: BTreeMap::new(),
    };
    let mut commands = Vec::with_capacity(group.commands.len());
    for (i, record) in group.commands.iter().enumerate() {
        let (word, result) = t.command(record);
        commands.push(result.map_err(|why| invalid(i, word, &why))?);
    }
    Ok(Command::Group {
        label: group.label.clone(),
        commands,
    })
}

fn invalid(index: usize, word: &str, why: &str) -> Diagnostic {
    Diagnostic::new(
        Severity::Error,
        "command.invalid",
        format!("record {} ({word}) is malformed: {why}", index + 1),
    )
    .expect("a literal code is well-formed")
}

struct Translator<'a> {
    /// The ids the command was given.
    allowed: BTreeSet<Id>,
    /// Those a record has defined.
    used: BTreeSet<Id>,
    registry: &'a Registry,
    doc: &'a Document,
    /// The history length of each part the group has added, so a feature
    /// appended to it lands after the ones it came with.
    lengths: BTreeMap<PartId, usize>,
}

type Why<T> = Result<T, String>;

impl Translator<'_> {
    fn command(&mut self, record: &api::PluginCommand) -> (&'static str, Why<Command>) {
        match record {
            api::PluginCommand::AddPart(p) => ("add-part", self.add_part(p)),
            api::PluginCommand::AddFeature(a) => ("add-feature", self.add_feature(a)),
            api::PluginCommand::AddComponent(c) => ("add-component", self.add_component(c)),
            api::PluginCommand::EditComponent(c) => ("edit-component", self.edit_component(c)),
            api::PluginCommand::AddJoint(j) => ("add-joint", self.add_joint(j)),
            api::PluginCommand::EditJoint(j) => ("edit-joint", self.edit_joint(j)),
            api::PluginCommand::AddMaterial(m) => ("add-material", self.add_material(m)),
            api::PluginCommand::AssignMaterial(a) => ("assign-material", Ok(assign(a))),
        }
    }

    /// `id` as a new record's: one of the ids given, defined once.
    fn fresh(&mut self, id: Id, what: &str) -> Why<()> {
        if !self.allowed.contains(&id) {
            return Err(format!("{what} {id} is not one of the ids it was given"));
        }
        if !self.used.insert(id) {
            return Err(format!("{what} {id} is used twice"));
        }
        Ok(())
    }

    fn add_part(&mut self, p: &api::PartDraft) -> Why<Command> {
        self.fresh(p.id.0, "the part")?;
        let mut history = Vec::new();
        let mut features = BTreeMap::new();
        for draft in &p.features {
            let record = self.feature(draft)?;
            history.push(record.id);
            features.insert(record.id, record);
        }
        self.lengths.insert(p.id, history.len());
        Ok(Command::AddPart {
            part: Part {
                id: p.id,
                name: p.name.clone(),
                history,
                features,
                rollback: None,
                material: p.material,
                body_materials: BTreeMap::new(),
            },
        })
    }

    fn add_feature(&mut self, a: &api::AddFeature) -> Why<Command> {
        let record = self.feature(&a.feature)?;
        let len = match self.lengths.get(&a.part) {
            Some(n) => *n,
            None => self
                .doc
                .parts()
                .get(&a.part)
                .map(|p| p.history.len())
                .ok_or_else(|| format!("no part {}", a.part))?,
        };
        let at = a.at.map_or(len, |n| n as usize);
        if at > len {
            return Err(format!("index {at} is past the history's end ({len})"));
        }
        self.lengths.insert(a.part, len + 1);
        Ok(Command::AddFeature {
            part: a.part,
            at,
            record,
        })
    }

    /// A drafted feature as a record of its registered type.
    fn feature(&mut self, d: &api::FeatureDraft) -> Why<FeatureRecord> {
        self.fresh(d.id.0, "the feature")?;
        let type_id = FeatureTypeId::new(d.type_id.clone()).map_err(|e| e.to_string())?;
        let ty = self
            .registry
            .get(&type_id)
            .ok_or_else(|| format!("no feature type {type_id}"))?;
        let spec = ty.spec();
        let mut params = BTreeMap::new();
        for p in &d.params {
            let field = spec
                .params
                .iter()
                .find(|f| f.name == p.name)
                .ok_or_else(|| format!("{type_id} has no param `{}`", p.name))?;
            let expr = expression(&p.expression, field.kind)?;
            if params.insert(p.name.clone(), expr).is_some() {
                return Err(format!("param `{}` is given twice", p.name));
            }
        }
        let mut choices = BTreeMap::new();
        for c in &d.choices {
            let field = spec
                .choices
                .iter()
                .find(|f| f.name == c.name)
                .ok_or_else(|| format!("{type_id} has no choice `{}`", c.name))?;
            if !field.options.iter().any(|o| o.name == c.value) {
                return Err(format!(
                    "`{}` is no option of `{}` of {type_id}",
                    c.value, c.name
                ));
            }
            if choices.insert(c.name.clone(), c.value.clone()).is_some() {
                return Err(format!("choice `{}` is given twice", c.name));
            }
        }
        let mut inputs = BTreeMap::new();
        for i in &d.inputs {
            let field = spec
                .inputs
                .iter()
                .find(|f| f.name == i.name)
                .ok_or_else(|| format!("{type_id} has no input `{}`", i.name))?;
            if field.list != i.many {
                return Err(format!(
                    "input `{}` of {type_id} {} a list",
                    i.name,
                    if field.list { "takes" } else { "does not take" }
                ));
            }
            let input = if i.many {
                Input::Many(i.refs.clone())
            } else {
                match &i.refs[..] {
                    [one] => Input::One(one.clone()),
                    other => {
                        return Err(format!(
                            "input `{}` takes one reference, not {}",
                            i.name,
                            other.len()
                        ));
                    }
                }
            };
            if inputs.insert(i.name.clone(), input).is_some() {
                return Err(format!("input `{}` is given twice", i.name));
            }
        }
        Ok(FeatureRecord {
            id: d.id,
            type_id,
            type_version: spec.version,
            name: d.name.clone(),
            params,
            choices,
            inputs,
            suppressed: false,
            sketch: None,
            frozen: None,
            file: None,
        })
    }

    fn add_component(&mut self, c: &api::ComponentChange) -> Why<Command> {
        self.fresh(c.component.id.0, "the component")?;
        Ok(Command::AddComponent {
            assembly: c.assembly,
            component: Box::new(component(&c.component)?),
        })
    }

    fn edit_component(&mut self, c: &api::ComponentChange) -> Why<Command> {
        Ok(Command::EditComponent {
            assembly: c.assembly,
            component: Box::new(component(&c.component)?),
        })
    }

    fn add_joint(&mut self, j: &api::JointChange) -> Why<Command> {
        self.fresh(j.joint.id.0, "the joint")?;
        Ok(Command::AddJoint {
            assembly: j.assembly,
            joint: Box::new(joint(&j.joint)?),
        })
    }

    fn edit_joint(&mut self, j: &api::JointChange) -> Why<Command> {
        Ok(Command::EditJoint {
            assembly: j.assembly,
            joint: Box::new(joint(&j.joint)?),
        })
    }

    fn add_material(&mut self, m: &api::MaterialDraft) -> Why<Command> {
        self.fresh(m.id.0, "the material")?;
        let c = &m.color;
        let appearance = Rgb([c.red, c.green, c.blue]);
        if !appearance.is_valid() {
            return Err("a colour channel is outside 0 to 1".into());
        }
        let density = m
            .density
            .as_ref()
            .map(|d| expression(d, QuantityKind::Density))
            .transpose()?;
        Ok(Command::AddMaterial {
            material: Material {
                id: m.id,
                name: m.name.clone(),
                density,
                appearance,
            },
        })
    }
}

fn assign(a: &api::AssignMaterial) -> Command {
    Command::AssignMaterial {
        part: a.part,
        body: a.body.as_ref().map(|b| SlotRef {
            feature: b.feature,
            slot: b.slot.clone(),
        }),
        material: a.material,
    }
}

fn component(c: &api::ComponentDraft) -> Why<Component> {
    let pose = c
        .pose
        .as_ref()
        .map(|p| {
            let length = |e: &api::Expression| expression(e, QuantityKind::Length);
            let angle = |e: &api::Expression| expression(e, QuantityKind::Angle);
            let [x, y, z] = &p.position;
            let [r, pi, ya] = &p.rpy;
            Ok::<_, String>(Pose {
                position: [length(x)?, length(y)?, length(z)?],
                rpy: [angle(r)?, angle(pi)?, angle(ya)?],
            })
        })
        .transpose()?;
    Ok(Component {
        id: c.id,
        name: c.name.clone(),
        of: match c.of {
            api::ComponentOf::Part(p) => ComponentOf::Part(p),
            api::ComponentOf::Assembly(a) => ComponentOf::Assembly(a),
        },
        pose,
    })
}

fn joint(j: &api::JointDraft) -> Why<Joint> {
    let kind = match j.kind {
        api::JointKind::Fixed => JointKind::Fixed,
        api::JointKind::Revolute => JointKind::Revolute,
        api::JointKind::Prismatic => JointKind::Prismatic,
    };
    // A revolute joint's value is an angle, a prismatic one's a length; a
    // fixed joint's is read as neither, so a count-free zero is enough.
    let value_kind = match kind {
        JointKind::Prismatic => QuantityKind::Length,
        _ => QuantityKind::Angle,
    };
    let anchor = |a: &api::Anchor| Anchor {
        path: a.path.clone(),
        axis: a.axis.clone(),
        origin: a.origin.clone(),
    };
    let limits = j
        .limits
        .as_ref()
        .map(|[lo, hi]| Ok::<_, String>([expression(lo, value_kind)?, expression(hi, value_kind)?]))
        .transpose()?;
    Ok(Joint {
        id: j.id,
        name: j.name.clone(),
        kind,
        parent: anchor(&j.parent),
        child: anchor(&j.child),
        value: expression(&j.value, value_kind)?,
        offset: expression(&j.offset, value_kind)?,
        limits,
        flip: j.flip,
    })
}

/// A record's value as an expression: an SI quantity written as a literal
/// in SI units, or the text parsed. The kind checks a quantity against the
/// slot; a text is checked when the document evaluates it.
fn expression(e: &api::Expression, kind: QuantityKind) -> Why<Expr> {
    let text = match e {
        api::Expression::Text(t) => t.clone(),
        api::Expression::Si(q) => {
            if q.kind != kind {
                return Err(format!("{:?} given where {kind:?} is wanted", q.kind));
            }
            if !q.si.is_finite() {
                return Err("a quantity is not finite".into());
            }
            let unit = match kind {
                QuantityKind::Length => " m",
                QuantityKind::Angle => " rad",
                QuantityKind::Mass => " kg",
                QuantityKind::Density => " kg/m3",
                QuantityKind::Count | QuantityKind::Ratio => "",
            };
            format!("{:?}{unit}", q.si)
        }
    };
    Expr::parse(&text).map_err(|e| format!("`{text}`: {e}"))
}
