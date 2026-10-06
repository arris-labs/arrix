//! `Kernel::vertex_point` and `Kernel::edge_axis` (plans/c1-m2-features
//! step 5): where a vertex is and the line a straight edge lies on, which
//! the evaluator turns into `point` and `axis` inputs. Both are recorded,
//! and a name that is not what was asked for is refused by name.

use arrix_core::{
    CurveKey, DVec2, DVec3, FeatureId, Frame, Id, NameRoot, PersistentName, Profile, ProfileLoop,
    ProfileSegment, SweepPartName, TopoKind,
};
use arrix_kernel::{Kernel, KernelBody, KernelOp};

const MM: f64 = 1e-3;
const PLATE: FeatureId = FeatureId(Id(1));

fn key(n: u64) -> CurveKey {
    CurveKey(Id(n))
}

fn name(kind: TopoKind, part: SweepPartName) -> PersistentName {
    PersistentName {
        kind,
        root: NameRoot::Sweep {
            feature: PLATE,
            index: 0,
            part,
        },
        chain: vec![],
    }
}

/// A 40 × 30 × 5 mm plate, sides keyed 1–4 from the origin anticlockwise,
/// and a 6 mm circular boss base on it.
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
    let hole = ProfileLoop::Circle {
        key: key(9),
        center: p(20.0, 15.0),
        radius: 3.0 * MM,
    };
    let profile = Profile::new(Frame::WORLD_XY, outer, vec![hole]).unwrap();
    k.extrude(PLATE, 0, &profile, 5.0 * MM).unwrap()
}

fn close(a: DVec3, b: DVec3) -> bool {
    a.distance(b) < 1e-9
}

#[test]
fn a_vertex_is_where_the_plate_has_it() {
    let mut k = Kernel::new();
    let plate = plate(&mut k);
    // Side 1 starts at the origin: its start vertex is the corner there,
    // and the top corner above it is the end vertex.
    let corner = |part| name(TopoKind::Vertex, part);
    let at = k
        .vertex_point(plate, &corner(SweepPartName::StartVertex(key(1))))
        .unwrap();
    assert!(close(at, DVec3::ZERO), "{at}");
    let top = k
        .vertex_point(plate, &corner(SweepPartName::EndVertex(key(1))))
        .unwrap();
    assert!(close(top, DVec3::new(0.0, 0.0, 5.0 * MM)), "{top}");
    let record = k.records().last().unwrap();
    assert!(matches!(&record.op, KernelOp::VertexPoint { .. }));
    assert_eq!(record.operands.len(), 1);
}

#[test]
fn a_straight_edge_gives_its_line_and_a_circle_is_refused() {
    let mut k = Kernel::new();
    let plate = plate(&mut k);
    let bottom_of_side_1 = name(TopoKind::Edge, SweepPartName::StartEdge(key(1)));
    let axis = k.edge_axis(plate, &bottom_of_side_1).unwrap();
    // Side 1 runs along x on the bottom face, either way round.
    assert!(
        close(axis.direction.abs(), DVec3::X),
        "{:?}",
        axis.direction
    );
    assert!(axis.origin.z.abs() < 1e-12 && axis.origin.y.abs() < 1e-12);
    assert!((axis.direction.length() - 1.0).abs() < 1e-12);

    let rim = name(TopoKind::Edge, SweepPartName::EndEdge(key(9)));
    let err = k.edge_axis(plate, &rim).unwrap_err();
    assert_eq!(err.code().as_str(), "kernel.invalid-input");
    assert!(err.call().is_some(), "the refusal is a recorded call");
}

#[test]
fn a_name_that_is_not_what_was_asked_is_refused_by_name() {
    let mut k = Kernel::new();
    let plate = plate(&mut k);
    let face = name(TopoKind::Face, SweepPartName::EndCap);
    let edge = name(TopoKind::Edge, SweepPartName::StartEdge(key(1)));
    let gone = name(TopoKind::Vertex, SweepPartName::StartVertex(key(77)));
    let calls = k.records().len();
    assert_eq!(
        k.vertex_point(plate, &edge).unwrap_err().code().as_str(),
        "kernel.wrong-kind"
    );
    assert_eq!(
        k.edge_axis(plate, &face).unwrap_err().code().as_str(),
        "kernel.wrong-kind"
    );
    assert_eq!(
        k.vertex_point(plate, &gone).unwrap_err().code().as_str(),
        "kernel.unknown-name"
    );
    assert_eq!(k.records().len(), calls, "a name check is not a call");
}
