//! `arrix eval --save-fixtures` as a user runs it (plans/c1-m2-naming
//! step 10, docs/ARCHITECTURE.md §The kernel choke point, "Fixture
//! export"): a document whose plate is filleted at 30 mm fails at the
//! kernel as `kernel.degenerate.blend-too-large`; its record is written as
//! a recipe. The grammar is checked at the kernel (`arrix-kernel`'s
//! `fixture_export`, over the vendored Arris reader), where the recipe of
//! the same call is a golden: Arris types stay in `arrix-kernel`, and this
//! test holds the document's recipe equal to that golden.

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use arrix_core::{
    CurveKey, FeatureId, IdMinter, NameRoot, PartId, PersistentName, Ref, SlotName, SweepPartName,
    TopoKind,
};
use arrix_doc::{
    Document, Evaluator, FeatureRecord, FeatureTypeId, Part, Registry, SlotView, apply, expr::Expr,
    save,
};
use arrix_sketch::{Draft, Entity, Point, Sketch};
use common::scenario_directory;

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn arrix_eval(doc: &str, extra: &[&Path]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_arrix"));
    cmd.current_dir(workspace()).arg("eval").arg(doc);
    for dir in extra {
        cmd.arg("--save-fixtures").arg(dir);
    }
    cmd.output().expect("run arrix")
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("save-fixtures")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// The directories under `dir`, by name.
fn recipes(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("the fixtures directory exists")
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

const MM: f64 = 1e-3;

/// The scenario, `tests/docs/oversize-fillet/`: a 40 × 30 mm plate 5 mm
/// thick, and a fillet of 30 mm on its top edge along the sketch's bottom
/// side, which no 5 mm face can hold. It is what the commands save.
fn oversize_fillet() -> arrix_doc::MemorySource {
    let mut ids = IdMinter::new(2027);
    let (part, sketch_id, plate_id, fillet_id): (PartId, FeatureId, FeatureId, FeatureId) =
        (ids.mint(), ids.mint(), ids.mint(), ids.mint());
    let mut d = Draft::seeded(5);
    let p = [(0.0, 0.0), (40.0, 0.0), (40.0, 30.0), (0.0, 30.0)]
        .map(|(x, y)| d.add_point(Point::new(x * MM, y * MM)));
    let sides: Vec<_> = (0..4)
        .map(|i| {
            d.add_entity(Entity::Line {
                start: p[i],
                end: p[(i + 1) % 4],
            })
        })
        .collect();
    let sketch: Sketch = d.sketch;
    let record = |id: FeatureId, ty: &str, name: &str| FeatureRecord {
        id,
        type_id: FeatureTypeId::new(ty).unwrap(),
        type_version: 1,
        name: name.into(),
        params: BTreeMap::new(),
        choices: BTreeMap::new(),
        inputs: BTreeMap::new(),
        suppressed: false,
        sketch: None,
        frozen: None,
        file: None,
    };
    let mut sketch_record = record(sketch_id, "core.sketch", "Sketch");
    sketch_record.sketch = Some(Box::new(sketch));
    let features = |plate: FeatureRecord, fillet: FeatureRecord| Part {
        id: part,
        name: "Plate".into(),
        history: vec![sketch_id, plate_id, fillet_id],
        features: [sketch_record.clone(), plate, fillet]
            .into_iter()
            .map(|f| (f.id, f))
            .collect(),
        rollback: None,
    };
    let mut plate = record(plate_id, "core.extrude", "Plate");
    plate
        .params
        .insert("distance".into(), Expr::parse("5 mm").unwrap());
    plate.choices.insert("mode".into(), "new".into());
    // The sketch's one region, as evaluating it gives it.
    let doc = apply(
        &Document::default(),
        &arrix_doc::Command::AddPart {
            part: features(plate.clone(), record(fillet_id, "core.fillet", "Fillet")),
        },
    )
    .unwrap()
    .document;
    let key = match Evaluator::new(Registry::with_core_types())
        .evaluate(&doc)
        .slot(sketch_id, "sketch")
    {
        Some(SlotView::Sketch(v)) => v.regions[0].key.clone(),
        other => panic!("{other:?}"),
    };
    plate.inputs.insert(
        "region".into(),
        Ref::Region {
            feature: sketch_id,
            key,
        }
        .into(),
    );
    let mut fillet = record(fillet_id, "core.fillet", "Fillet");
    fillet
        .params
        .insert("radius".into(), Expr::parse("30 mm").unwrap());
    fillet.inputs.insert(
        "target".into(),
        Ref::Slot {
            feature: plate_id,
            slot: SlotName::new("body").unwrap(),
        }
        .into(),
    );
    let bottom = PersistentName {
        kind: TopoKind::Edge,
        root: NameRoot::Sweep {
            feature: plate_id,
            index: 0,
            part: SweepPartName::EndEdge(CurveKey(sides[0].0)),
        },
        chain: vec![],
    };
    fillet
        .inputs
        .insert("edges".into(), vec![Ref::Topo(bottom)].into());
    let doc = apply(
        &Document::default(),
        &arrix_doc::Command::AddPart {
            part: features(plate, fillet),
        },
    )
    .unwrap()
    .document;
    save(&doc)
}

#[test]
fn the_oversize_fillet_scenario_is_what_the_commands_save() {
    scenario_directory("oversize-fillet", &oversize_fillet());
}

const CODE: &str = "kernel.degenerate.blend-too-large";

#[test]
fn a_failed_kernel_call_is_written_as_a_recipe_arris_reads() {
    let dir = scratch("oversize-fillet");
    let out = arrix_eval("tests/docs/oversize-fillet", &[&dir]);
    assert_eq!(out.status.code(), Some(1));
    let line: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(line["status"], "failed");
    assert!(
        line["features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["category"] == CODE),
        "{line}"
    );

    // One failure at a kernel call: the plate's was made and read.
    let names = recipes(&dir);
    let [name] = names.as_slice() else {
        panic!("{names:?}")
    };
    let hash = name.strip_prefix(&format!("{CODE}-")).expect(name);
    assert_eq!(hash.len(), 12);
    assert!(hash.bytes().all(|b| b.is_ascii_hexdigit()), "{name}");

    let json = std::fs::read_to_string(dir.join(name).join("fixture.json")).unwrap();
    let recipe: serde_json::Value = serde_json::from_str(&json).unwrap();
    let ops: Vec<_> = recipe["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["op"].as_str().unwrap())
        .collect();
    assert_eq!(ops, ["profile", "extrude", "fillet"]);
    // The fillet is call 2 here: call 1 located its edge, which is no step.
    assert_eq!(recipe["result"], "c2");
    assert_eq!(recipe["steps"][2]["radius"], 0.03);
    // It is the recipe the kernel's own golden is, which the vendored
    // grammar reads and which states the refusal Arris should give.
    let golden = workspace().join("crates/arrix-kernel/tests/recipes/oversize-fillet/fixture.json");
    let golden: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(golden).unwrap()).unwrap();
    let renumbered = |mut r: serde_json::Value| {
        r["description"] = "".into();
        r["result"] = "".into();
        r["steps"][2]["name"] = "".into();
        r
    };
    assert_eq!(renumbered(recipe), renumbered(golden));
    // The name is the recipe's own hash: written again, it is the same.
    let again = scratch("oversize-fillet-again");
    arrix_eval("tests/docs/oversize-fillet", &[&again]);
    assert_eq!(recipes(&again), names);
}

#[test]
fn the_line_is_the_same_with_or_without_the_flag() {
    let dir = scratch("same-line");
    let with = arrix_eval("tests/docs/oversize-fillet", &[&dir]);
    let without = arrix_eval("tests/docs/oversize-fillet", &[]);
    assert_eq!(with.stdout, without.stdout);
    assert_eq!(with.status.code(), without.status.code());
}

#[test]
fn a_document_that_fails_at_no_kernel_call_writes_no_recipe() {
    let dir = scratch("no-call");
    let out = arrix_eval("tests/docs/slice", &[&dir]);
    assert_eq!(out.status.code(), Some(0));
    assert!(!dir.exists() || recipes(&dir).is_empty());
}
