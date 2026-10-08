//! Where a frozen feature's faces are (plans/m3-accept step 2): the slice's
//! gear, opened without `gears`, keeps the names its stored table gives it
//! (docs/DATA-MODEL.md §Frozen results), so its faces are found on the
//! final body by their root's feature after the join and a fillet
//! downstream, which is what the frozen badge is anchored on. Here
//! because the stored result is made by evaluating the plugin.
//!
//! The fillet is on an edge of the plate: an edge of the gear the join
//! left alone is named in the gear's consumed slot alone, and no
//! downstream feature reads it yet (docs/BACKLOG.md).

mod common;

use std::collections::BTreeMap;

use arrix_core::{
    FeatureId, MeshDetail, PersistentName, Ref, RenderMesh, SlotName, SweepPartName, TopoKind,
};
use arrix_doc::{
    Command, Document, Evaluator, FeatureOutcome, FeatureRecord, FeatureTypeId, MemorySource,
    Registry, apply, expr::Expr, open,
};
use common::{read_tree, registry, workspace};

fn fid(s: &str) -> FeatureId {
    s.parse().unwrap()
}

/// The slice's gear, and the plate it is joined to.
const GEAR: &str = "5BJX8TTST7RVD";
const PLATE: &str = "5BE3HE4D204QM";

/// The slice with the gear's frozen result in it, as the app saves it.
fn frozen_slice() -> Document {
    let mut files = BTreeMap::new();
    read_tree(&workspace().join("tests/docs/slice"), "", &mut files);
    let mut doc = open(&MemorySource(files)).unwrap();
    let mut live = Evaluator::new(registry());
    live.evaluate(&doc);
    for (feature, frozen) in live.frozen() {
        assert!(doc.freeze(*feature, frozen));
    }
    doc
}

/// Every final body's refined mesh.
fn meshes(e: &mut Evaluator) -> Vec<RenderMesh> {
    let finals = e.final_bodies().to_vec();
    finals
        .iter()
        .map(|f| e.mesh(f, MeshDetail::Refined).unwrap())
        .collect()
}

/// The faces and edges of `meshes` whose name starts at `feature`.
fn rooted_at(meshes: &[RenderMesh], feature: FeatureId) -> (Vec<PersistentName>, usize) {
    let faces = meshes
        .iter()
        .flat_map(|m| &m.faces)
        .filter(|r| r.name.root.feature() == feature)
        .map(|r| r.name.clone())
        .collect();
    let edges = meshes
        .iter()
        .flat_map(|m| &m.edges)
        .filter(|r| r.name.root.feature() == feature)
        .count();
    (faces, edges)
}

/// A fillet of `radius` on `edge`, on the plate's body.
fn fillet(edge: PersistentName, radius: &str) -> FeatureRecord {
    let target = Ref::Slot {
        feature: fid(PLATE),
        slot: SlotName::new("body").unwrap(),
    };
    FeatureRecord {
        id: fid("7FILLETPR0BE0"),
        type_id: FeatureTypeId::new("core.fillet").unwrap(),
        type_version: 1,
        name: "Fillet".into(),
        params: BTreeMap::from([("radius".into(), Expr::parse(radius).unwrap())]),
        choices: BTreeMap::new(),
        inputs: BTreeMap::from([
            ("target".into(), target.into()),
            ("edges".into(), vec![Ref::Topo(edge)].into()),
        ]),
        suppressed: false,
        sketch: None,
        frozen: None,
    }
}

#[test]
fn a_frozen_gears_faces_keep_its_root_through_the_join_and_a_fillet() {
    let doc = frozen_slice();
    let gear = fid(GEAR);
    let core = Registry::with_core_types();

    // Without the plugin: the gear frozen, its faces on the joined body.
    let mut e = Evaluator::new(core.clone());
    let evaluation = e.evaluate(&doc);
    assert!(matches!(
        evaluation.outcome(gear),
        Some(FeatureOutcome::Ok {
            frozen: Some(_),
            ..
        })
    ));
    let joined = meshes(&mut e);
    let (faces, edges) = rooted_at(&joined, gear);
    assert!(faces.len() > 40, "{} gear faces", faces.len());

    // A fillet on an edge of the plate's bottom, downstream of the join.
    let edge = joined
        .iter()
        .flat_map(|m| &m.edges)
        .map(|r| &r.name)
        .find(|n| {
            n.kind == TopoKind::Edge
                && matches!(
                    n.root,
                    arrix_core::NameRoot::Sweep {
                        part: SweepPartName::StartEdge(_),
                        feature,
                        ..
                    } if feature == fid(PLATE)
                )
        })
        .expect("an edge of the plate's bottom")
        .clone();
    let (part, record) = doc.parts().iter().next().unwrap();
    let command = Command::AddFeature {
        part: *part,
        at: record.features.len(),
        record: fillet(edge, "1 mm"),
    };
    let filleted = apply(&doc, &command).unwrap().document;
    let mut e = Evaluator::new(core);
    let evaluation = e.evaluate(&filleted);
    let outcome = evaluation.outcome(fid("7FILLETPR0BE0"));
    assert!(
        matches!(outcome, Some(FeatureOutcome::Ok { .. })),
        "{outcome:?}"
    );
    let after = meshes(&mut e);
    let (kept, kept_edges) = rooted_at(&after, gear);
    // The gear's faces and edges are named as they were: the badge finds
    // them on the filleted body.
    assert_eq!(kept, faces);
    assert_eq!(kept_edges, edges);
}
