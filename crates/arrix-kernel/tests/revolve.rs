//! `Kernel::revolve` (plans/c1-m2-naming step 6), in SI: what a stepped
//! shaft's half-section, a partial turn and a hollow ring name, each entity
//! exactly once (a void's shell is not an entity with a name), rooted at the key of the curve that swept it.

use std::collections::BTreeSet;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

use arrix_core::{
    CurveKey, DVec2, DVec3, FeatureId, Frame, Id, NameRoot, PersistentName, Profile, ProfileLoop,
    ProfileSegment, SweepPartName, TopoKind,
};
use arrix_kernel::{Kernel, KernelBody, KernelOp};

const MM: f64 = 1e-3;
const SHAFT: FeatureId = FeatureId(Id(7));

fn key(n: u64) -> CurveKey {
    CurveKey(Id(n))
}

/// The XZ plane: profile x along the world X, profile y along the world Z.
fn xz() -> Frame {
    Frame::new(DVec3::ZERO, DVec3::X, -DVec3::Y).unwrap()
}

/// A closed polygon through `points` in mm, its sides keyed from `first`.
fn polygon(points: &[(f64, f64)], first: u64) -> ProfileLoop {
    let p = |(x, y): (f64, f64)| DVec2::new(x * MM, y * MM);
    let segments = points[1..]
        .iter()
        .chain([&points[0]])
        .zip(first..)
        .map(|(&to, k)| ProfileSegment::Line {
            key: key(k),
            to: p(to),
        })
        .collect();
    ProfileLoop::Path {
        start: p(points[0]),
        segments,
    }
}

/// Radius 10 up to z = 5, then 6 up to z = 15, closed along the axis;
/// sides keyed 1–6.
fn stepped() -> Profile {
    let outer = polygon(
        &[
            (0.0, 0.0),
            (10.0, 0.0),
            (10.0, 5.0),
            (6.0, 5.0),
            (6.0, 15.0),
            (0.0, 15.0),
        ],
        1,
    );
    Profile::new(xz(), outer, vec![]).unwrap()
}

const AXIS: (DVec3, DVec3) = (DVec3::ZERO, DVec3::Z);

fn counts(k: &Kernel, body: KernelBody) -> [usize; 3] {
    let names = k.names(body).unwrap();
    [TopoKind::Face, TopoKind::Edge, TopoKind::Vertex].map(|t| names.count(t))
}

fn revolve(k: &mut Kernel, profile: &Profile, angle: f64) -> KernelBody {
    k.revolve(SHAFT, 0, profile, AXIS, angle).unwrap()
}

/// Every name is rooted at this feature's revolve and no entity has two.
fn assert_revolve_roots(k: &Kernel, body: KernelBody, index: u32) {
    let names: Vec<&PersistentName> = k.names(body).unwrap().iter().collect();
    let unique: BTreeSet<_> = names.iter().collect();
    assert_eq!(unique.len(), names.len(), "{names:#?}");
    for name in names {
        assert!(
            matches!(
                name.root,
                NameRoot::Revolve { feature, index: i, .. } if feature == SHAFT && i == index
            ),
            "{name}"
        );
        assert!(name.chain.is_empty(), "{name}");
    }
}

fn side(k: u64) -> SweepPartName {
    SweepPartName::Side(key(k))
}

fn has(k: &Kernel, body: KernelBody, kind: TopoKind, part: SweepPartName) -> bool {
    k.names(body).unwrap().iter().any(|n| {
        n.kind == kind && matches!(&n.root, NameRoot::Revolve { part: p, .. } if *p == part)
    })
}

#[test]
fn a_full_turn_has_no_end_cap_and_its_seams_are_start_edges() {
    let mut k = Kernel::new();
    let body = revolve(&mut k, &stepped(), TAU);
    // Two discs, two walls and the annulus at the step; no caps.
    assert_eq!(counts(&k, body), [5, 6, 4]);
    assert_revolve_roots(&k, body, 0);
    let v = k.mass_properties(body).unwrap().volume;
    let want = PI * (100.0 * 5.0 + 36.0 * 10.0) * MM.powi(3);
    assert!((v - want).abs() < 1e-9 * want, "{v} vs {want}");
    for s in 1..=5 {
        assert!(has(&k, body, TopoKind::Face, side(s)), "side {s}");
    }
    // The side on the axis made nothing.
    assert!(!has(&k, body, TopoKind::Face, side(6)));
    assert!(!has(&k, body, TopoKind::Face, SweepPartName::StartCap));
    assert!(!has(&k, body, TopoKind::Face, SweepPartName::EndCap));
    // The seam of the wall at the step's outer rim is a start edge.
    assert!(has(
        &k,
        body,
        TopoKind::Edge,
        SweepPartName::StartEdge(key(2))
    ));
    assert!(!has(
        &k,
        body,
        TopoKind::Edge,
        SweepPartName::EndEdge(key(2))
    ));
}

#[test]
fn a_partial_turn_has_both_caps_and_every_end() {
    let mut k = Kernel::new();
    let body = revolve(&mut k, &stepped(), FRAC_PI_2);
    assert_eq!(counts(&k, body), [7, 15, 10]);
    assert_revolve_roots(&k, body, 0);
    let v = k.mass_properties(body).unwrap().volume;
    let want = PI * (100.0 * 5.0 + 36.0 * 10.0) * MM.powi(3) / 4.0;
    assert!((v - want).abs() < 1e-9 * want, "{v} vs {want}");
    for part in [SweepPartName::StartCap, SweepPartName::EndCap] {
        assert!(has(&k, body, TopoKind::Face, part));
    }
    for s in 1..=6 {
        assert!(has(
            &k,
            body,
            TopoKind::Edge,
            SweepPartName::StartEdge(key(s))
        ));
    }
    for s in 1..=5 {
        assert!(has(
            &k,
            body,
            TopoKind::Edge,
            SweepPartName::EndEdge(key(s))
        ));
    }
}

#[test]
fn a_hollow_ring_names_the_walls_of_its_void_by_their_own_loop() {
    let mut k = Kernel::new();
    let ring = polygon(&[(2.0, 0.0), (10.0, 0.0), (10.0, 10.0), (2.0, 10.0)], 1);
    let hole = polygon(&[(4.0, 3.0), (8.0, 3.0), (8.0, 7.0), (4.0, 7.0)], 11);
    let profile = Profile::new(xz(), ring, vec![hole]).unwrap();
    let body = revolve(&mut k, &profile, TAU);
    assert_eq!(counts(&k, body), [8, 12, 8]);
    assert_revolve_roots(&k, body, 0);
    let v = k.mass_properties(body).unwrap().volume;
    let want = PI * ((100.0 - 4.0) * 10.0 - (64.0 - 16.0) * 4.0) * MM.powi(3);
    assert!((v - want).abs() < 1e-9 * want, "{v} vs {want}");
    for s in (1..=4).chain(11..=14) {
        assert!(has(&k, body, TopoKind::Face, side(s)), "side {s}");
    }
}

#[test]
fn a_second_revolve_of_a_feature_is_named_apart() {
    let mut k = Kernel::new();
    let first = revolve(&mut k, &stepped(), TAU);
    let second = k.revolve(SHAFT, 1, &stepped(), AXIS, TAU).unwrap();
    assert_revolve_roots(&k, second, 1);
    let name = k.names(second).unwrap().iter().next().unwrap().to_string();
    assert!(name.contains(".1."), "{name}");
    let a: BTreeSet<_> = k.names(first).unwrap().iter().collect();
    assert!(k.names(second).unwrap().iter().all(|n| !a.contains(n)));
}

#[test]
fn a_refused_revolve_keeps_its_record_and_a_category() {
    let mut k = Kernel::new();
    let err = k
        .revolve(SHAFT, 0, &stepped(), (DVec3::ZERO, DVec3::ZERO), TAU)
        .unwrap_err();
    assert_eq!(err.code().as_str(), "kernel.invalid-input");
    // A profile that crosses its axis.
    let across = Profile::new(
        xz(),
        polygon(&[(-2.0, 0.0), (2.0, 0.0), (2.0, 4.0), (-2.0, 4.0)], 1),
        vec![],
    )
    .unwrap();
    let err = k.revolve(SHAFT, 0, &across, AXIS, TAU).unwrap_err();
    assert_eq!(
        err.code().as_str(),
        "kernel.degenerate.profile-crosses-axis"
    );
    let err = k
        .revolve(SHAFT, 0, &stepped(), AXIS, TAU + 1.0)
        .unwrap_err();
    assert_eq!(err.code().as_str(), "kernel.degenerate.angle-above-turn");
    assert_eq!(k.records().len(), 3);
    assert!(
        k.records()
            .iter()
            .all(|c| matches!(c.op, KernelOp::Revolve { .. }))
    );
}
