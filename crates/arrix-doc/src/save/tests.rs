use std::collections::BTreeMap;

use arrix_core::{FeatureId, Id, IdMinter, ParamId, PartId, QuantityKind, Ref, SlotName};

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
