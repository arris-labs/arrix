//! Materials in the document (plans/c2-m3 step 4, ADR-0031): the four
//! commands and their inverses, `material.in-use`, a part's edge to its
//! materials in the DAG, and the `.arrx` and directory forms round-tripping.

use arrix_core::QuantityKind;
use arrix_core::{FeatureId, Id, MaterialId, ParamId, PartId, SlotName};
use arrix_doc::{
    Command, CommandError, Dag, Document, FeatureRecord, FeatureTypeId, Invalid, Material, Node,
    Param, Part, Rgb, SlotRef, apply, expr::Expr, from_zip, open, save, to_zip,
};

const STEEL: MaterialId = MaterialId(Id(1));
const PLA: MaterialId = MaterialId(Id(2));
const PART: PartId = PartId(Id(10));
const FEATURE: FeatureId = FeatureId(Id(11));
const DENSITY: ParamId = ParamId(Id(5));

fn material(id: MaterialId, name: &str, density: Option<&str>) -> Material {
    Material {
        id,
        name: name.into(),
        density: density.map(|d| Expr::parse(d).unwrap()),
        appearance: Rgb([0.8, 0.8, 0.85]),
    }
}

fn body() -> SlotRef {
    SlotRef {
        feature: FEATURE,
        slot: SlotName::new("body").unwrap(),
    }
}

fn with(doc: &Document, command: Command) -> Document {
    apply(doc, &command)
        .unwrap_or_else(|e| panic!("{command:?}: {e}"))
        .document
}

fn refused(doc: &Document, command: Command) -> CommandError {
    apply(doc, &command).expect_err("the command is refused")
}

/// A part of one feature, and a parameter for a density to read.
fn base() -> Document {
    let doc = with(
        &Document::default(),
        Command::AddParam {
            param: DENSITY,
            record: Param {
                name: "rho".into(),
                kind: QuantityKind::Density,
                expr: Expr::parse("7.85 g/cm3").unwrap(),
            },
        },
    );
    let record = FeatureRecord {
        id: FEATURE,
        type_id: FeatureTypeId::new("core.sketch").unwrap(),
        type_version: 1,
        name: "Sketch".into(),
        params: Default::default(),
        choices: Default::default(),
        inputs: Default::default(),
        suppressed: false,
        sketch: None,
        frozen: None,
        file: None,
    };
    with(
        &doc,
        Command::AddPart {
            part: Part {
                id: PART,
                name: "Plate".into(),
                history: vec![FEATURE],
                features: [(FEATURE, record)].into(),
                rollback: None,
                material: None,
                body_materials: Default::default(),
            },
        },
    )
}

fn add(doc: &Document, m: Material) -> Document {
    with(doc, Command::AddMaterial { material: m })
}

fn assign(part: PartId, body: Option<SlotRef>, material: Option<MaterialId>) -> Command {
    Command::AssignMaterial {
        part,
        body,
        material,
    }
}

#[test]
fn every_command_undoes_to_the_document_before() {
    let start = add(&base(), material(STEEL, "Steel", Some("7850 kg/m3")));
    let commands = [
        Command::AddMaterial {
            material: material(PLA, "PLA", Some("1.24 g/cm3")),
        },
        Command::EditMaterial {
            material: material(STEEL, "Steel", Some("rho")),
        },
        assign(PART, None, Some(STEEL)),
        assign(PART, Some(body()), Some(STEEL)),
    ];
    for command in commands {
        let applied = apply(&start, &command).unwrap();
        assert_ne!(applied.document, start, "{command:?} changes the document");
        let back = apply(&applied.document, &applied.inverse).unwrap();
        assert_eq!(back.document, start, "{command:?}");
        let again = apply(&back.document, &back.inverse).unwrap();
        assert_eq!(again.document, applied.document, "redo of {command:?}");
    }
    let unused = apply(&start, &Command::DeleteMaterial { material: STEEL }).unwrap();
    assert!(unused.document.materials().is_empty());
    assert_eq!(
        apply(&unused.document, &unused.inverse).unwrap().document,
        start
    );
}

#[test]
fn assigning_clears_and_replaces() {
    let doc = add(
        &add(&base(), material(STEEL, "Steel", None)),
        material(PLA, "PLA", None),
    );
    let doc = with(&doc, assign(PART, None, Some(STEEL)));
    let doc = with(&doc, assign(PART, Some(body()), Some(PLA)));
    let part = &doc.parts()[&PART];
    assert_eq!(part.material, Some(STEEL));
    assert_eq!(part.body_materials[&body()], PLA);
    let doc = with(&doc, assign(PART, Some(body()), None));
    assert!(doc.parts()[&PART].body_materials.is_empty());
    let doc = with(&doc, assign(PART, None, None));
    assert_eq!(doc.parts()[&PART].material, None);
}

#[test]
fn a_material_a_part_is_made_of_cannot_be_deleted() {
    let doc = add(&base(), material(STEEL, "Steel", None));
    for target in [None, Some(body())] {
        let used = with(&doc, assign(PART, target, Some(STEEL)));
        let err = refused(&used, Command::DeleteMaterial { material: STEEL });
        assert_eq!(
            err,
            CommandError::MaterialInUse {
                material: STEEL,
                part: PART
            }
        );
        assert_eq!(err.diagnostic().code.as_str(), "material.in-use");
    }
}

#[test]
fn what_is_missing_or_taken_is_refused() {
    let doc = add(&base(), material(STEEL, "Steel", None));
    assert_eq!(
        refused(
            &doc,
            Command::AddMaterial {
                material: material(STEEL, "Other", None)
            }
        ),
        CommandError::IdTaken(STEEL.0)
    );
    assert_eq!(
        refused(
            &doc,
            Command::EditMaterial {
                material: material(PLA, "PLA", None)
            }
        ),
        CommandError::NoMaterial(PLA)
    );
    assert_eq!(
        refused(&doc, Command::DeleteMaterial { material: PLA }),
        CommandError::NoMaterial(PLA)
    );
    assert_eq!(
        refused(&doc, assign(PART, None, Some(PLA))),
        CommandError::NoMaterial(PLA)
    );
    assert_eq!(
        refused(&doc, assign(PartId(Id(99)), None, Some(STEEL))),
        CommandError::NoPart(PartId(Id(99)))
    );
    // A second material may not take a name.
    let err = refused(
        &doc,
        Command::AddMaterial {
            material: material(PLA, "Steel", None),
        },
    );
    assert!(
        matches!(err, CommandError::Invalid(Invalid::DuplicateMaterial(_))),
        "{err}"
    );
    // A colour is within 0 to 1, a density an expression that parses.
    let mut loud = material(PLA, "Loud", None);
    loud.appearance = Rgb([2.0, 0.0, 0.0]);
    let err = refused(&doc, Command::AddMaterial { material: loud });
    assert!(
        matches!(err, CommandError::Invalid(Invalid::MaterialColour { .. })),
        "{err}"
    );
    // A body override names a feature of its part.
    let stray = SlotRef {
        feature: FeatureId(Id(77)),
        slot: SlotName::new("body").unwrap(),
    };
    let err = refused(&doc, assign(PART, Some(stray), Some(STEEL)));
    assert!(
        matches!(err, CommandError::Invalid(Invalid::BodyMaterial { .. })),
        "{err}"
    );
}

#[test]
fn a_group_may_delete_the_assignment_then_the_material() {
    let doc = with(
        &add(&base(), material(STEEL, "Steel", None)),
        assign(PART, None, Some(STEEL)),
    );
    let group = Command::Group {
        label: "Remove steel".into(),
        commands: vec![
            assign(PART, None, None),
            Command::DeleteMaterial { material: STEEL },
        ],
    };
    let applied = apply(&doc, &group).unwrap();
    assert!(applied.document.materials().is_empty());
    assert_eq!(
        apply(&applied.document, &applied.inverse).unwrap().document,
        doc
    );
}

#[test]
fn a_part_reads_its_materials_and_a_material_its_parameters() {
    let doc = add(&base(), material(STEEL, "Steel", Some("rho")));
    let doc = with(&doc, assign(PART, None, Some(STEEL)));
    let dag = Dag::build(&doc).unwrap();
    let reads: Vec<Node> = dag.reads(Node::Part(PART)).collect();
    assert_eq!(reads, [Node::Material(STEEL)]);
    let reads: Vec<Node> = dag.reads(Node::Material(STEEL)).collect();
    assert_eq!(reads, [Node::Param(DENSITY)]);
    let order = dag.order();
    let at = |n: Node| order.iter().position(|o| *o == n).unwrap();
    assert!(at(Node::Param(DENSITY)) < at(Node::Material(STEEL)));
    assert!(at(Node::Material(STEEL)) < at(Node::Part(PART)));
    // An edit of the density touches the material, and the part reads it.
    let edit = Command::EditMaterial {
        material: material(STEEL, "Steel", Some("8 g/cm3")),
    };
    let applied = apply(&doc, &edit).unwrap();
    assert!(applied.touched.contains(&Node::Material(STEEL)));
    assert!(
        dag.readers(Node::Material(STEEL))
            .any(|n| n == Node::Part(PART))
    );
}

#[test]
fn materials_round_trip_through_the_directory_and_the_zip() {
    let doc = add(
        &add(&base(), material(STEEL, "Steel", Some("rho"))),
        material(PLA, "PLA", None),
    );
    let doc = with(&doc, assign(PART, None, Some(STEEL)));
    let doc = with(&doc, assign(PART, Some(body()), Some(PLA)));
    let files = save(&doc);
    assert!(files.0.contains_key(&format!("materials/{STEEL}.json")));
    assert!(files.0.contains_key(&format!("materials/{PLA}.json")));
    let pla = String::from_utf8(files.0[&format!("materials/{PLA}.json")].clone()).unwrap();
    assert!(
        !pla.contains("density"),
        "an unset density is omitted: {pla}"
    );
    let part = String::from_utf8(files.0[&format!("parts/{PART}.json")].clone()).unwrap();
    assert!(part.contains(&format!("\"{FEATURE}.body\"")), "{part}");
    let opened = open(&files).unwrap();
    assert_eq!(opened, doc);
    assert_eq!(
        save(&opened).0,
        files.0,
        "saving what was loaded gives what was read"
    );
    let zip = to_zip(&files).unwrap();
    assert_eq!(open(&from_zip(&zip).unwrap()).unwrap(), doc);
    // A document without a material writes none of these.
    let plain = save(&base());
    assert!(plain.0.keys().all(|p| !p.starts_with("materials/")));
    assert!(
        !String::from_utf8(plain.0[&format!("parts/{PART}.json")].clone())
            .unwrap()
            .contains("material")
    );
}
