//! The edge-geometry call (plans/c3-m3 step 5, ADR-0038 §3): a circular
//! rim and a straight edge of an extruded plate against hand-computed
//! values, the side read from the faces' outward normals, and the call
//! recorded.

use arrix_core::{
    CurveKey, DVec2, DVec3, FeatureId, Frame, Id, NameRoot, PersistentName, Profile, ProfileLoop,
    ProfileSegment, SweepPartName, TopoKind,
};
use arrix_kernel::{EdgeCurve, Kernel, KernelBody, KernelOp};

const MM: f64 = 1e-3;
const SWEEP: FeatureId = FeatureId(Id(8));

fn key(n: u64) -> CurveKey {
    CurveKey(Id(n))
}

fn sweep(kind: TopoKind, part: SweepPartName) -> PersistentName {
    PersistentName {
        kind,
        root: NameRoot::Sweep {
            feature: SWEEP,
            index: 0,
            part,
        },
        chain: vec![],
    }
}

fn near(a: DVec3, b: DVec3) -> bool {
    (a - b).length() < 1e-9
}

/// A 40 × 30 × 10 mm plate with a bore of radius 4 at (20, 15), keys 1–4
/// its sides, 5 the bore.
fn plate(k: &mut Kernel) -> KernelBody {
    let p = |x: f64, y: f64| DVec2::new(x * MM, y * MM);
    let line = |n, to| ProfileSegment::Line { key: key(n), to };
    let outer = ProfileLoop::Path {
        start: p(0.0, 0.0),
        segments: vec![
            line(1, p(40.0, 0.0)),
            line(2, p(40.0, 30.0)),
            line(3, p(0.0, 30.0)),
            line(4, p(0.0, 0.0)),
        ],
    };
    let bore = ProfileLoop::Circle {
        key: key(5),
        center: p(20.0, 15.0),
        radius: 4.0 * MM,
    };
    let profile = Profile::new(Frame::WORLD_XY, outer, vec![bore]).unwrap();
    k.extrude(SWEEP, 0, &profile, 10.0 * MM).unwrap()
}

#[test]
fn a_rim_is_a_circle_on_the_end_cap_and_the_bore() {
    let mut k = Kernel::new();
    let body = plate(&mut k);
    let rim = sweep(TopoKind::Edge, SweepPartName::EndEdge(key(5)));
    let g = k.edge_geometry(body, &rim).unwrap();
    let EdgeCurve::Circle {
        centre,
        axis,
        radius,
    } = g.curve
    else {
        panic!("{:?}", g.curve)
    };
    assert!(
        near(centre, DVec3::new(20.0, 15.0, 10.0) * MM),
        "{centre:?}"
    );
    assert!(near(axis.abs(), DVec3::Z), "{axis:?}");
    assert!((radius - 4.0 * MM).abs() < 1e-12, "{radius}");
    // The rim bounds the end cap (planar, facing +z) and the bore (a
    // cylinder, which has no normal listed).
    assert_eq!(g.face_normals.len(), 1);
    assert!(near(g.face_normals[0], DVec3::Z), "{:?}", g.face_normals);
}

#[test]
fn a_side_edge_is_a_line_between_its_ends_bounding_two_planes() {
    let mut k = Kernel::new();
    let body = plate(&mut k);
    let edge = sweep(TopoKind::Edge, SweepPartName::EndEdge(key(1)));
    let g = k.edge_geometry(body, &edge).unwrap();
    let EdgeCurve::Line { start, end } = g.curve else {
        panic!("{:?}", g.curve)
    };
    let ends = [start, end];
    let (a, b) = (
        DVec3::new(0.0, 0.0, 10.0) * MM,
        DVec3::new(40.0, 0.0, 10.0) * MM,
    );
    assert!(
        (near(ends[0], a) && near(ends[1], b)) || (near(ends[0], b) && near(ends[1], a)),
        "{ends:?}"
    );
    // The top edge of the front side: the end cap above, the side face
    // facing -y; the names' order makes the list stable.
    assert_eq!(g.face_normals.len(), 2);
    assert!(g.face_normals.iter().any(|n| near(*n, DVec3::Z)));
    assert!(g.face_normals.iter().any(|n| near(*n, -DVec3::Y)));
    assert_eq!(k.edge_geometry(body, &edge).unwrap(), g, "deterministic");
}

#[test]
fn the_call_is_recorded_and_a_wrong_name_is_refused() {
    let mut k = Kernel::new();
    let body = plate(&mut k);
    let rim = sweep(TopoKind::Edge, SweepPartName::EndEdge(key(5)));
    k.edge_geometry(body, &rim).unwrap();
    let last = k.records().last().unwrap();
    assert!(matches!(&last.op, KernelOp::EdgeGeometry { edge } if edge == &rim));

    let cap = sweep(TopoKind::Face, SweepPartName::EndCap);
    assert!(k.edge_geometry(body, &cap).is_err(), "a face is no edge");
    let gone = sweep(TopoKind::Edge, SweepPartName::EndEdge(key(99)));
    assert!(k.edge_geometry(body, &gone).is_err());
}
