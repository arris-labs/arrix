use std::collections::BTreeMap;

use arrix_core::{FeatureId, Id, IdMinter, ParamId, PartId, QuantityKind, Ref, SlotName};
use arrix_sketch::{Constraint, Draft, Entity, EntityId, Point, Sketch, SketchEdit};

use super::*;
use crate::authority::{AuthorId, Authority, CommandEnvelope, Outcome};
use crate::command::{Command, FeatureEdit};
use crate::document::{FeatureRecord, FeatureTypeId, Param, Part};
use crate::expr::Expr;
use crate::open::open;

const ME: AuthorId = AuthorId(Id(1));

fn text(files: &MemorySource, path: &str) -> String {
    String::from_utf8(files.0[path].clone()).unwrap()
}

#[test]
fn writes_sorted_keys_two_spaces_shortest_floats_and_a_newline() {
    #[derive(serde::Serialize)]
    struct S {
        zeta: f64,
        alpha: Vec<f64>,
        mid: Option<u8>,
    }
    let bytes = to_json(&S {
        zeta: 0.1 + 0.2,
        alpha: vec![1.0, 1e-7, 12.5],
        mid: None,
    });
    assert_eq!(
        String::from_utf8(bytes).unwrap(),
        "{\n  \"alpha\": [\n    1.0,\n    1e-7,\n    12.5\n  ],\n  \"mid\": null,\n  \"zeta\": 0.30000000000000004\n}\n"
    );
}

fn param(name: &str, kind: QuantityKind, e: &str) -> Param {
    Param {
        name: name.into(),
        kind,
        expr: Expr::parse(e).unwrap(),
    }
}

fn feature(
    id: FeatureId,
    type_id: &str,
    name: &str,
    params: &[(&str, &str)],
    inputs: Vec<(&str, Ref)>,
) -> FeatureRecord {
    FeatureRecord {
        id,
        type_id: FeatureTypeId::new(type_id).unwrap(),
        type_version: 1,
        name: name.into(),
        params: params
            .iter()
            .map(|(k, v)| (k.to_string(), Expr::parse(v).unwrap()))
            .collect(),
        choices: BTreeMap::new(),
        inputs: inputs.into_iter().map(|(k, r)| (k.into(), r)).collect(),
        suppressed: false,
        sketch: None,
    }
}

/// The acceptance scenario's commands, as far as the document goes: the
/// parameters, a datum plane, a gear on it, a plane on the gear's top face,
/// then the edits.
fn scenario() -> Vec<Command> {
    let mut ids = IdMinter::new(42);
    let (teeth, m, w): (ParamId, ParamId, ParamId) = (ids.mint(), ids.mint(), ids.mint());
    let part: PartId = ids.mint();
    let (lower, gear, upper): (FeatureId, FeatureId, FeatureId) =
        (ids.mint(), ids.mint(), ids.mint());
    let top = format!("face:sweep.{gear}.end-cap").parse().unwrap();
    let plane = SlotName::new("plane").unwrap();
    vec![
        Command::AddParam {
            param: teeth,
            record: param("teeth", QuantityKind::Count, "20"),
        },
        Command::AddParam {
            param: m,
            record: param("m", QuantityKind::Length, "1 mm"),
        },
        Command::AddParam {
            param: w,
            record: param("w", QuantityKind::Length, "8 mm"),
        },
        Command::AddPart {
            part: Part {
                id: part,
                name: "Gearbox".into(),
                history: vec![],
                features: BTreeMap::new(),
                rollback: None,
            },
        },
        Command::AddFeature {
            part,
            at: 0,
            record: feature(
                lower,
                "core.datum-plane",
                "Lower",
                &[("offset", "10 mm")],
                vec![],
            ),
        },
        Command::AddFeature {
            part,
            at: 1,
            record: feature(
                gear,
                "gears.spur",
                "Spur gear",
                &[
                    ("teeth", "teeth"),
                    ("module", "m"),
                    ("width", "w"),
                    ("pressure-angle", "20 deg"),
                ],
                vec![(
                    "plane",
                    Ref::Slot {
                        feature: lower,
                        slot: plane.clone(),
                    },
                )],
            ),
        },
        Command::AddFeature {
            part,
            at: 2,
            record: feature(
                upper,
                "core.datum-plane",
                "Upper",
                &[("offset", "2 mm")],
                vec![("plane", Ref::Topo(top))],
            ),
        },
        Command::SetParam {
            param: w,
            expr: Expr::parse("12 mm").unwrap(),
        },
        Command::SetParam {
            param: teeth,
            expr: Expr::parse("24").unwrap(),
        },
        Command::EditFeature {
            feature: upper,
            edit: FeatureEdit {
                name: Some("Upper plane".into()),
                ..FeatureEdit::default()
            },
        },
        Command::SetRollback { part, at: Some(2) },
        Command::ReorderFeature {
            feature: upper,
            to: 2,
        },
        Command::DeleteFeature { feature: upper },
    ]
}

#[test]
fn saving_is_deterministic_and_undo_restores_every_earlier_file() {
    let mut auth = Authority::default();
    let mut saves = vec![save(auth.document())];
    for command in scenario() {
        let envelope = CommandEnvelope {
            author: ME,
            base: auth.generation(),
            command,
        };
        match auth.submit(&envelope) {
            Ok(Outcome::Applied(_)) => {}
            Ok(Outcome::NoOp) => continue,
            other => panic!("{:?}: {other:?}", envelope.command),
        }
        let files = save(auth.document());
        assert_eq!(save(auth.document()).0, files.0, "saved twice");
        let reopened = open(&files).unwrap();
        assert_eq!(&reopened, auth.document());
        assert_eq!(save(&reopened).0, files.0, "saved, opened, saved");
        saves.push(files);
    }
    assert_eq!(saves.len(), 13, "the reorder to where it is is a no-op");
    for want in saves.iter().rev().skip(1) {
        auth.undo(ME).unwrap();
        assert_eq!(&save(auth.document()).0, &want.0);
    }
    for want in saves.iter().skip(1) {
        auth.redo(ME).unwrap();
        assert_eq!(&save(auth.document()).0, &want.0);
    }
}

#[test]
fn writes_the_documented_layout() {
    let mut doc = crate::document::Document::default();
    for command in scenario().into_iter().take(7) {
        doc = crate::command::apply(&doc, &command).unwrap().document;
    }
    let files = save(&doc);
    let paths: Vec<&str> = files.0.keys().map(String::as_str).collect();
    let part = doc.parts.keys().next().unwrap();
    assert_eq!(
        paths,
        [
            DOCUMENT_JSON.to_owned(),
            PARAMS_JSON.into(),
            format!("parts/{part}.json")
        ]
    );
    assert_eq!(text(&files, DOCUMENT_JSON), "{\n  \"schema\": 1\n}\n");
    let params = text(&files, PARAMS_JSON);
    assert!(
        params.contains("\"expr\": \"1 mm\",\n    \"kind\": \"length\",\n    \"name\": \"m\""),
        "{params}"
    );
    let part = text(&files, &format!("parts/{part}.json"));
    assert!(part.contains("\"type_id\": \"gears.spur\""), "{part}");
    assert!(part.contains("\"rollback\": null"), "{part}");
}

/// The `SketchEdit` that takes the sketch from `before` to `now`, which
/// becomes the next `before`: how a client makes one command of a gesture
/// it ran on its own draft, positions solved there.
fn gesture(feature: FeatureId, before: &mut Sketch, now: &Sketch) -> Command {
    let edit = SketchEdit::diff(before, now);
    *before = now.clone();
    Command::SketchEdit { feature, edit }
}

/// A sketched rectangle dimensioned by `w` and `h`, then edited the ways a
/// client edits a sketch.
fn sketch_scenario() -> Vec<Command> {
    let mut ids = IdMinter::new(7);
    let (w, h): (ParamId, ParamId) = (ids.mint(), ids.mint());
    let part: PartId = ids.mint();
    let sketch: FeatureId = ids.mint();
    let mut d = Draft::seeded(99);
    let mut before = Sketch::new();

    let corners = [(0.0, 0.0), (0.04, 0.0), (0.04, 0.03), (0.0, 0.03)];
    let p = corners.map(|(x, y)| d.add_point(Point::new(x, y)));
    let l: Vec<EntityId> = (0..4)
        .map(|i| {
            d.add_entity(Entity::Line {
                start: p[i],
                end: p[(i + 1) % 4],
            })
        })
        .collect();
    for (i, line) in l.iter().enumerate() {
        d.add_constraint(if i % 2 == 0 {
            Constraint::Horizontal { line: *line }
        } else {
            Constraint::Vertical { line: *line }
        });
    }
    let dw = d.add_constraint(Constraint::Distance {
        a: p[0],
        b: p[1],
        value: 0.04,
    });
    let dh = d.add_constraint(Constraint::Distance {
        a: p[1],
        b: p[2],
        value: 0.03,
    });
    d.set_constraint_expr(dw, Some("w".into()));
    d.set_constraint_expr(dh, Some("h".into()));
    let draw = gesture(sketch, &mut before, &d);

    d.get_constraint_mut(dw)
        .unwrap()
        .set_dimensional_value(0.05);
    assert!(d.solve().converged);
    let resolve = gesture(sketch, &mut before, &d);

    d.set_construction(l[3], true);
    let construction = gesture(sketch, &mut before, &d);

    d.set_constraint_expr(dh, Some("h / 2".into()));
    let half = gesture(sketch, &mut before, &d);

    let remove = Command::SketchEdit {
        feature: sketch,
        edit: SketchEdit::remove(&d, &[l[2]].into()),
    };
    vec![
        Command::AddParam {
            param: w,
            record: param("w", QuantityKind::Length, "40 mm"),
        },
        Command::AddParam {
            param: h,
            record: param("h", QuantityKind::Length, "30 mm"),
        },
        Command::AddPart {
            part: Part {
                id: part,
                name: "Plate".into(),
                history: vec![],
                features: BTreeMap::new(),
                rollback: None,
            },
        },
        Command::AddFeature {
            part,
            at: 0,
            record: FeatureRecord {
                sketch: Some(Box::default()),
                ..feature(sketch, "core.sketch", "Sketch", &[], vec![])
            },
        },
        draw,
        Command::SetParam {
            param: w,
            expr: Expr::parse("50 mm").unwrap(),
        },
        resolve,
        construction,
        half,
        remove,
    ]
}

#[test]
fn a_sketch_saves_opens_saves_the_same_and_undo_restores_every_earlier_file() {
    let mut auth = Authority::default();
    let mut saves = vec![save(auth.document())];
    for command in sketch_scenario() {
        let json = serde_json::to_string(&command).unwrap();
        assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);
        let envelope = CommandEnvelope {
            author: ME,
            base: auth.generation(),
            command,
        };
        let inverse = crate::command::apply(auth.document(), &envelope.command)
            .unwrap()
            .inverse;
        let json = serde_json::to_string(&inverse).unwrap();
        assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), inverse);
        match auth.submit(&envelope) {
            Ok(Outcome::Applied(_)) => {}
            other => panic!("{:?}: {other:?}", envelope.command),
        }
        let files = save(auth.document());
        let reopened = open(&files).unwrap();
        assert_eq!(&reopened, auth.document());
        assert_eq!(save(&reopened).0, files.0, "saved, opened, saved");
        saves.push(files);
    }
    let part = auth.document().parts.keys().next().unwrap();
    let text = text(saves.last().unwrap(), &format!("parts/{part}.json"));
    assert!(text.contains("\"expr\": \"h / 2\""), "{text}");
    for want in saves.iter().rev().skip(1) {
        auth.undo(ME).unwrap();
        assert_eq!(&save(auth.document()).0, &want.0);
    }
    for want in saves.iter().skip(1) {
        auth.redo(ME).unwrap();
        assert_eq!(&save(auth.document()).0, &want.0);
    }
}
