//! `Kernel::read_step_structure` and `import_step_in` (plans/c2-m3 step 7,
//! ADR-0032) on `tests/fixtures/step/arm.step`, the probe's file: the tree
//! as plain data, a solid read in its product's frame with the names of
//! every instance the same, a record that replays.

mod arm;

use std::path::PathBuf;

use arrix_core::{FeatureId, Id, NameRoot, TopoKind};
use arrix_kernel::{ImportFrame, Kernel, KernelOp, StepOccurrence};

const FEATURE: FeatureId = FeatureId(Id(7));

fn path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/step/arm.step")
}

fn arm_bytes() -> Vec<u8> {
    std::fs::read(path()).unwrap()
}

/// The fixture is Arris's own STEP of the probe's recipe, as its README
/// says; `UPDATE_SNAPSHOTS=1` writes it again.
#[test]
fn the_arm_fixture_is_its_recipe_s_step() {
    let text = arm::arm_step();
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(path(), &text).unwrap();
        return;
    }
    let golden = std::fs::read_to_string(path()).unwrap();
    assert!(
        text == golden,
        "{}: Arris's writer changed it; rerun with UPDATE_SNAPSHOTS=1 and read the diff",
        path().display()
    );
}

#[test]
fn the_structure_is_the_tree_in_metres() {
    let mut k = Kernel::new();
    let s = k.read_step_structure(&arm_bytes()).unwrap();
    assert_eq!(s.solids.len(), 4, "bracket, servo, two screws");
    assert!(s.solids.iter().all(|s| s.refused.is_none()));
    let [arm] = &s.roots[..] else {
        panic!("one root: {:?}", s.roots)
    };
    let names = |o: &StepOccurrence| {
        o.children
            .iter()
            .map(|c| c.name.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(arm.name, "arm");
    assert_eq!(names(arm), ["bracket", "mount"]);
    let mount = &arm.children[1];
    assert_eq!(names(mount), ["servo", "screw", "screw"]);
    assert_eq!(mount.children[1].product, mount.children[2].product);
    assert!(mount.children[1].product.is_some());
    assert_eq!(mount.children[1].colour, Some(arm::STEEL.0));
    // The bracket is placed (5, -3, 0) mm from the arm.
    let at = arm.children[0].placement.as_ref().unwrap().origin();
    for (got, want) in at.to_array().iter().zip([0.005, -0.003, 0.0]) {
        assert!((got - want).abs() < 1e-15, "{at:?}");
    }
    // Every solid is held by exactly one leaf, numbered from 1.
    let mut held: Vec<u32> = [&arm.children[0], &mount.children[0], &mount.children[1]]
        .iter()
        .chain([&&mount.children[2]])
        .flat_map(|o| o.solids.clone())
        .collect();
    held.sort();
    assert_eq!(held, [1, 2, 3, 4]);
    // The read left nothing in the model, and is a recorded call.
    assert!(matches!(
        k.records().last().unwrap().op,
        KernelOp::ReadStepStructure { .. }
    ));
}

#[test]
fn a_solid_in_its_product_frame_is_the_part_as_built_and_named_alike() {
    let mut k = Kernel::new();
    let s = k.read_step_structure(&arm_bytes()).unwrap();
    let mount = &s.roots[0].children[1];
    let [a, b] = [&mount.children[1], &mount.children[2]].map(|o| o.solids[0]);
    let bytes = arm_bytes();
    // A screw is a 3 x 10 mm cylinder: its volume is pi r^2 h, whichever
    // instance, and the world read of the same solid is somewhere else.
    let volume = std::f64::consts::PI * 1.5e-3_f64.powi(2) * 10e-3;
    let mut seen = Vec::new();
    for solid in [a, b] {
        let body = k
            .import_step_in(FEATURE, &bytes, solid, ImportFrame::Product)
            .unwrap();
        let p = k.mass_properties(body).unwrap();
        assert!((p.volume - volume).abs() / volume < 1e-12, "{}", p.volume);
        // Standing on its own origin: z from 0 to 10 mm, centred on x, y.
        let c = p.centroid;
        assert!(c.x.abs() < 1e-15 && c.y.abs() < 1e-15, "{c:?}");
        assert!((c.z - 5e-3).abs() < 1e-15, "{c:?}");
        let names = k.names(body).unwrap();
        assert_eq!(names.count(TopoKind::Face), 3);
        seen.push(names.iter().cloned().collect::<Vec<_>>());
    }
    // The two instances are two solids of one file entity; the names carry
    // the instance, and nothing else tells them apart.
    let root = |n: &arrix_core::PersistentName| match &n.root {
        NameRoot::Imported {
            entity, instance, ..
        } => (*entity, *instance),
        other => panic!("{other:?}"),
    };
    let ids =
        |names: &[arrix_core::PersistentName]| names.iter().map(|n| root(n).0).collect::<Vec<_>>();
    assert_eq!(ids(&seen[0]), ids(&seen[1]));
    assert_ne!(root(&seen[0][0]).1, root(&seen[1][0]).1);
    let world = k
        .import_step_in(FEATURE, &bytes, a, ImportFrame::World)
        .unwrap();
    let c = k.mass_properties(world).unwrap().centroid;
    assert!(
        c.z.abs() > 5e-3 || c.x.abs() > 1e-3,
        "baked in the world: {c:?}"
    );
}

#[test]
fn a_call_in_the_product_frame_records_it() {
    let mut k = Kernel::new();
    k.import_step_in(FEATURE, &arm_bytes(), 1, ImportFrame::Product)
        .unwrap();
    let KernelOp::ImportStep { frame, .. } = &k.records()[0].op else {
        panic!("{:?}", k.records()[0].op)
    };
    assert_eq!(*frame, ImportFrame::Product);
    let json = serde_json::to_string(&k.records()[0].op).unwrap();
    assert!(json.contains(r#""frame":"product""#), "{json}");
}

#[test]
fn text_that_is_no_step_file_has_no_structure() {
    let mut k = Kernel::new();
    let err = k.read_step_structure(b"not a step file").unwrap_err();
    assert_eq!(err.code().as_str(), "kernel.import.parse");
}
