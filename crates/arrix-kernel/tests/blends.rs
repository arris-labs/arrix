//! `Kernel::fillet` and `Kernel::chamfer` (plans/c1-m2-naming step 7): the
//! blend's entities are named from the edges, every trimmed neighbour keeps
//! its name (ADR-0010), and every entity has exactly one.

use std::collections::BTreeSet;

use arrix_core::{
    CurveKey, DVec2, FeatureId, Frame, Id, NameRoot, NameStep, PersistentName, Profile,
    ProfileLoop, ProfileSegment, SweepPartName, TopoKind,
};
use arrix_kernel::{Kernel, KernelBody, KernelOp};

const MM: f64 = 1e-3;
const PLATE: FeatureId = FeatureId(Id(1));
const BLEND: FeatureId = FeatureId(Id(2));

fn key(n: u64) -> CurveKey {
    CurveKey(Id(n))
}

/// A 40 × 30 × 5 mm plate, sides keyed 1–4 from the origin anticlockwise.
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
    let profile = Profile::new(Frame::WORLD_XY, outer, vec![]).unwrap();
    k.extrude(PLATE, 0, &profile, 5.0 * MM).unwrap()
}

fn root(kind: TopoKind, part: SweepPartName) -> PersistentName {
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

/// The plate's top edge along side `s`.
fn top(s: u64) -> PersistentName {
    root(TopoKind::Edge, SweepPartName::EndEdge(key(s)))
}

fn rise(v: u64) -> PersistentName {
    root(TopoKind::Edge, SweepPartName::Rise(key(v)))
}

fn counts(k: &Kernel, body: KernelBody) -> [usize; 3] {
    let names = k.names(body).unwrap();
    [TopoKind::Face, TopoKind::Edge, TopoKind::Vertex].map(|t| names.count(t))
}

fn all(k: &Kernel, body: KernelBody) -> BTreeSet<PersistentName> {
    k.names(body).unwrap().iter().cloned().collect()
}

/// The faces, edges and vertices of `body` by kind, as name sets.
fn of_kind(k: &Kernel, body: KernelBody, kind: TopoKind) -> BTreeSet<PersistentName> {
    k.names(body)
        .unwrap()
        .iter()
        .filter(|n| n.kind == kind)
        .cloned()
        .collect()
}

fn blend(k: &mut Kernel, chamfer: bool, body: KernelBody, edges: &[PersistentName]) -> KernelBody {
    if chamfer {
        k.chamfer(BLEND, 0, body, edges, 2.0 * MM).unwrap()
    } else {
        k.fillet(BLEND, 0, body, edges, 2.0 * MM).unwrap()
    }
}

/// The step of a blend-made name: `gen.<BLEND>[…]` last in its chain.
fn made_by_blend(n: &PersistentName) -> bool {
    matches!(n.chain.last(), Some(NameStep::Generated { feature, .. }) if *feature == BLEND)
}

#[test]
fn a_blend_keeps_every_name_it_only_trims_and_names_what_it_made() {
    let corner = vec![top(1), top(2), rise(2)];
    // (edges, counts, edges and vertices that went)
    for (what, edges, counts_want, gone) in [
        ("one", vec![top(1)], [7, 15, 10], (1, 2)),
        ("miter", vec![top(1), top(2)], [8, 17, 11], (2, 3)),
        ("corner", corner, [10, 22, 13], (3, 4)),
    ] {
        for chamfer in [false, true] {
            let mut k = Kernel::new();
            let body = plate(&mut k);
            let before = all(&k, body);
            let blended = blend(&mut k, chamfer, body, &edges);
            let mut want = counts_want;
            if chamfer && what == "corner" {
                want[1] -= 1; // a chamfer's corner has one trim edge fewer
            }
            assert_eq!(counts(&k, blended), want, "{what} chamfer={chamfer}");
            let after = all(&k, blended);
            // Only the blended edges and their corner vertices went; every
            // other entity, trimmed or not, keeps its name (ADR-0010).
            let went: Vec<_> = before.difference(&after).collect();
            let edges_gone = went.iter().filter(|n| n.kind == TopoKind::Edge).count();
            let vertices_gone = went.iter().filter(|n| n.kind == TopoKind::Vertex).count();
            assert_eq!((edges_gone, vertices_gone), gone, "{what}: {went:#?}");
            assert!(went.iter().all(|n| n.kind != TopoKind::Face));
            let new: Vec<_> = after.difference(&before).collect();
            assert!(new.iter().all(|n| made_by_blend(n)), "{what}: {new:#?}");
            assert_eq!(new.len() + before.len() - went.len(), after.len());
            // The operand is untouched.
            assert_eq!(all(&k, body), before);
        }
    }
}

#[test]
fn a_blend_face_is_generated_from_its_edge_alone() {
    let mut k = Kernel::new();
    let body = plate(&mut k);
    let blended = k
        .fillet(BLEND, 0, body, &[top(1), top(2)], 2.0 * MM)
        .unwrap();
    let faces = of_kind(&k, blended, TopoKind::Face);
    for s in [1, 2] {
        let edge = top(s);
        let face = PersistentName {
            kind: TopoKind::Face,
            chain: vec![NameStep::Generated {
                boolean: 0,
                feature: BLEND,
                from: vec![edge.clone()],
            }],
            ..edge
        };
        assert!(faces.contains(&face), "{face} in {faces:#?}");
    }
}

#[test]
fn a_reference_to_the_top_face_survives_a_blend_added_above_it() {
    let mut k = Kernel::new();
    let body = plate(&mut k);
    let top_face = root(TopoKind::Face, SweepPartName::EndCap);
    let z = |k: &mut Kernel, b| k.face_frame(b, &top_face).unwrap().origin().z;
    let blended = blend(&mut k, false, body, &[top(1), top(2)]);
    assert!((z(&mut k, body) - 5.0 * MM).abs() < 1e-12);
    assert!((z(&mut k, blended) - 5.0 * MM).abs() < 1e-12);
    let chamfered = blend(&mut k, true, body, &[top(3)]);
    assert!(k.names(chamfered).unwrap().contains(&top_face));
}

#[test]
fn a_second_blend_on_a_blended_body_leaves_the_first_blend_alone() {
    let mut k = Kernel::new();
    let body = plate(&mut k);
    let once = blend(&mut k, false, body, &[top(1)]);
    let twice = blend(&mut k, true, once, &[top(3)]);
    assert_eq!(counts(&k, twice), [8, 18, 12]);
    // Only the opposite edge and its two vertices went; the first blend's
    // face, edges and vertices are named as before.
    let gone: Vec<_> = all(&k, once).difference(&all(&k, twice)).cloned().collect();
    assert_eq!(gone.len(), 3, "{gone:#?}");
    assert!(gone.contains(&top(3)));
    let first_blend: BTreeSet<_> = all(&k, once)
        .difference(&all(&k, body))
        .filter(|n| made_by_blend(n))
        .cloned()
        .collect();
    assert_eq!(first_blend.len(), 9);
    assert!(first_blend.is_subset(&all(&k, twice)));
}

#[test]
fn an_oversize_blend_fails_with_its_category_and_its_record() {
    let mut k = Kernel::new();
    let body = plate(&mut k);
    let err = k.fillet(BLEND, 0, body, &[top(1)], 30.0 * MM).unwrap_err();
    assert_eq!(err.code().as_str(), "kernel.degenerate.blend-too-large");
    let call = k.record(err.call().unwrap()).unwrap();
    assert!(matches!(&call.op, KernelOp::Fillet { edges, .. } if edges == &[top(1)]));
    assert_eq!(call.operands.len(), 1);
    // The second fillet on an edge the first shortened runs off its face.
    let once = blend(&mut k, false, body, &[top(1)]);
    let err = k.fillet(BLEND, 0, once, &[top(4)], 2.0 * MM).unwrap_err();
    assert_eq!(err.code().as_str(), "kernel.degenerate.blend-too-large");
}

#[test]
fn a_name_that_is_not_an_edge_of_the_body_is_refused_without_a_record() {
    let mut k = Kernel::new();
    let body = plate(&mut k);
    let calls = k.records().len();
    let face = root(TopoKind::Face, SweepPartName::EndCap);
    let err = k.fillet(BLEND, 0, body, &[face], 2.0 * MM).unwrap_err();
    assert_eq!(err.code().as_str(), "kernel.wrong-kind");
    let err = k.chamfer(BLEND, 0, body, &[rise(9)], 2.0 * MM).unwrap_err();
    assert_eq!(err.code().as_str(), "kernel.unknown-name");
    assert_eq!(k.records().len(), calls);
}

/// A 20 × 10 mm stadium (two lines, two half circles), sides keyed 1–4.
fn stadium(k: &mut Kernel) -> KernelBody {
    let p = |x: f64, y: f64| DVec2::new(x * MM, y * MM);
    let outer = ProfileLoop::Path {
        start: p(5.0, 0.0),
        segments: vec![
            ProfileSegment::Line {
                key: key(1),
                to: p(15.0, 0.0),
            },
            ProfileSegment::Arc {
                key: key(2),
                to: p(15.0, 10.0),
                via: p(20.0, 5.0),
            },
            ProfileSegment::Line {
                key: key(3),
                to: p(5.0, 10.0),
            },
            ProfileSegment::Arc {
                key: key(4),
                to: p(5.0, 0.0),
                via: p(0.0, 5.0),
            },
        ],
    };
    let profile = Profile::new(Frame::WORLD_XY, outer, vec![]).unwrap();
    k.extrude(PLATE, 0, &profile, 5.0 * MM).unwrap()
}

#[test]
fn naming_one_edge_of_a_tangent_chain_blends_the_chain_with_one_name_per_entity() {
    let mut k = Kernel::new();
    let body = stadium(&mut k);
    let blended = k.fillet(BLEND, 0, body, &[top(1)], 2.0 * MM).unwrap();
    // Whichever edge of the chain is named, the same names come out.
    let faces = of_kind(&k, blended, TopoKind::Face);
    let made: BTreeSet<_> = faces.iter().filter(|n| made_by_blend(n)).collect();
    assert_eq!(
        made.len(),
        4,
        "one blend face per edge of the chain: {made:#?}"
    );
    for s in 1..=4 {
        let from = top(s);
        assert!(
            made.iter().any(|n| matches!(
                n.chain.last(),
                Some(NameStep::Generated { from: f, .. }) if f == std::slice::from_ref(&from)
            )),
            "side {s}: {made:#?}"
        );
    }
    let again = k.fillet(BLEND, 0, body, &[top(3)], 2.0 * MM).unwrap();
    assert_eq!(all(&k, again), all(&k, blended));
}

/// Arris 0.5.0 gap, kept as a failing-soft fact until it is fixed there
/// (plans/c1-m2-naming, the open question on step 7): a fillet of 1 mm or
/// less on this stadium's top rim, named by one edge or all four, fails
/// inside `ops::fillet` as `Internal(Invariant)` "a junction arc of
/// positive angle was not found"; 2 mm and every chamfer succeed. It goes
/// to Arris as a fixture from step 9's export and a backlog line there.
#[test]
fn a_small_fillet_of_a_stadium_rim_fails_inside_arris_and_keeps_its_record() {
    let mut k = Kernel::new();
    let body = stadium(&mut k);
    let err = k.fillet(BLEND, 0, body, &[top(1)], 1.0 * MM).unwrap_err();
    assert_eq!(err.code().as_str(), "kernel.internal.invariant");
    assert!(matches!(
        &k.record(err.call().unwrap()).unwrap().op,
        KernelOp::Fillet { radius, .. } if *radius == 1.0 * MM
    ));
    assert!(k.chamfer(BLEND, 0, body, &[top(1)], 1.0 * MM).is_ok());
}
