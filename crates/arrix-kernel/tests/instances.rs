//! `Kernel::copy` and `Kernel::mirror` (plans/c1-m2-naming step 8): every
//! entity of a copy is its original's name with `inst.<feature>.<i>`, and a
//! pattern's holes keep their names when the count changes (ADR-0010).

use std::collections::BTreeSet;
use std::f64::consts::FRAC_PI_2;

use arrix_core::{
    CurveKey, DVec2, DVec3, FeatureId, Frame, Id, NameStep, PersistentName, Profile, ProfileLoop,
    ProfileSegment, TopoKind,
};
use arrix_kernel::{Kernel, KernelBody, KernelOp, Motion};

const MM: f64 = 1e-3;
const PLATE: FeatureId = FeatureId(Id(1));
const PIN: FeatureId = FeatureId(Id(2));
const PATTERN: FeatureId = FeatureId(Id(3));
const CUT: FeatureId = FeatureId(Id(4));
const FLIP: FeatureId = FeatureId(Id(5));

fn key(n: u64) -> CurveKey {
    CurveKey(Id(n))
}

/// A 60 × 30 × 5 mm plate, sides keyed 1–4.
fn plate(k: &mut Kernel) -> KernelBody {
    let p = |x: f64, y: f64| DVec2::new(x * MM, y * MM);
    let line = |n, to| ProfileSegment::Line { key: key(n), to };
    let outer = ProfileLoop::Path {
        start: p(0.0, 0.0),
        segments: vec![
            line(1, p(60.0, 0.0)),
            line(2, p(60.0, 30.0)),
            line(3, p(0.0, 30.0)),
            line(4, p(0.0, 0.0)),
        ],
    };
    let profile = Profile::new(Frame::WORLD_XY, outer, vec![]).unwrap();
    k.extrude(PLATE, 0, &profile, 5.0 * MM).unwrap()
}

/// A 2 mm pin at (5, 15) mm reaching from 1 mm below the plate to 1 mm above.
fn pin(k: &mut Kernel) -> KernelBody {
    let circle = ProfileLoop::Circle {
        key: key(11),
        center: DVec2::new(5.0 * MM, 15.0 * MM),
        radius: MM,
    };
    let plane = Frame::new(DVec3::new(0.0, 0.0, -MM), DVec3::X, DVec3::Z).unwrap();
    let profile = Profile::new(plane, circle, vec![]).unwrap();
    k.extrude(PIN, 0, &profile, 7.0 * MM).unwrap()
}

fn counts(k: &Kernel, body: KernelBody) -> [usize; 3] {
    let names = k.names(body).unwrap();
    [TopoKind::Face, TopoKind::Edge, TopoKind::Vertex].map(|t| names.count(t))
}

fn all(k: &Kernel, body: KernelBody) -> BTreeSet<PersistentName> {
    k.names(body).unwrap().iter().cloned().collect()
}

fn inst(feature: FeatureId, index: u32) -> NameStep {
    NameStep::Instance { feature, index }
}

fn step(by: f64) -> Motion {
    Motion::translation(DVec3::new(by * MM, 0.0, 0.0))
}

/// `plate` with `n` holes 8 mm apart, the pin's copies fused, then cut.
/// Returns the cut and the names of each hole's wall, by copy index.
fn pattern(k: &mut Kernel, n: u32) -> (KernelBody, Vec<PersistentName>) {
    let plate = plate(k);
    let seed = pin(k);
    let mut tool = k.copy(PATTERN, 1, seed, step(8.0)).unwrap();
    for i in 2..=n {
        let next = k.copy(PATTERN, i, seed, step(8.0 * f64::from(i))).unwrap();
        tool = k.fuse(PATTERN, i - 2, tool, next).unwrap();
    }
    let cut = k.cut(CUT, 0, plate, tool).unwrap();
    // A hole's wall is `gen.CUT[` the copy's wall `]`: the one face whose
    // origin carries instance `i`.
    let walls = (1..=n)
        .map(|i| {
            let mut found = all(k, cut).into_iter().filter(|name| {
                name.kind == TopoKind::Face
                    && matches!(
                        name.chain.last(),
                        Some(NameStep::Generated { from, .. })
                            if from.iter().any(|f| f.chain.contains(&inst(PATTERN, i)))
                    )
            });
            let wall = found.next().unwrap_or_else(|| panic!("n = {n}: hole {i}"));
            assert!(found.next().is_none(), "n = {n}: hole {i} has two walls");
            wall
        })
        .collect();
    (cut, walls)
}

#[test]
fn a_copy_names_every_entity_as_its_original_with_an_instance_step() {
    let mut k = Kernel::new();
    let plate = plate(&mut k);
    let copy = k.copy(PATTERN, 2, plate, step(70.0)).unwrap();
    assert_eq!(counts(&k, copy), counts(&k, plate));
    let want: BTreeSet<_> = all(&k, plate)
        .into_iter()
        .map(|mut name| {
            name.chain.push(inst(PATTERN, 2));
            name
        })
        .collect();
    assert_eq!(all(&k, copy), want);
    assert_eq!(
        k.records().last().unwrap().op,
        KernelOp::Copy {
            feature: PATTERN,
            index: 2,
            motion: step(70.0),
        }
    );
    // The copy has the original's volume, moved.
    let (a, b) = (
        k.mass_properties(plate).unwrap(),
        k.mass_properties(copy).unwrap(),
    );
    assert!((a.volume - b.volume).abs() < 1e-12 * a.volume);
    assert!((b.centroid.x - a.centroid.x - 70.0 * MM).abs() < 1e-9);
}

#[test]
fn a_turned_copy_is_the_same_body_turned() {
    let mut k = Kernel::new();
    let plate = plate(&mut k);
    let motion = Motion {
        translation: DVec3::ZERO,
        axis: DVec3::Z,
        angle: FRAC_PI_2,
    };
    let copy = k.copy(PATTERN, 1, plate, motion).unwrap();
    let (a, b) = (
        k.mass_properties(plate).unwrap(),
        k.mass_properties(copy).unwrap(),
    );
    assert!((a.volume - b.volume).abs() < 1e-12 * a.volume);
    // About the origin, a quarter turn takes (x, y) to (-y, x).
    assert!((b.centroid.x + a.centroid.y).abs() < 1e-9);
    assert!((b.centroid.y - a.centroid.x).abs() < 1e-9);
    assert_eq!(counts(&k, copy), counts(&k, plate));
}

#[test]
fn a_mirror_is_instance_one_and_a_copy_of_an_instance_chains() {
    let mut k = Kernel::new();
    let plate = plate(&mut k);
    let image = k
        .mirror(FLIP, plate, DVec3::new(70.0 * MM, 0.0, 0.0), DVec3::X)
        .unwrap();
    assert_eq!(counts(&k, image), counts(&k, plate));
    assert!(all(&k, image).iter().all(|n| n.chain == [inst(FLIP, 1)]));
    let a = k.mass_properties(plate).unwrap();
    let b = k.mass_properties(image).unwrap();
    assert!((a.volume - b.volume).abs() < 1e-12 * a.volume);
    // Reflected in x = 70 mm: 30 mm becomes 110 mm.
    assert!((b.centroid.x - (140.0 * MM - a.centroid.x)).abs() < 1e-9);
    assert!((b.centroid.y - a.centroid.y).abs() < 1e-9);
    // A copy of the image keeps its instance and adds its own.
    let again = k.copy(PATTERN, 1, image, step(10.0)).unwrap();
    assert!(
        all(&k, again)
            .iter()
            .all(|n| n.chain == [inst(FLIP, 1), inst(PATTERN, 1)])
    );
}

#[test]
fn a_zero_axis_or_normal_fails_soft_with_the_record_kept() {
    let mut k = Kernel::new();
    let plate = plate(&mut k);
    let before = k.records().len();
    let bad = Motion {
        translation: DVec3::ZERO,
        axis: DVec3::ZERO,
        angle: 1.0,
    };
    let err = k.copy(PATTERN, 1, plate, bad).err().unwrap();
    assert_eq!(err.code().as_str(), "kernel.invalid-input");
    let err = k
        .mirror(FLIP, plate, DVec3::ZERO, DVec3::ZERO)
        .err()
        .unwrap();
    assert_eq!(err.code().as_str(), "kernel.invalid-input");
    assert_eq!(k.records().len(), before + 2);
}

#[test]
fn a_pattern_of_holes_keeps_each_hole_s_name_at_any_count() {
    let mut k = Kernel::new();
    let (cut, walls) = pattern(&mut k, 4);
    // Per hole: a wall, two rims, a seam and its two ends; the plate's
    // faces, edges and vertices are 6, 12 and 8 before.
    assert_eq!(counts(&k, cut), [6 + 4, 12 + 4 * 3, 8 + 4 * 2]);
    let unique: BTreeSet<_> = walls.iter().collect();
    assert_eq!(unique.len(), 4);
    for n in 1..=4 {
        let (_, fewer) = pattern(&mut Kernel::new(), n);
        // The same names for the holes that stay, whatever the count.
        assert_eq!(fewer[..], walls[..n as usize], "n = {n}");
    }
    // Hole 3's wall is not hole 4's.
    assert_ne!(walls[2], walls[3]);
}

#[test]
fn removing_a_copy_removes_its_hole_alone() {
    let mut k = Kernel::new();
    let (with_four, walls) = pattern(&mut k, 4);
    let four = all(&k, with_four);
    let mut k3 = Kernel::new();
    let (three_cut, _) = pattern(&mut k3, 3);
    let three = all(&k3, three_cut);
    // Every name of three holes is a name of four, and what four adds
    // belongs to hole 4: its wall among it.
    assert!(three.is_subset(&four));
    let added: BTreeSet<_> = four.difference(&three).cloned().collect();
    assert!(added.contains(&walls[3]));
    assert!(!three.contains(&walls[3]));
}
