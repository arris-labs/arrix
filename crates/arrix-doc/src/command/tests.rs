use std::collections::BTreeMap;

use arrix_core::{
    AssemblyId, ComponentId, FeatureId, Id, IdMinter, Input, JointId, ParamId, PartId,
    QuantityKind, Ref,
};
use arrix_sketch::{Constraint, Draft, Entity, Point, Sketch, SketchEdit, SketchError};
use proptest::prelude::*;

use super::*;
use crate::authority::{AuthorId, Authority, CommandEnvelope, Generation, Outcome, Rejected};
use crate::document::FeatureTypeId;
use crate::frozen::BlobRef;

const ALICE: AuthorId = AuthorId(Id(1));
const BOB: AuthorId = AuthorId(Id(2));

fn expr(t: &str) -> Expr {
    Expr::parse(t).unwrap()
}

fn add_param(id: u64, name: &str, e: &str) -> Command {
    Command::AddParam {
        param: ParamId(Id(id)),
        record: Param {
            name: name.into(),
            kind: QuantityKind::Length,
            expr: expr(e),
        },
    }
}

fn empty_part(id: u64) -> Command {
    Command::AddPart {
        part: Part {
            id: PartId(Id(id)),
            name: "Part".into(),
            history: vec![],
            features: BTreeMap::new(),
            rollback: None,
            material: None,
            body_materials: Default::default(),
        },
    }
}

fn plane(id: u64, name: &str, inputs: &[(&str, Ref)]) -> FeatureRecord {
    FeatureRecord {
        id: FeatureId(Id(id)),
        type_id: FeatureTypeId::new("core.datum-plane").unwrap(),
        type_version: 1,
        name: name.into(),
        params: BTreeMap::from([("offset".into(), expr("10 mm"))]),
        choices: BTreeMap::new(),
        inputs: inputs
            .iter()
            .map(|(k, r)| (k.to_string(), r.clone().into()))
            .collect(),
        suppressed: false,
        sketch: None,
        frozen: None,
        file: None,
    }
}

fn sketch_feature(id: u64, name: &str) -> FeatureRecord {
    FeatureRecord {
        type_id: FeatureTypeId::new("core.sketch").unwrap(),
        params: BTreeMap::new(),
        sketch: Some(Box::default()),
        frozen: None,
        file: None,
        ..plane(id, name, &[])
    }
}

/// The sketch of feature `id`.
fn sketch_of(doc: &Document, id: u64) -> &Sketch {
    let (_, _, record) = doc.feature(FeatureId(Id(id))).unwrap();
    record.sketch.as_deref().unwrap()
}

fn add_feature(part: u64, at: usize, record: FeatureRecord) -> Command {
    Command::AddFeature {
        part: PartId(Id(part)),
        at,
        record,
    }
}

fn env(author: AuthorId, base: u64, command: Command) -> CommandEnvelope {
    CommandEnvelope {
        author,
        base: Generation(base),
        command,
    }
}

fn applied(o: Result<Outcome, Rejected>) -> Generation {
    match o {
        Ok(Outcome::Applied(c)) => c.generation,
        other => panic!("{other:?}"),
    }
}

#[test]
fn each_command_applies_and_its_inverse_restores() {
    let doc = Document::default();
    let steps = [
        add_param(1, "w", "8 mm"),
        Command::SetParam {
            param: ParamId(Id(1)),
            expr: expr("12 mm"),
        },
        empty_part(10),
        add_feature(10, 0, plane(20, "Lower", &[])),
        add_feature(
            10,
            1,
            plane(21, "Upper", &[("plane", Ref::Feature(FeatureId(Id(20))))]),
        ),
        Command::SetRollback {
            part: PartId(Id(10)),
            at: Some(1),
        },
        add_feature(10, 0, plane(22, "First", &[])),
        Command::EditFeature {
            feature: FeatureId(Id(22)),
            edit: FeatureEdit {
                name: Some("Base".into()),
                suppressed: Some(true),
                params: BTreeMap::from([
                    ("offset".into(), None),
                    ("angle".into(), Some(expr("5 deg"))),
                ]),
                choices: BTreeMap::from([("world".into(), Some("zx".into()))]),
                inputs: BTreeMap::new(),
            },
        },
        Command::ReorderFeature {
            feature: FeatureId(Id(22)),
            to: 2,
        },
        Command::DeleteFeature {
            feature: FeatureId(Id(20)),
        },
        Command::DeleteParam {
            param: ParamId(Id(1)),
        },
        Command::DeletePart {
            part: PartId(Id(10)),
        },
    ];
    let mut doc = doc;
    for c in steps {
        let a = apply(&doc, &c).unwrap_or_else(|e| panic!("{c:?}: {e}"));
        assert_ne!(a.document, doc, "{c:?} changed something");
        assert_eq!(
            apply(&a.document, &a.inverse).unwrap().document,
            doc,
            "{c:?}"
        );
        doc = a.document;
    }
    assert_eq!(doc, Document::default());
}

#[test]
fn a_list_input_is_set_whole_and_its_inverse_is_the_earlier_value() {
    let f = |n| Ref::Feature(FeatureId(Id(n)));
    let mut doc = Document::default();
    for c in [
        empty_part(10),
        add_feature(10, 0, plane(20, "A", &[])),
        add_feature(10, 1, plane(21, "B", &[])),
        add_feature(10, 2, plane(22, "C", &[("plane", f(20))])),
    ] {
        doc = apply(&doc, &c).unwrap().document;
    }
    let set = |value: Option<Input>| Command::EditFeature {
        feature: FeatureId(Id(22)),
        edit: FeatureEdit {
            inputs: BTreeMap::from([("plane".into(), value)]),
            ..FeatureEdit::default()
        },
    };
    let input = |doc: &Document| doc.feature(FeatureId(Id(22))).unwrap().2.inputs["plane"].clone();
    // A single input becomes a list, in the order given, and back.
    let list = apply(&doc, &set(Some(vec![f(21), f(20)].into()))).unwrap();
    assert_eq!(input(&list.document), Input::Many(vec![f(21), f(20)]));
    let back = apply(&list.document, &list.inverse).unwrap();
    assert_eq!(back.document, doc);
    assert_eq!(input(&back.document), Input::One(f(20)));
    // And the list is removed whole, its inverse putting it back.
    let removed = apply(&list.document, &set(None)).unwrap();
    assert!(
        !removed
            .document
            .feature(FeatureId(Id(22)))
            .unwrap()
            .2
            .inputs
            .contains_key("plane")
    );
    assert_eq!(
        apply(&removed.document, &removed.inverse).unwrap().document,
        list.document
    );
    // The command crosses as JSON with the array.
    let json = serde_json::to_string(&set(Some(vec![f(21), f(20)].into()))).unwrap();
    assert!(json.contains(r#""plane":[{"feature""#), "{json}");
    assert_eq!(
        serde_json::from_str::<Command>(&json).unwrap(),
        set(Some(vec![f(21), f(20)].into()))
    );
}

#[test]
fn the_rollback_index_moves_with_the_features_before_it() {
    let mut doc = Document::default();
    for c in [
        empty_part(10),
        add_feature(10, 0, plane(20, "A", &[])),
        add_feature(10, 1, plane(21, "B", &[])),
        Command::SetRollback {
            part: PartId(Id(10)),
            at: Some(1),
        },
    ] {
        doc = apply(&doc, &c).unwrap().document;
    }
    let rollback = |d: &Document| d.parts[&PartId(Id(10))].rollback;
    let ins = apply(&doc, &add_feature(10, 0, plane(22, "C", &[]))).unwrap();
    assert_eq!(rollback(&ins.document), Some(2));
    let at_bar = apply(&doc, &add_feature(10, 1, plane(22, "C", &[]))).unwrap();
    assert_eq!(
        rollback(&at_bar.document),
        Some(1),
        "inserted at the bar: rolled back"
    );
    // Deleting the last active feature: its inverse restores the index too.
    let del = apply(
        &doc,
        &Command::DeleteFeature {
            feature: FeatureId(Id(20)),
        },
    )
    .unwrap();
    assert_eq!(rollback(&del.document), Some(0));
    assert!(matches!(del.inverse, Command::Group { .. }));
    assert_eq!(apply(&del.document, &del.inverse).unwrap().document, doc);
}

#[test]
fn refuses_whole_or_not_at_all() {
    let mut doc = Document::default();
    for c in [add_param(1, "w", "8 mm"), empty_part(10)] {
        doc = apply(&doc, &c).unwrap().document;
    }
    let err = |c: Command| apply(&doc, &c).unwrap_err();
    assert_eq!(err(add_param(1, "v", "1 mm")), CommandError::IdTaken(Id(1)));
    assert!(matches!(
        err(add_param(2, "w", "1 mm")),
        CommandError::Invalid(_)
    ));
    assert!(
        apply(&doc, &add_param(2, "a", "w + b")).is_ok(),
        "an unknown name is the evaluator's"
    );
    assert_eq!(
        err(add_feature(10, 1, plane(20, "A", &[]))),
        CommandError::Index { at: 1, len: 0 }
    );
    assert_eq!(
        err(add_feature(11, 0, plane(20, "A", &[]))),
        CommandError::NoPart(PartId(Id(11)))
    );
    // A group whose second command fails leaves nothing of its first.
    let group = Command::Group {
        label: "two".into(),
        commands: vec![add_param(2, "v", "1 mm"), add_param(3, "v", "2 mm")],
    };
    assert!(matches!(err(group), CommandError::Invalid(_)));
    // A cycle is refused with its nodes.
    let cyc = Command::Group {
        label: "cycle".into(),
        commands: vec![add_param(2, "a", "b"), add_param(3, "b", "a")],
    };
    let e = err(cyc);
    assert_eq!(e.diagnostic().code.as_str(), "dag.cycle");
    assert_eq!(e.diagnostic().refs.len(), 2);
}

#[test]
fn a_no_op_leaves_no_undo_entry() {
    let mut auth = Authority::default();
    applied(auth.submit(&env(ALICE, 0, add_param(1, "w", "8 mm"))));
    let same = Command::SetParam {
        param: ParamId(Id(1)),
        expr: expr("8 mm"),
    };
    assert_eq!(auth.submit(&env(ALICE, 1, same)), Ok(Outcome::NoOp));
    assert_eq!(auth.generation(), Generation(1));
    applied(auth.undo(ALICE));
    assert_eq!(auth.undo(ALICE), Err(Rejected::NothingToUndo));
    assert_eq!(auth.document(), &Document::default());
}

#[test]
fn a_stale_command_is_rejected_and_undo_is_per_author() {
    let mut auth = Authority::default();
    let set = |e: &str| Command::SetParam {
        param: ParamId(Id(1)),
        expr: expr(e),
    };
    applied(auth.submit(&env(ALICE, 0, add_param(1, "w", "8 mm"))));
    applied(auth.submit(&env(ALICE, 1, add_param(2, "h", "1 mm"))));
    // Bob saw generation 1; `w` has not changed since, so his edit applies.
    let g = applied(auth.submit(&env(BOB, 1, set("9 mm"))));
    assert_eq!(g, Generation(3));
    // Alice, at 2, now edits `w`: stale, naming it.
    let stale = auth.submit(&env(ALICE, 2, set("7 mm"))).unwrap_err();
    assert_eq!(
        stale,
        Rejected::Stale {
            base: Generation(2),
            nodes: vec![crate::dag::Node::Param(ParamId(Id(1)))]
        }
    );
    // Alice's undo of `h` touches only `h`: it applies. Her undo of `w`
    // would clobber Bob's edit: stale, and her entry stays.
    applied(auth.undo(ALICE));
    assert!(matches!(auth.undo(ALICE), Err(Rejected::Stale { .. })));
    assert!(matches!(auth.undo(ALICE), Err(Rejected::Stale { .. })));
    // Bob undoes his edit; now Alice's undo is fresh again.
    applied(auth.undo(BOB));
    applied(auth.undo(ALICE));
    assert_eq!(auth.document(), &Document::default());
    // Redo walks forward again, per author. Alice's redo returns `w` to
    // the content Bob's undo left, stamp included, so his redo clobbers
    // nothing and applies.
    applied(auth.redo(ALICE));
    applied(auth.redo(ALICE));
    assert_eq!(auth.redo(ALICE), Err(Rejected::NothingToRedo));
    applied(auth.redo(BOB));
    assert_eq!(auth.document().params[&ParamId(Id(1))].expr.text(), "9 mm");
    // Alice's edit after Bob's undo makes his redo stale.
    applied(auth.undo(BOB));
    applied(auth.submit(&env(ALICE, auth.generation().0, set("6 mm"))));
    assert!(matches!(auth.redo(BOB), Err(Rejected::Stale { .. })));
    // A new command clears its author's redo stack.
    applied(auth.undo(ALICE));
    applied(auth.submit(&env(ALICE, auth.generation().0, set("5 mm"))));
    assert_eq!(auth.redo(ALICE), Err(Rejected::NothingToRedo));
}

#[test]
fn same_author_undo_after_undo_stays_fresh() {
    let mut auth = Authority::default();
    let set = |e: &str| Command::SetParam {
        param: ParamId(Id(1)),
        expr: expr(e),
    };
    applied(auth.submit(&env(ALICE, 0, add_param(1, "w", "1 mm"))));
    applied(auth.submit(&env(ALICE, 1, set("2 mm"))));
    applied(auth.submit(&env(ALICE, 2, set("3 mm"))));
    for _ in 0..3 {
        applied(auth.undo(ALICE));
    }
    for _ in 0..3 {
        applied(auth.redo(ALICE));
    }
    for _ in 0..3 {
        applied(auth.undo(ALICE));
    }
    assert_eq!(auth.document(), &Document::default());
}

#[test]
fn a_sketch_edit_applies_whole_or_is_refused() {
    let mut doc = Document::default();
    for c in [
        add_param(1, "w", "8 mm"),
        empty_part(10),
        add_feature(10, 0, plane(20, "Plane", &[])),
        add_feature(10, 1, sketch_feature(21, "Sketch")),
    ] {
        doc = apply(&doc, &c).unwrap().document;
    }
    let sketch = |edit: SketchEdit| Command::SketchEdit {
        feature: FeatureId(Id(21)),
        edit,
    };
    let mut d = Draft::seeded(3);
    let (a, b, line) = d.add_line(0.0, 0.0, 0.01, 0.0);
    let dim = d.add_constraint(Constraint::Distance { a, b, value: 0.01 });
    d.set_constraint_expr(dim, Some("w".into()));
    let draw = sketch(SketchEdit::diff(&Sketch::new(), &d));
    let drawn = apply(&doc, &draw).unwrap();
    assert_eq!(sketch_of(&drawn.document, 21), &d.sketch);
    assert_eq!(drawn.touched, [Node::Feature(FeatureId(Id(21)))].into());
    assert_eq!(
        apply(&drawn.document, &drawn.inverse).unwrap().document,
        doc
    );

    // Removing a point a line still stands on is refused, not swept.
    let dangling = sketch(SketchEdit {
        points: [(a, None)].into(),
        ..SketchEdit::default()
    });
    assert_eq!(
        apply(&drawn.document, &dangling),
        Err(CommandError::Sketch {
            feature: FeatureId(Id(21)),
            error: SketchError::MissingPoint { by: line, point: a },
        })
    );
    // With what stands on it, it goes, and comes back.
    let remove = sketch(SketchEdit::remove(&d, &[a].into()));
    let removed = apply(&drawn.document, &remove).unwrap();
    assert_eq!(sketch_of(&removed.document, 21).points().len(), 1);
    assert_eq!(
        apply(&removed.document, &removed.inverse).unwrap().document,
        drawn.document
    );

    // A plane has no sketch to edit.
    let on_plane = Command::SketchEdit {
        feature: FeatureId(Id(20)),
        edit: SketchEdit::default(),
    };
    assert_eq!(
        apply(&drawn.document, &on_plane),
        Err(CommandError::NoSketch(FeatureId(Id(20))))
    );
    // A dimension's expression parses, as a feature field's does.
    let mut record = d.get_constraint_record(dim).unwrap().clone();
    record.expr = Some("w +".into());
    let bad = sketch(SketchEdit {
        constraints: [(dim, Some(record))].into(),
        ..SketchEdit::default()
    });
    assert!(matches!(
        apply(&drawn.document, &bad),
        Err(CommandError::Invalid(Invalid::SketchExpr { constraint, .. })) if constraint == dim
    ));
}

#[test]
fn commands_cross_as_json() {
    let c = Command::Group {
        label: "g".into(),
        commands: vec![
            add_param(1, "w", "8 mm"),
            Command::EditFeature {
                feature: FeatureId(Id(3)),
                edit: FeatureEdit {
                    params: BTreeMap::from([("x".into(), None)]),
                    ..FeatureEdit::default()
                },
            },
        ],
    };
    let json = serde_json::to_string(&c).unwrap();
    assert_eq!(
        json,
        r#"{"group":{"label":"g","commands":[{"add_param":{"param":"0000000000001","record":{"name":"w","kind":"length","expr":"8 mm"}}},{"edit_feature":{"feature":"0000000000003","edit":{"params":{"x":null}}}}]}}"#
    );
    assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), c);
    crate::session::crosses(&crate::Request::Submit(env(ALICE, 0, c)), "request");
}

/// A random command against the current document, from a few numbers.
fn command_for(doc: &Document, minter: &mut IdMinter, op: (u8, u64, u64), depth: u8) -> Command {
    let (kind, a, b) = op;
    let params: Vec<ParamId> = doc.params.keys().copied().collect();
    let parts: Vec<&Part> = doc.parts.values().collect();
    let features: Vec<FeatureId> = parts.iter().flat_map(|p| p.history.clone()).collect();
    let pick = |n: usize| (a as usize) % n.max(1);
    let exprs = ["1 mm", "p0 + 1 mm", "p1 * 2", "p2 - p0"];
    let e = || expr(exprs[(b % 4) as usize]);
    let sketches: Vec<(FeatureId, &Sketch)> = parts
        .iter()
        .flat_map(|p| p.features.values())
        .filter_map(|r| Some((r.id, r.sketch.as_deref()?)))
        .collect();
    if (16..24).contains(&(kind % 32)) {
        return assembly_command_for(doc, minter, kind % 32 - 16, a, b);
    }
    match kind % 16 {
        3 => {
            let id = minter.next_id().0;
            empty_part(id)
        }
        0 => add_param(
            minter.next_id().0,
            &format!("p{}", a % 4),
            exprs[(b % 4) as usize],
        ),
        1 if !params.is_empty() => Command::SetParam {
            param: params[pick(params.len())],
            expr: e(),
        },
        2 if !params.is_empty() => Command::DeleteParam {
            param: params[pick(params.len())],
        },
        4 if !parts.is_empty() => Command::DeletePart {
            part: parts[pick(parts.len())].id,
        },
        6 if !features.is_empty() => Command::EditFeature {
            feature: features[pick(features.len())],
            edit: FeatureEdit {
                name: (b % 3 == 0).then(|| format!("f{}", a % 5)),
                suppressed: b.is_multiple_of(2).then_some(a % 2 == 0),
                params: BTreeMap::from([("offset".into(), (b % 4 != 1).then(e))]),
                choices: BTreeMap::from([("world".into(), (b % 5 == 0).then(|| "yz".into()))]),
                inputs: BTreeMap::from([("plane".into(), None)]),
            },
        },
        7 if !features.is_empty() => Command::DeleteFeature {
            feature: features[pick(features.len())],
        },
        8 if !features.is_empty() => Command::ReorderFeature {
            feature: features[pick(features.len())],
            to: (b % 4) as usize,
        },
        9 if !parts.is_empty() => {
            let part = parts[pick(parts.len())];
            Command::SetRollback {
                part: part.id,
                at: (b % 3 != 0).then(|| (b as usize) % (part.history.len() + 1)),
            }
        }
        11 if !parts.is_empty() => {
            let part = parts[pick(parts.len())];
            let rec = sketch_feature(minter.next_id().0, &format!("f{}", b % 5));
            add_feature(part.id.0.0, (b as usize) % (part.history.len() + 1), rec)
        }
        // Weighted up: a sketch takes many edits to hold anything.
        12..=15 if !sketches.is_empty() => {
            let (feature, sketch) = sketches[pick(sketches.len())];
            Command::SketchEdit {
                feature,
                edit: sketch_edit_for(sketch, minter, a, b, e()),
            }
        }
        10 if depth < 2 => Command::Group {
            label: "g".into(),
            commands: vec![
                command_for(doc, minter, (a as u8, b, a), depth + 1),
                command_for(doc, minter, (b as u8, a, b), depth + 1),
            ],
        },
        _ if !parts.is_empty() => {
            let part = parts[pick(parts.len())];
            let mut inputs = vec![];
            if let Some(t) = features.get((b as usize) % (features.len() + 1)) {
                inputs.push(("plane", Ref::Feature(*t)));
            }
            let rec = plane(minter.next_id().0, &format!("f{}", b % 5), &inputs);
            add_feature(part.id.0.0, (b as usize) % (part.history.len() + 2), rec)
        }
        _ => {
            let id = minter.next_id().0;
            empty_part(id)
        }
    }
}

/// A random assembly or file command, from a few numbers.
fn assembly_command_for(
    doc: &Document,
    minter: &mut IdMinter,
    which: u8,
    a: u64,
    b: u64,
) -> Command {
    use crate::assembly::{Anchor, Assembly, Component, ComponentOf, Joint, JointKind, Pose};
    let assemblies: Vec<&Assembly> = doc.assemblies.values().collect();
    let parts: Vec<PartId> = doc.parts.keys().copied().collect();
    let pick = |n: usize| (a as usize) % n.max(1);
    let id: u64 = minter.next_id().0;
    let named = |what: &str| format!("{what}{id:x}");
    let pose = || Pose {
        position: [expr("1 mm"), expr("2 mm"), expr("p0 + 1 mm")],
        rpy: [expr("0 deg"), expr("0 deg"), expr("90 deg")],
    };
    let component = |of| Component {
        id: ComponentId(Id(id)),
        name: named("c"),
        of,
        pose: b.is_multiple_of(2).then(pose),
    };
    let files: Vec<BlobRef> = doc
        .blobs
        .keys()
        .filter(|k| k.path().ends_with(".step"))
        .cloned()
        .collect();
    let Some(asm) = assemblies.get(pick(assemblies.len())).copied() else {
        return match which {
            0 | 1 => Command::AddFile {
                bytes: vec![a as u8; (b % 3) as usize + 1],
            },
            _ => Command::AddAssembly {
                assembly: Assembly {
                    id: AssemblyId(Id(id)),
                    name: named("a"),
                    components: BTreeMap::new(),
                    joints: BTreeMap::new(),
                },
            },
        };
    };
    let components: Vec<&Component> = asm.components.values().collect();
    let joints: Vec<&Joint> = asm.joints.values().collect();
    match which {
        0 => Command::AddFile {
            bytes: vec![a as u8; (b % 3) as usize + 1],
        },
        1 if !files.is_empty() => Command::RemoveFile {
            blob: files[pick(files.len())].clone(),
        },
        2 => Command::AddAssembly {
            assembly: Assembly {
                id: AssemblyId(Id(id)),
                name: named("a"),
                components: BTreeMap::new(),
                joints: BTreeMap::new(),
            },
        },
        3 => {
            let of = match (parts.is_empty(), b % 3) {
                (false, 0 | 1) => ComponentOf::Part(parts[pick(parts.len())]),
                _ => ComponentOf::Assembly(assemblies[(b as usize) % assemblies.len()].id),
            };
            Command::AddComponent {
                assembly: asm.id,
                component: Box::new(component(of)),
            }
        }
        4 if !components.is_empty() => {
            let old = components[pick(components.len())];
            Command::EditComponent {
                assembly: asm.id,
                component: Box::new(Component {
                    id: old.id,
                    name: if b.is_multiple_of(2) {
                        old.name.clone()
                    } else {
                        named("e")
                    },
                    of: old.of,
                    pose: if b.is_multiple_of(3) {
                        None
                    } else {
                        Some(pose())
                    },
                }),
            }
        }
        5 if components.len() >= 2 => {
            let (p, c) = (
                components[pick(components.len())],
                components[(a as usize + 1) % components.len()],
            );
            let anchor = |k: &Component| Anchor {
                path: vec![k.id],
                axis: "face:import.0000000000007.42".parse().unwrap(),
                origin: None,
            };
            Command::AddJoint {
                assembly: asm.id,
                joint: Box::new(Joint {
                    id: JointId(Id(id)),
                    name: named("j"),
                    kind: JointKind::Revolute,
                    parent: anchor(p),
                    child: anchor(c),
                    value: expr("p0"),
                    offset: expr("0 deg"),
                    limits: None,
                    flip: b.is_multiple_of(2),
                }),
            }
        }
        6 if !joints.is_empty() => {
            let old = joints[pick(joints.len())];
            if b.is_multiple_of(2) {
                Command::DeleteJoint {
                    assembly: asm.id,
                    joint: old.id,
                }
            } else {
                Command::EditJoint {
                    assembly: asm.id,
                    joint: Box::new(Joint {
                        flip: !old.flip,
                        ..old.clone()
                    }),
                }
            }
        }
        7 if !components.is_empty() && b.is_multiple_of(2) => Command::DeleteComponent {
            assembly: asm.id,
            component: components[pick(components.len())].id,
        },
        _ => Command::DeleteAssembly { assembly: asm.id },
    }
}

/// A random edit of `sketch`: a line drawn from one of its points, a
/// dimension by an expression, a point moved, a construction mark flipped,
/// a record removed with what stands on it, or a bare point removed, which
/// is refused while something stands on it.
fn sketch_edit_for(sketch: &Sketch, minter: &mut IdMinter, a: u64, b: u64, e: Expr) -> SketchEdit {
    let mut d = Draft::with_sketch(sketch.clone(), IdMinter::new(minter.next_id().0));
    let points: Vec<_> = sketch.points().keys().copied().collect();
    let lines: Vec<_> = sketch.entities().keys().copied().collect();
    let pick = |n: usize| (b as usize) % n.max(1);
    let (x, y) = ((a % 97) as f64 * 1e-3, (b % 89) as f64 * 1e-3);
    match (a % 6, points.first()) {
        (1, Some(_)) if points.len() > 1 => {
            let (p, q) = (
                points[pick(points.len())],
                points[(b as usize + 1) % points.len()],
            );
            if p != q {
                let c = d.add_constraint(Constraint::Distance {
                    a: p,
                    b: q,
                    value: x + 1e-3,
                });
                d.set_constraint_expr(c, Some(e.text().into()));
            }
        }
        (2, Some(_)) => d
            .point_mut(points[pick(points.len())])
            .unwrap()
            .set_pos(x, y),
        (3, _) if !lines.is_empty() => {
            let l = lines[pick(lines.len())];
            let on = !d.is_construction(l);
            d.set_construction(l, on);
        }
        (4, _) if !points.is_empty() => {
            return SketchEdit::remove(sketch, &[points[pick(points.len())]].into());
        }
        (5, Some(_)) => {
            return SketchEdit {
                points: [(points[pick(points.len())], None)].into(),
                ..SketchEdit::default()
            };
        }
        (_, from) => {
            let start = from
                .copied()
                .unwrap_or_else(|| d.add_point(Point::new(0.0, 0.0)));
            let end = d.add_point(Point::new(x, y));
            d.add_entity(Entity::Line { start, end });
        }
    }
    SketchEdit::diff(sketch, &d)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn apply_then_inverse_is_the_identity(
        ops in proptest::collection::vec((any::<u8>(), any::<u64>(), any::<u64>()), 1..40),
    ) {
        let mut minter = IdMinter::new(7);
        let mut auth = Authority::default();
        let mut states = vec![auth.document().clone()];
        for op in ops {
            let doc = auth.document().clone();
            let c = command_for(&doc, &mut minter, op, 0);
            let Ok(a) = apply(&doc, &c) else { continue };
            let back = apply(&a.document, &a.inverse).map_err(|e| TestCaseError::fail(format!("{c:?}: {e}")))?;
            prop_assert_eq!(&back.document, &doc, "{:?}", c);
            prop_assert_eq!(&apply(&back.document, &back.inverse).unwrap().document, &a.document);
            let envelope = env(ALICE, auth.generation().0, c);
            crate::session::crosses(&crate::Request::Submit(envelope.clone()), "request");
            let outcome = auth.submit(&envelope).unwrap();
            crate::session::crosses(&outcome, "outcome");
            match outcome {
                Outcome::Applied(_) => states.push(auth.document().clone()),
                Outcome::NoOp => prop_assert_eq!(&a.document, &doc),
                Outcome::Previewing | Outcome::Exporting | Outcome::Loaded { .. } => {
                    unreachable!("a submit is never a preview, an export or a load")
                }
            }
        }
        for want in states.iter().rev().skip(1) {
            applied(auth.undo(ALICE));
            prop_assert_eq!(auth.document(), want);
        }
        prop_assert_eq!(auth.undo(ALICE), Err(Rejected::NothingToUndo));
        for want in states.iter().skip(1) {
            applied(auth.redo(ALICE));
            prop_assert_eq!(auth.document(), want);
        }
    }
}

mod assemblies {
    use super::*;
    use crate::assembly::{Anchor, Assembly, Component, ComponentOf, Joint, JointKind, Pose};
    use crate::document::ImportedFile;
    use arrix_core::{AssemblyId, ComponentId, JointId};

    fn component(id: u64, name: &str, of: ComponentOf, grounded: bool) -> Component {
        Component {
            id: ComponentId(Id(id)),
            name: name.into(),
            of,
            pose: grounded.then(|| Pose {
                position: [expr("0 mm"), expr("0 mm"), expr("5 mm")],
                rpy: [expr("0 deg"), expr("0 deg"), expr("0 deg")],
            }),
        }
    }

    fn joint(id: u64, parent: u64, child: u64) -> Joint {
        let anchor = |c: u64| Anchor {
            path: vec![ComponentId(Id(c))],
            axis: "face:import.0000000000007.42".parse().unwrap(),
            origin: None,
        };
        Joint {
            id: JointId(Id(id)),
            name: "hinge".into(),
            kind: JointKind::Revolute,
            parent: anchor(parent),
            child: anchor(child),
            value: expr("30 deg"),
            offset: expr("0 deg"),
            limits: None,
            flip: false,
        }
    }

    fn asm(id: u64, name: &str) -> Assembly {
        Assembly {
            id: AssemblyId(Id(id)),
            name: name.into(),
            components: BTreeMap::new(),
            joints: BTreeMap::new(),
        }
    }

    fn run(doc: &Document, command: Command) -> Document {
        let a = apply(doc, &command).unwrap_or_else(|e| panic!("{command:?}: {e}"));
        let back = apply(&a.document, &a.inverse).unwrap();
        assert_eq!(&back.document, doc, "{command:?}");
        a.document
    }

    fn base() -> Document {
        let mut doc = Document::default();
        for c in [empty_part(10), empty_part(11)] {
            doc = run(&doc, c);
        }
        doc = run(
            &doc,
            Command::AddAssembly {
                assembly: asm(1, "arm"),
            },
        );
        for (id, name, part, grounded) in [(2, "base", 10, true), (3, "servo", 11, false)] {
            doc = run(
                &doc,
                Command::AddComponent {
                    assembly: AssemblyId(Id(1)),
                    component: Box::new(component(
                        id,
                        name,
                        ComponentOf::Part(PartId(Id(part))),
                        grounded,
                    )),
                },
            );
        }
        run(
            &doc,
            Command::AddJoint {
                assembly: AssemblyId(Id(1)),
                joint: Box::new(joint(4, 2, 3)),
            },
        )
    }

    #[test]
    fn every_assembly_command_applies_and_its_inverse_restores() {
        let doc = base();
        assert_eq!(doc.assemblies[&AssemblyId(Id(1))].joints.len(), 1);
        let a = AssemblyId(Id(1));
        let mut moved = component(3, "servo", ComponentOf::Part(PartId(Id(11))), true);
        moved.name = "servo 2".into();
        let doc2 = run(
            &doc,
            Command::EditComponent {
                assembly: a,
                component: Box::new(moved),
            },
        );
        let flipped = Joint {
            flip: true,
            ..joint(4, 2, 3)
        };
        let doc3 = run(
            &doc2,
            Command::EditJoint {
                assembly: a,
                joint: Box::new(flipped),
            },
        );
        let doc4 = run(
            &doc3,
            Command::DeleteJoint {
                assembly: a,
                joint: JointId(Id(4)),
            },
        );
        let doc5 = run(
            &doc4,
            Command::DeleteComponent {
                assembly: a,
                component: ComponentId(Id(3)),
            },
        );
        run(&doc5, Command::DeleteAssembly { assembly: a });
    }

    #[test]
    fn what_is_still_named_is_refused_as_dangling_not_cascaded() {
        let doc = base();
        let a = AssemblyId(Id(1));
        let code = |c: Command| match apply(&doc, &c) {
            Err(e) => e.diagnostic().code.to_string(),
            Ok(_) => panic!("{c:?} applied"),
        };
        assert_eq!(
            code(Command::DeletePart {
                part: PartId(Id(11))
            }),
            "assembly.dangling"
        );
        assert_eq!(
            code(Command::DeleteComponent {
                assembly: a,
                component: ComponentId(Id(3))
            }),
            "assembly.dangling"
        );
        // A group may pass through the state, ending whole.
        let both = Command::Group {
            label: "remove the servo".into(),
            commands: vec![
                Command::DeleteJoint {
                    assembly: a,
                    joint: JointId(Id(4)),
                },
                Command::DeleteComponent {
                    assembly: a,
                    component: ComponentId(Id(3)),
                },
                Command::DeletePart {
                    part: PartId(Id(11)),
                },
            ],
        };
        let done = run(&doc, both);
        assert!(!done.parts.contains_key(&PartId(Id(11))));
        // A nested assembly is held by the component that places it.
        let nested = run(
            &doc,
            Command::AddAssembly {
                assembly: asm(5, "outer"),
            },
        );
        let nested = run(
            &nested,
            Command::AddComponent {
                assembly: AssemblyId(Id(5)),
                component: Box::new(component(6, "arm", ComponentOf::Assembly(a), true)),
            },
        );
        let refused = apply(&nested, &Command::DeleteAssembly { assembly: a }).unwrap_err();
        assert_eq!(refused.diagnostic().code.as_str(), "assembly.dangling");
    }

    #[test]
    fn ids_are_not_reused_and_names_are_unique() {
        let doc = base();
        let a = AssemblyId(Id(1));
        let again = Command::AddComponent {
            assembly: a,
            component: Box::new(component(
                2,
                "other",
                ComponentOf::Part(PartId(Id(10))),
                true,
            )),
        };
        assert!(matches!(apply(&doc, &again), Err(CommandError::IdTaken(_))));
        let joint_as_component = Command::AddComponent {
            assembly: a,
            component: Box::new(component(
                4,
                "other",
                ComponentOf::Part(PartId(Id(10))),
                true,
            )),
        };
        assert!(matches!(
            apply(&doc, &joint_as_component),
            Err(CommandError::IdTaken(_))
        ));
        let same_name = Command::AddComponent {
            assembly: a,
            component: Box::new(component(
                9,
                "base",
                ComponentOf::Part(PartId(Id(10))),
                true,
            )),
        };
        assert!(matches!(
            apply(&doc, &same_name),
            Err(CommandError::Invalid(Invalid::DuplicateAssemblyName { .. }))
        ));
        let missing = Command::DeleteJoint {
            assembly: a,
            joint: JointId(Id(99)),
        };
        assert!(matches!(
            apply(&doc, &missing),
            Err(CommandError::NoJoint { .. })
        ));
        let no_assembly = Command::DeleteAssembly {
            assembly: AssemblyId(Id(99)),
        };
        assert!(matches!(
            apply(&doc, &no_assembly),
            Err(CommandError::NoAssembly(_))
        ));
    }

    #[test]
    fn a_file_is_held_by_its_hash_and_kept_while_a_record_names_it() {
        let bytes = b"ISO-10303-21;".to_vec();
        let blob = BlobRef::of(&bytes, "step");
        let doc = run(
            &base(),
            Command::AddFile {
                bytes: bytes.clone(),
            },
        );
        assert_eq!(doc.blob(&blob), Some(&bytes[..]));
        // The same bytes again change nothing, and undoing that keeps the file.
        let twice = apply(
            &doc,
            &Command::AddFile {
                bytes: bytes.clone(),
            },
        )
        .unwrap();
        assert_eq!(twice.document, doc);
        assert_eq!(
            apply(&twice.document, &twice.inverse).unwrap().document,
            doc
        );
        // A record naming a file the document lacks is malformed.
        let record = FeatureRecord {
            type_id: FeatureTypeId::new("core.import-step").unwrap(),
            params: BTreeMap::new(),
            file: Some(ImportedFile {
                name: "servo.step".into(),
                blob: blob.clone(),
            }),
            ..plane(30, "servo", &[])
        };
        let lacking = add_feature(10, 0, record.clone());
        assert!(matches!(
            apply(&base(), &lacking),
            Err(CommandError::Invalid(Invalid::FileMissing { .. }))
        ));
        let named = run(&doc, add_feature(10, 0, record));
        let refused = apply(&named, &Command::RemoveFile { blob: blob.clone() }).unwrap_err();
        assert!(
            matches!(refused, CommandError::FileInUse { .. }),
            "{refused}"
        );
        // An import is one gesture: the file, then the record.
        let import = Command::Group {
            label: "import servo.step".into(),
            commands: vec![
                Command::AddFile {
                    bytes: bytes.clone(),
                },
                add_feature(
                    10,
                    0,
                    FeatureRecord {
                        file: Some(ImportedFile {
                            name: "servo.step".into(),
                            blob: blob.clone(),
                        }),
                        type_id: FeatureTypeId::new("core.import-step").unwrap(),
                        params: BTreeMap::new(),
                        ..plane(31, "servo", &[])
                    },
                ),
            ],
        };
        let imported = run(&base(), import.clone());
        assert!(imported.blob(&blob).is_some());
        crate::session::crosses(&crate::Request::Submit(env(ALICE, 0, import)), "request");
        let gone = Command::RemoveFile {
            blob: BlobRef::of(b"nothing", "step"),
        };
        assert!(matches!(apply(&doc, &gone), Err(CommandError::NoFile(_))));
    }

    #[test]
    fn the_file_crosses_as_base64() {
        let c = Command::AddFile {
            bytes: b"STEP".to_vec(),
        };
        let json = serde_json::to_string(&c).unwrap();
        assert_eq!(json, r#"{"add_file":{"bytes":"U1RFUA=="}}"#);
        assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), c);
    }
}
