//! The features probe, in SI (plans/c1-m2-features step 2): `common`,
//! several booleans in one feature, a counterbored hole's tool, and
//! `project`, through the choke point, before any feature type leans on
//! them.
//!
//! What it found, and what the steps after it lean on:
//!
//! - `common` names as `fuse` and `cut` do: the target's faces inside the
//!   tool keep their names, the tool's faces inside the target keep
//!   theirs, the new edges are `gen` of the faces that meet there.
//! - **Two booleans of one feature on one target clash.** The second cut
//!   of two channels across a plate splits piece `0` of the top face
//!   again, and its piece `1` is `mod.<F>.1` a second time: the kernel
//!   refuses the result as `kernel.naming`. The same two cuts under two
//!   features name cleanly. Tools fused first and cut once also name
//!   cleanly, disjoint or crossing: the fuse touches only the tools, and a
//!   cut's tool faces are `gen`, never `mod`. A call number in the step is
//!   an ADR for the human (the plan's open questions).
//! - A counterbored hole's revolved tool, cut from the plate at another
//!   point, gives the same names: the point is not in any of them.
//! - `project` gives a plate's edges as lines and a hole's rims as circles
//!   on a parallel plane; on a tilted one the lines foreshorten and the
//!   rims are ellipses, refused as `kernel.unsupported.projection`. An
//!   edge seen end-on (a plate's rise, a revolve's seam) is
//!   `kernel.degenerate.projection-collapses`. An arc comes out
//!   anticlockwise in the plane's own `(u, v)`, so the plane seen from
//!   below swaps its ends.

use std::collections::BTreeSet;
use std::f64::consts::{PI, TAU};

use arrix_core::{
    CurveKey, DVec2, DVec3, FeatureId, Frame, Id, NameRoot, PersistentName, Profile, ProfileLoop,
    ProfileSegment, SweepPartName, TopoKind,
};
use arrix_kernel::{Kernel, KernelBody, KernelError, KernelOp, Projected};

const MM: f64 = 1e-3;
const PLATE: FeatureId = FeatureId(Id(1));
const TOOLS: FeatureId = FeatureId(Id(2));
const F: FeatureId = FeatureId(Id(9));

fn key(n: u64) -> CurveKey {
    CurveKey(Id(n))
}

fn p(x: f64, y: f64) -> DVec2 {
    DVec2::new(x * MM, y * MM)
}

/// The plane `z` mm up, facing up.
fn plane_at(z: f64) -> Frame {
    Frame::new(DVec3::new(0.0, 0.0, z * MM), DVec3::X, DVec3::Z).unwrap()
}

/// A closed polygon through `points` in mm, its sides keyed from `first`.
fn polygon(points: &[(f64, f64)], first: u64) -> ProfileLoop {
    let segments = points[1..]
        .iter()
        .chain([&points[0]])
        .zip(first..)
        .map(|(&(x, y), k)| ProfileSegment::Line {
            key: key(k),
            to: p(x, y),
        })
        .collect();
    ProfileLoop::Path {
        start: p(points[0].0, points[0].1),
        segments,
    }
}

/// A box over the rectangle `corners` (low, high) in mm, from `z` mm up
/// to `z + h`, as sweep `index` of `TOOLS`, its sides keyed from `first`.
fn block(
    k: &mut Kernel,
    index: u32,
    ((x0, y0), (x1, y1)): ((f64, f64), (f64, f64)),
    (z, h): (f64, f64),
    first: u64,
) -> KernelBody {
    let outer = polygon(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)], first);
    let profile = Profile::new(plane_at(z), outer, vec![]).unwrap();
    k.extrude(TOOLS, index, &profile, h * MM).unwrap()
}

/// A 40 × 30 × 5 mm plate, sides keyed 1–4 anticlockwise from the origin.
fn plate(k: &mut Kernel) -> KernelBody {
    let outer = polygon(&[(0.0, 0.0), (40.0, 0.0), (40.0, 30.0), (0.0, 30.0)], 1);
    let profile = Profile::new(Frame::WORLD_XY, outer, vec![]).unwrap();
    k.extrude(PLATE, 0, &profile, 5.0 * MM).unwrap()
}

/// The two booleans' tools: 2 mm deep from the plate's top.
const DEEP: (f64, f64) = (3.0, 3.0);

/// A 4 mm channel across the plate's top, from `x` mm.
fn channel(k: &mut Kernel, index: u32, x: f64, first: u64) -> KernelBody {
    block(k, index, ((x, -1.0), (x + 4.0, 31.0)), DEEP, first)
}

fn sweep(kind: TopoKind, feature: FeatureId, part: SweepPartName) -> PersistentName {
    PersistentName {
        kind,
        root: NameRoot::Sweep {
            feature,
            index: 0,
            part,
        },
        chain: vec![],
    }
}

fn names_of(k: &Kernel, body: KernelBody, kind: TopoKind) -> BTreeSet<PersistentName> {
    k.names(body).unwrap().of_kind(kind).cloned().collect()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * b.abs()
}

fn close2(a: DVec2, b: DVec2) -> bool {
    a.distance(b) <= 1e-12
}

#[test]
fn a_common_keeps_each_operand_s_faces_inside_the_other() {
    let mut k = Kernel::new();
    let plate = plate(&mut k);
    let tool = block(&mut k, 0, ((30.0, 20.0), (50.0, 40.0)), (-1.0, 7.0), 11);
    let common = k.common(F, 0, plate, tool).unwrap();
    assert!(close(
        k.mass_properties(common).unwrap().volume,
        10.0 * 10.0 * 5.0 * MM.powi(3)
    ));
    let side = |feature, s| sweep(TopoKind::Face, feature, SweepPartName::Side(key(s)));
    let want: BTreeSet<_> = [
        sweep(TopoKind::Face, PLATE, SweepPartName::StartCap),
        sweep(TopoKind::Face, PLATE, SweepPartName::EndCap),
        side(PLATE, 2),
        side(PLATE, 3),
        side(TOOLS, 11),
        side(TOOLS, 14),
    ]
    .into();
    assert_eq!(names_of(&k, common, TopoKind::Face), want);
    let names = k.names(common).unwrap();
    assert_eq!(
        [TopoKind::Edge, TopoKind::Vertex].map(|t| names.count(t)),
        [12, 8]
    );
    // The record names the operation; the operands are its two bodies.
    let record = &k.records()[2];
    assert!(matches!(
        record.op,
        KernelOp::Common {
            feature: F,
            boolean: 0
        }
    ));
    assert_eq!(record.operands.len(), 2);
}

/// The finding that stopped step 2, and ADR-0014's answer: a second cut of
/// the same feature splits piece `0` of what the first split, and would name
/// its piece `1` as the first did (`kernel.naming` without a boolean number).
/// With the boolean's number in the step the names are clean, and a
/// boolean 0 is written as before.
#[test]
fn two_booleans_of_one_feature_on_one_target_name_by_boolean_number() {
    let mut k = Kernel::new();
    let plate = plate(&mut k);
    let (a, b) = (channel(&mut k, 0, 10.0, 11), channel(&mut k, 1, 26.0, 21));
    let once = k.cut(F, 0, plate, a).unwrap();
    let twice = k.cut(F, 1, once, b).unwrap();
    // The first cut's piece 1 of the top face is `mod.F.1`, the second's
    // `mod.F.1.1`: one text each.
    let top = |boolean| {
        let mut piece = sweep(TopoKind::Face, PLATE, SweepPartName::EndCap);
        piece.chain.push(arrix_core::NameStep::Modified {
            feature: F,
            boolean,
            split: 1,
        });
        piece
    };
    assert!(k.names(once).unwrap().contains(&top(0)));
    assert!(k.names(twice).unwrap().contains(&top(1)));
    assert!(top(0).to_string().ends_with("/mod.0000000000009.1"));
    assert!(top(1).to_string().ends_with("/mod.0000000000009.1.1"));
    assert!(matches!(
        k.records()[k.records().len() - 1].op,
        KernelOp::Cut { boolean: 1, .. }
    ));
}

#[test]
fn tools_fused_first_and_cut_once_name_cleanly() {
    // A second channel apart from the first, then a slot crossing it.
    for second in [((26.0, -1.0), (30.0, 31.0)), ((-1.0, 13.0), (41.0, 17.0))] {
        let mut k = Kernel::new();
        let plate = plate(&mut k);
        let a = channel(&mut k, 0, 10.0, 11);
        let b = block(&mut k, 1, second, DEEP, 21);
        let tools = k.fuse(F, 0, a, b).unwrap();
        k.cut(F, 1, plate, tools).unwrap();
    }
}

/// A counterbored hole's tool at `(x, y)` mm, as revolve `index` of
/// `TOOLS`: 6 mm across from 1 mm below the plate, 10 mm across from
/// 2 mm below its top to 1 mm above.
fn counterbore(k: &mut Kernel, index: u32, (x, y): (f64, f64)) -> KernelBody {
    let origin = DVec3::new(x * MM, y * MM, 0.0);
    let plane = Frame::new(origin, DVec3::X, -DVec3::Y).unwrap();
    let outer = polygon(
        &[
            (0.0, -1.0),
            (3.0, -1.0),
            (3.0, 3.0),
            (5.0, 3.0),
            (5.0, 6.0),
            (0.0, 6.0),
        ],
        41,
    );
    let profile = Profile::new(plane, outer, vec![]).unwrap();
    k.revolve(TOOLS, index, &profile, (origin, DVec3::Z), TAU)
        .unwrap()
}

fn drilled(k: &mut Kernel, at: (f64, f64)) -> KernelBody {
    let plate = plate(k);
    let tool = counterbore(k, 0, at);
    k.cut(F, 0, plate, tool).unwrap()
}

#[test]
fn a_counterbored_hole_moved_keeps_every_name() {
    let mut names = Vec::new();
    for at in [(20.0, 15.0), (12.0, 10.0)] {
        let mut k = Kernel::new();
        let body = drilled(&mut k, at);
        let volume = 40.0 * 30.0 * 5.0 - PI * (9.0 * 3.0 + 25.0 * 2.0);
        assert!(close(
            k.mass_properties(body).unwrap().volume,
            volume * MM.powi(3)
        ));
        names.push(k.names(body).unwrap().clone());
    }
    assert_eq!(names[0], names[1]);
    // The plate's six faces and the hole's three: its wall, the
    // counterbore's floor and the counterbore's wall.
    assert_eq!(names[0].count(TopoKind::Face), 9);
}

fn project_one(
    k: &mut Kernel,
    body: KernelBody,
    name: &PersistentName,
    plane: &Frame,
) -> Result<Projected, KernelError> {
    let [one] = k.project(body, std::slice::from_ref(name), plane)?[..] else {
        panic!("one projection per name")
    };
    Ok(one)
}

/// The hole's two rims: where its wall meets the bottom and where the
/// counterbore's wall meets the top.
fn rims(k: &Kernel, body: KernelBody) -> [PersistentName; 2] {
    let on_cap = |cap| {
        let mut found = names_of(k, body, TopoKind::Edge).into_iter().filter(
            |n| matches!(&n.root, NameRoot::Sweep { feature: PLATE, part, .. } if *part == cap),
        );
        let rim = found.next().unwrap();
        assert!(found.next().is_none());
        rim
    };
    [
        on_cap(SweepPartName::StartCap),
        on_cap(SweepPartName::EndCap),
    ]
}

#[test]
fn a_plate_s_edges_and_a_hole_s_rims_project_onto_its_top() {
    let mut k = Kernel::new();
    let body = drilled(&mut k, (20.0, 15.0));
    let top = plane_at(5.0);
    let corners = [(0.0, 0.0), (40.0, 0.0), (40.0, 30.0), (0.0, 30.0)];
    let mut names: Vec<_> = (1..=4)
        .map(|s| sweep(TopoKind::Edge, PLATE, SweepPartName::EndEdge(key(s))))
        .collect();
    names.extend(rims(&k, body));
    names.push(sweep(
        TopoKind::Vertex,
        PLATE,
        SweepPartName::StartVertex(key(3)),
    ));
    let got = k.project(body, &names, &top).unwrap();
    for (s, projected) in got[..4].iter().enumerate() {
        let (a, b) = (corners[s], corners[(s + 1) % 4]);
        let Projected::Line { start, end } = *projected else {
            panic!("{projected:?}")
        };
        assert!(
            close2(start, p(a.0, a.1)) && close2(end, p(b.0, b.1)),
            "{projected:?}"
        );
    }
    for (projected, radius) in got[4..6].iter().zip([3.0, 5.0]) {
        let Projected::Circle { center, radius: r } = *projected else {
            panic!("{projected:?}")
        };
        assert!(close2(center, p(20.0, 15.0)) && close(r, radius * MM));
    }
    // The bottom corner under (40, 30), seen on the top plane.
    let Projected::Point(corner) = got[6] else {
        panic!("{:?}", got[6])
    };
    assert!(close2(corner, p(40.0, 30.0)));
    assert!(matches!(
        &k.records().last().unwrap().op,
        KernelOp::Project { names: n, plane } if *n == names && *plane == top
    ));
}

#[test]
fn a_tilted_plane_foreshortens_lines_and_refuses_ellipses() {
    let mut k = Kernel::new();
    let body = drilled(&mut k, (20.0, 15.0));
    // Turned 30° about the X axis through the plate's top.
    let (s, c) = (PI / 6.0).sin_cos();
    let origin = DVec3::new(0.0, 0.0, 5.0 * MM);
    let tilted = Frame::new(origin, DVec3::X, DVec3::new(0.0, -s, c)).unwrap();
    let back = sweep(TopoKind::Edge, PLATE, SweepPartName::EndEdge(key(3)));
    let got = project_one(&mut k, body, &back, &tilted).unwrap();
    let Projected::Line { start, end } = got else {
        panic!("{got:?}")
    };
    assert!(close2(start, p(40.0, 30.0 * c)) && close2(end, p(0.0, 30.0 * c)));
    for rim in rims(&k, body) {
        let err = project_one(&mut k, body, &rim, &tilted).unwrap_err();
        assert_eq!(err.code().as_str(), "kernel.unsupported.projection");
        assert!(err.to_string().contains("an ellipse"), "{err}");
        let call = err.call().expect("a recorded failure");
        assert!(matches!(
            k.record(call).unwrap().op,
            KernelOp::Project { .. }
        ));
    }
}

#[test]
fn an_edge_seen_end_on_collapses() {
    let mut k = Kernel::new();
    let body = drilled(&mut k, (20.0, 15.0));
    let rise = sweep(TopoKind::Edge, PLATE, SweepPartName::Rise(key(1)));
    let seam = names_of(&k, body, TopoKind::Edge)
        .into_iter()
        .find(|n| {
            matches!(
                n.root,
                NameRoot::Revolve {
                    part: SweepPartName::StartEdge(_),
                    ..
                }
            )
        })
        .unwrap();
    for name in [rise, seam] {
        let err = project_one(&mut k, body, &name, &plane_at(5.0)).unwrap_err();
        assert_eq!(
            err.code().as_str(),
            "kernel.degenerate.projection-collapses",
            "{name}"
        );
        assert!(err.call().is_some());
    }
}

#[test]
fn a_projection_takes_only_the_body_s_edges_and_vertices() {
    let mut k = Kernel::new();
    let body = plate(&mut k);
    let calls = k.records().len();
    let face = sweep(TopoKind::Face, PLATE, SweepPartName::EndCap);
    let err = project_one(&mut k, body, &face, &plane_at(5.0)).unwrap_err();
    assert_eq!(err.code().as_str(), "kernel.wrong-kind");
    let elsewhere = sweep(TopoKind::Edge, TOOLS, SweepPartName::EndEdge(key(1)));
    let err = project_one(&mut k, body, &elsewhere, &plane_at(5.0)).unwrap_err();
    assert_eq!(err.code().as_str(), "kernel.unknown-name");
    assert_eq!(k.records().len(), calls, "no call was made");
}

#[test]
fn an_arc_projects_anticlockwise_in_the_plane_s_own_coordinates() {
    let mut k = Kernel::new();
    // A plate whose side 2 is a half-circle about (40, 15) mm.
    let outer = ProfileLoop::Path {
        start: p(0.0, 0.0),
        segments: vec![
            ProfileSegment::Line {
                key: key(1),
                to: p(40.0, 0.0),
            },
            ProfileSegment::Arc {
                key: key(2),
                to: p(40.0, 30.0),
                via: p(55.0, 15.0),
            },
            ProfileSegment::Line {
                key: key(3),
                to: p(0.0, 30.0),
            },
            ProfileSegment::Line {
                key: key(4),
                to: p(0.0, 0.0),
            },
        ],
    };
    let profile = Profile::new(Frame::WORLD_XY, outer, vec![]).unwrap();
    let body = k.extrude(PLATE, 0, &profile, 5.0 * MM).unwrap();
    let below = Frame::new(DVec3::new(0.0, 0.0, 5.0 * MM), DVec3::X, -DVec3::Z).unwrap();
    // Seen from below, v is −y: the arc runs from (40, −30) to (40, 0).
    for (plane, center, start, end) in [
        (plane_at(5.0), (40.0, 15.0), (40.0, 0.0), (40.0, 30.0)),
        (below, (40.0, -15.0), (40.0, -30.0), (40.0, 0.0)),
    ] {
        for part in [
            SweepPartName::StartEdge(key(2)),
            SweepPartName::EndEdge(key(2)),
        ] {
            let name = sweep(TopoKind::Edge, PLATE, part);
            let got = project_one(&mut k, body, &name, &plane).unwrap();
            let Projected::Arc {
                center: c,
                start: a,
                end: b,
            } = got
            else {
                panic!("{got:?}")
            };
            assert!(
                close2(c, p(center.0, center.1))
                    && close2(a, p(start.0, start.1))
                    && close2(b, p(end.0, end.1)),
                "{name}: {got:?}"
            );
        }
    }
}
