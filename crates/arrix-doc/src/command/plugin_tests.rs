//! The translation of a plugin's group (ADR-0038 §5, §6): every record
//! kind becomes its core command, in order, under the group's label; each
//! way a record can be malformed refuses the whole group, naming the
//! record, and applying the group to a document is the core's.

use arrix_core::{
    AssemblyId, ComponentId, FeatureId, Id, JointId, MaterialId, PartId, PersistentName, Quantity,
    QuantityKind, Ref, SlotName,
};
use arrix_plugin_api as api;

use super::plugin::translate;
use super::*;
use crate::registry::Registry;

fn id(n: u64) -> Id {
    Id(n)
}

fn ids() -> Vec<Id> {
    (1..=20).map(id).collect()
}

fn si(kind: QuantityKind, v: f64) -> api::Expression {
    api::Expression::Si(Quantity::si(kind, v))
}

fn text(t: &str) -> api::Expression {
    api::Expression::Text(t.into())
}

fn plane_draft(n: u64) -> api::FeatureDraft {
    api::FeatureDraft {
        id: FeatureId(id(n)),
        type_id: "core.datum-plane".into(),
        name: format!("plane {n}"),
        params: vec![api::NamedExpression {
            name: "offset".into(),
            expression: si(QuantityKind::Length, 0.01),
        }],
        choices: vec![api::ChoiceValue {
            name: "world".into(),
            value: "xy".into(),
        }],
        inputs: vec![],
    }
}

fn part_draft(n: u64, features: Vec<api::FeatureDraft>) -> api::PluginCommand {
    api::PluginCommand::AddPart(api::PartDraft {
        id: PartId(id(n)),
        name: format!("part {n}"),
        features,
        material: None,
    })
}

fn edge() -> PersistentName {
    "edge:revolve.0000000000001.side.0000000000002"
        .parse()
        .unwrap()
}

fn group(commands: Vec<api::PluginCommand>) -> api::CommandGroup {
    api::CommandGroup {
        label: "Group".into(),
        commands,
    }
}

fn translated(commands: Vec<api::PluginCommand>) -> Result<Command, Diagnostic> {
    translate(
        &group(commands),
        &ids(),
        &Registry::with_core_types(),
        &Document::default(),
    )
}

fn refused(commands: Vec<api::PluginCommand>) -> String {
    let d = translated(commands).unwrap_err();
    assert_eq!(d.code.as_str(), "command.invalid");
    d.message
}

#[test]
fn every_record_becomes_its_core_command_under_the_label() {
    let component = api::ComponentDraft {
        id: ComponentId(id(5)),
        name: "c".into(),
        of: api::ComponentOf::Part(PartId(id(1))),
        pose: Some(api::Pose {
            position: [
                si(QuantityKind::Length, 0.001),
                text("2 mm"),
                si(QuantityKind::Length, -3e-3),
            ],
            rpy: [
                si(QuantityKind::Angle, 0.0),
                si(QuantityKind::Angle, 0.5),
                text("90 deg"),
            ],
        }),
    };
    let anchor = |n: u64| api::Anchor {
        path: vec![ComponentId(id(n))],
        axis: edge(),
        origin: None,
    };
    let joint = api::JointDraft {
        id: JointId(id(6)),
        name: "j".into(),
        kind: api::JointKind::Revolute,
        parent: anchor(5),
        child: anchor(7),
        value: si(QuantityKind::Angle, 0.25),
        offset: text("0 deg"),
        limits: Some([text("-90 deg"), text("90 deg")]),
        flip: true,
    };
    let a = AssemblyId(id(9));
    let out = translated(vec![
        api::PluginCommand::AddMaterial(api::MaterialDraft {
            id: MaterialId(id(2)),
            name: "Steel".into(),
            density: Some(text("7850 kg/m3")),
            color: api::Rgb {
                red: 0.1,
                green: 0.2,
                blue: 0.3,
            },
        }),
        part_draft(1, vec![plane_draft(3)]),
        api::PluginCommand::AddFeature(api::AddFeature {
            part: PartId(id(1)),
            at: None,
            feature: plane_draft(4),
        }),
        api::PluginCommand::AddFeature(api::AddFeature {
            part: PartId(id(1)),
            at: Some(0),
            feature: plane_draft(8),
        }),
        api::PluginCommand::AssignMaterial(api::AssignMaterial {
            part: PartId(id(1)),
            body: Some(api::BodyRef {
                feature: FeatureId(id(3)),
                slot: SlotName::new("plane").unwrap(),
            }),
            material: Some(MaterialId(id(2))),
        }),
        api::PluginCommand::AddComponent(api::ComponentChange {
            assembly: a,
            component: component.clone(),
        }),
        api::PluginCommand::EditComponent(api::ComponentChange {
            assembly: a,
            component: api::ComponentDraft {
                pose: None,
                ..component
            },
        }),
        api::PluginCommand::AddJoint(api::JointChange {
            assembly: a,
            joint: joint.clone(),
        }),
        api::PluginCommand::EditJoint(api::JointChange {
            assembly: a,
            joint: api::JointDraft {
                limits: None,
                flip: false,
                ..joint
            },
        }),
    ])
    .unwrap();
    let Command::Group { label, commands } = out else {
        panic!("not a group")
    };
    assert_eq!(label, "Group");
    let kinds: Vec<&str> = commands
        .iter()
        .map(|c| match c {
            Command::AddMaterial { .. } => "add_material",
            Command::AddPart { .. } => "add_part",
            Command::AddFeature { .. } => "add_feature",
            Command::AssignMaterial { .. } => "assign_material",
            Command::AddComponent { .. } => "add_component",
            Command::EditComponent { .. } => "edit_component",
            Command::AddJoint { .. } => "add_joint",
            Command::EditJoint { .. } => "edit_joint",
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "add_material",
            "add_part",
            "add_feature",
            "add_feature",
            "assign_material",
            "add_component",
            "edit_component",
            "add_joint",
            "edit_joint"
        ]
    );

    // A feature appended lands after the ones the part came with, one
    // inserted at 0 before them; SI is written as a literal in SI units.
    let at = |i: usize| match &commands[i] {
        Command::AddFeature { at, .. } => *at,
        _ => unreachable!(),
    };
    assert_eq!((at(2), at(3)), (1, 0));
    let Command::AddFeature { record, .. } = &commands[2] else {
        unreachable!()
    };
    assert_eq!(record.type_version, 1);
    assert_eq!(record.params["offset"].text(), "0.01 m");
    let Command::AddComponent { component, .. } = &commands[5] else {
        unreachable!()
    };
    let pose = component.pose.as_ref().unwrap();
    assert_eq!(pose.position[0].text(), "0.001 m");
    assert_eq!(pose.position[1].text(), "2 mm");
    assert_eq!(pose.rpy[1].text(), "0.5 rad");
}

#[test]
fn a_group_applies_through_the_core_and_is_refused_there_when_it_does_not_fit() {
    let ok = translated(vec![part_draft(1, vec![plane_draft(2)])]).unwrap();
    let applied = apply(&Document::default(), &ok).unwrap();
    assert_eq!(applied.document.parts().len(), 1);
    assert_eq!(
        apply(&applied.document, &applied.inverse).unwrap().document,
        Document::default()
    );
    // A component of an assembly the document lacks is the core's refusal.
    let orphan = translated(vec![api::PluginCommand::AddComponent(
        api::ComponentChange {
            assembly: AssemblyId(id(9)),
            component: api::ComponentDraft {
                id: ComponentId(id(1)),
                name: "c".into(),
                of: api::ComponentOf::Part(PartId(id(2))),
                pose: None,
            },
        },
    )])
    .unwrap();
    assert!(apply(&Document::default(), &orphan).is_err());
}

#[test]
fn an_id_not_given_or_used_twice_is_refused_naming_the_record() {
    let why = refused(vec![part_draft(1, vec![]), part_draft(99, vec![])]);
    assert!(why.contains("record 2 (add-part)"), "{why}");
    assert!(why.contains("not one of the ids"), "{why}");
    let why = refused(vec![part_draft(1, vec![]), part_draft(1, vec![])]);
    assert!(
        why.contains("record 2") && why.contains("used twice"),
        "{why}"
    );
    // A feature's id counts, inside a part or added alone.
    let why = refused(vec![part_draft(1, vec![plane_draft(1)])]);
    assert!(
        why.contains("the feature") && why.contains("used twice"),
        "{why}"
    );
}

#[test]
fn a_feature_must_fit_its_registered_type() {
    let bad = |f: fn(&mut api::FeatureDraft)| {
        let mut d = plane_draft(2);
        f(&mut d);
        refused(vec![part_draft(1, vec![d])])
    };
    assert!(bad(|d| d.type_id = "nope.nothing".into()).contains("no feature type"));
    assert!(bad(|d| d.type_id = "Not An Id".into()).contains("not a feature type id"));
    assert!(bad(|d| d.params[0].name = "nope".into()).contains("no param `nope`"));
    assert!(bad(|d| d.choices[0].value = "ab".into()).contains("no option"));
    assert!(bad(|d| d.choices[0].name = "nope".into()).contains("no choice `nope`"));
    assert!(
        bad(|d| d.inputs.push(api::NamedInput {
            name: "plane".into(),
            refs: vec![Ref::Feature(FeatureId(id(7)))],
            many: true,
        }))
        .contains("does not take a list")
    );
    assert!(
        bad(|d| d.inputs.push(api::NamedInput {
            name: "plane".into(),
            refs: vec![],
            many: false,
        }))
        .contains("takes one reference")
    );
    assert!(bad(|d| d.params.push(d.params[0].clone())).contains("given twice"));
    assert!(
        bad(|d| d.params[0].expression = text("2 +")).contains("`2 +`"),
        "an unparsable expression"
    );
    assert!(
        bad(|d| d.params[0].expression = si(QuantityKind::Angle, 1.0)).contains("Angle"),
        "a quantity of another kind"
    );
    assert!(
        bad(|d| d.params[0].expression = si(QuantityKind::Length, f64::NAN)).contains("not finite")
    );
}

#[test]
fn a_feature_cannot_go_past_the_end_or_to_a_part_that_is_not_there() {
    let add = |part: u64, at: Option<u32>| {
        vec![api::PluginCommand::AddFeature(api::AddFeature {
            part: PartId(id(part)),
            at,
            feature: plane_draft(5),
        })]
    };
    assert!(refused(add(1, None)).contains("no part"));
    let both = |at| {
        let mut c = vec![part_draft(1, vec![plane_draft(2)])];
        c.extend(add(1, at));
        refused(c)
    };
    assert!(both(Some(2)).contains("past the history's end"));
}

#[test]
fn a_material_and_a_joint_are_checked() {
    let material = |color: api::Rgb, density: Option<api::Expression>| {
        refused(vec![api::PluginCommand::AddMaterial(api::MaterialDraft {
            id: MaterialId(id(1)),
            name: "m".into(),
            density,
            color,
        })])
    };
    let grey = |c| api::Rgb {
        red: c,
        green: c,
        blue: c,
    };
    assert!(material(grey(1.5), None).contains("outside 0 to 1"));
    assert!(material(grey(0.5), Some(si(QuantityKind::Length, 1.0))).contains("Length"));

    let joint = |f: fn(&mut api::JointDraft)| {
        let anchor = api::Anchor {
            path: vec![],
            axis: edge(),
            origin: None,
        };
        let mut j = api::JointDraft {
            id: JointId(id(1)),
            name: "j".into(),
            kind: api::JointKind::Prismatic,
            parent: anchor.clone(),
            child: anchor,
            value: si(QuantityKind::Length, 0.0),
            offset: si(QuantityKind::Length, 0.0),
            limits: None,
            flip: false,
        };
        f(&mut j);
        refused(vec![api::PluginCommand::AddJoint(api::JointChange {
            assembly: AssemblyId(id(2)),
            joint: j,
        })])
    };
    // A prismatic joint's value is a length; an angle is refused.
    assert!(joint(|j| j.value = si(QuantityKind::Angle, 0.0)).contains("Angle"));
    assert!(joint(|j| j.limits = Some([text("1 mm"), text("1 +")])).contains("`1 +`"));
}
