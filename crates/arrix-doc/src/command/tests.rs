use std::collections::BTreeMap;

use arrix_core::{FeatureId, Id, IdMinter, ParamId, PartId, QuantityKind, Ref};
use proptest::prelude::*;

use super::*;
use crate::authority::{AuthorId, Authority, CommandEnvelope, Generation, Outcome, Rejected};
use crate::document::FeatureTypeId;

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
            .map(|(k, r)| (k.to_string(), r.clone()))
            .collect(),
        suppressed: false,
    }
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
    match kind % 11 {
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
                suppressed: (b % 2 == 0).then_some(a % 2 == 0),
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
            match auth.submit(&env(ALICE, auth.generation().0, c)).unwrap() {
                Outcome::Applied(_) => states.push(auth.document().clone()),
                Outcome::NoOp => prop_assert_eq!(&a.document, &doc),
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
