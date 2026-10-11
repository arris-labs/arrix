//! `core.import-step` through the evaluator (ADR-0029): the servo of
//! `tests/fixtures/step/` imported from the file the record keeps, a
//! fillet on one of its edges by name surviving a re-evaluation, and the
//! refusals.

use std::collections::BTreeMap;
use std::f64::consts::PI;
use std::path::Path;

use arrix_core::{FeatureId, Id, PartId, PersistentName, Ref, SlotName, TopoKind};
use arrix_doc::{
    Command, Document, Evaluation, Evaluator, FeatureEdit, FeatureOutcome, FeatureRecord,
    FeatureTypeId, ImportedFile, Part, Registry, SlotView, apply, expr::Expr,
};
use arrix_kernel::Kernel;

const PART: PartId = PartId(Id(1));
const IMPORT: FeatureId = FeatureId(Id(2));
const FILLET: FeatureId = FeatureId(Id(3));
const SERVO_MM3: f64 = 40.0 * 20.0 * 36.0 + PI * 9.0 * 6.0 - PI * 4.0 * 36.0;

fn servo() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/step/servo.step");
    std::fs::read(path).expect("the servo fixture")
}

fn record(id: FeatureId, type_id: &str, name: &str) -> FeatureRecord {
    FeatureRecord {
        id,
        type_id: FeatureTypeId::new(type_id).unwrap(),
        type_version: 1,
        name: name.into(),
        params: BTreeMap::new(),
        choices: BTreeMap::new(),
        inputs: BTreeMap::new(),
        suppressed: false,
        sketch: None,
        frozen: None,
        file: None,
    }
}

fn with(doc: &Document, command: Command) -> Document {
    apply(doc, &command)
        .unwrap_or_else(|e| panic!("{command:?}: {e}"))
        .document
}

/// A part holding the servo imported, as one gesture: the file, then the
/// record naming it.
fn imported(bytes: &[u8]) -> Document {
    let blob = arrix_doc::BlobRef::of(bytes, "step");
    let mut doc = with(
        &Document::default(),
        Command::AddPart {
            part: Part {
                id: PART,
                name: "Servo".into(),
                history: vec![],
                features: BTreeMap::new(),
                rollback: None,
            },
        },
    );
    let mut import = record(IMPORT, "core.import-step", "Servo");
    import.file = Some(ImportedFile {
        name: "servo.step".into(),
        blob,
    });
    doc = with(
        &doc,
        Command::Group {
            label: "import servo.step".into(),
            commands: vec![
                Command::AddFile {
                    bytes: bytes.to_vec(),
                },
                Command::AddFeature {
                    part: PART,
                    at: 0,
                    record: import,
                },
            ],
        },
    );
    doc
}

fn body_slot() -> Ref {
    Ref::Slot {
        feature: IMPORT,
        slot: SlotName::new("body").unwrap(),
    }
}

fn fillet(edge: PersistentName) -> FeatureRecord {
    let mut r = record(FILLET, "core.fillet", "Fillet");
    r.params
        .insert("radius".into(), Expr::parse("1 mm").unwrap());
    r.inputs.insert("target".into(), body_slot().into());
    r.inputs
        .insert("edges".into(), vec![Ref::Topo(edge)].into());
    r
}

fn eval(doc: &Document) -> Evaluation {
    Evaluator::new(Registry::with_core_types()).evaluate(doc)
}

fn volume(e: &Evaluation, feature: FeatureId) -> f64 {
    match e.slot(feature, "body") {
        Some(SlotView::Body(m)) => m.volume * 1e9,
        other => panic!("{:?}: {other:?}", e.outcome(feature)),
    }
}

fn code(e: &Evaluation, feature: FeatureId) -> &str {
    match e.outcome(feature) {
        Some(FeatureOutcome::Failed { diagnostic, .. }) => diagnostic.code.as_str(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_servo_is_read_from_the_file_the_record_keeps() {
    let e = eval(&imported(&servo()));
    let v = volume(&e, IMPORT);
    assert!((v - SERVO_MM3).abs() < 1e-6 * SERVO_MM3, "{v} mm³");
    let Some(SlotView::Body(m)) = e.slot(IMPORT, "body") else {
        panic!("{:?}", e.outcome(IMPORT))
    };
    assert_eq!((m.faces, m.edges), (9, 18), "{m:?}");
}

#[test]
fn a_fillet_on_an_imported_edge_by_name_survives_a_re_evaluation() {
    let bytes = servo();
    let doc = imported(&bytes);
    // The edges the import names, in the kernel's order; the first one a
    // 1 mm fillet can round is the test's.
    let mut kernel = Kernel::new();
    let body = kernel.import_step(IMPORT, &bytes, 1).unwrap();
    let edges: Vec<PersistentName> = kernel
        .names(body)
        .unwrap()
        .of_kind(TopoKind::Edge)
        .cloned()
        .collect();
    assert!(
        edges
            .iter()
            .all(|n| n.to_string().starts_with("edge:import.")),
        "{edges:?}"
    );
    let rounded = edges
        .iter()
        .find_map(|edge| {
            let with_fillet = with(
                &doc,
                Command::AddFeature {
                    part: PART,
                    at: 1,
                    record: fillet(edge.clone()),
                },
            );
            let e = eval(&with_fillet);
            matches!(e.outcome(FILLET), Some(FeatureOutcome::Ok { .. }))
                .then(|| (with_fillet, volume(&e, FILLET)))
        })
        .expect("some edge of the servo takes a 1 mm fillet");
    let (doc, first) = rounded;
    assert!(first < SERVO_MM3, "{first}");

    // A second evaluator, from the same document, reads the same names.
    assert_eq!(volume(&eval(&doc), FILLET), first);
    // The same evaluator again is served from its cache.
    let mut evaluator = Evaluator::new(Registry::with_core_types());
    evaluator.evaluate(&doc);
    let again = evaluator.evaluate(&doc);
    assert_eq!(volume(&again, FILLET), first);
    assert!(matches!(
        again.outcome(IMPORT),
        Some(FeatureOutcome::Ok { cached: true, .. })
    ));
    // The solid edited to the same value is still the same result.
    let edited = with(
        &doc,
        Command::EditFeature {
            feature: IMPORT,
            edit: FeatureEdit {
                params: BTreeMap::from([("solid".into(), Some(Expr::parse("1").unwrap()))]),
                ..FeatureEdit::default()
            },
        },
    );
    assert_eq!(volume(&eval(&edited), FILLET), first);
}

#[test]
fn a_file_changes_the_result_and_a_missing_one_or_solid_is_named() {
    let doc = imported(&servo());
    let hash = |e: &Evaluation| match e.outcome(IMPORT) {
        Some(FeatureOutcome::Ok { hash, .. }) => *hash,
        other => panic!("{other:?}"),
    };
    let other = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/step/half-ball.stp"),
    )
    .unwrap();
    assert_ne!(hash(&eval(&doc)), hash(&eval(&imported(&other))));

    let past = with(
        &doc,
        Command::EditFeature {
            feature: IMPORT,
            edit: FeatureEdit {
                params: BTreeMap::from([("solid".into(), Some(Expr::parse("2").unwrap()))]),
                ..FeatureEdit::default()
            },
        },
    );
    assert_eq!(code(&eval(&past), IMPORT), "import.no-solid");
    let zero = with(
        &doc,
        Command::EditFeature {
            feature: IMPORT,
            edit: FeatureEdit {
                params: BTreeMap::from([("solid".into(), Some(Expr::parse("0").unwrap()))]),
                ..FeatureEdit::default()
            },
        },
    );
    assert_eq!(code(&eval(&zero), IMPORT), "import.no-solid");

    // A record with no file.
    let bare = with(
        &with(
            &Document::default(),
            Command::AddPart {
                part: Part {
                    id: PART,
                    name: "Servo".into(),
                    history: vec![],
                    features: BTreeMap::new(),
                    rollback: None,
                },
            },
        ),
        Command::AddFeature {
            part: PART,
            at: 0,
            record: record(IMPORT, "core.import-step", "Servo"),
        },
    );
    assert_eq!(code(&eval(&bare), IMPORT), "import.no-file");
    // Bytes that are no exchange structure.
    assert_eq!(
        code(&eval(&imported(b"not a step file")), IMPORT),
        "kernel.import.parse"
    );
}
