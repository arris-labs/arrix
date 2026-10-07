//! `Kernel::tessellate` on the M1 slice's bodies (plans/viewport-port
//! step 3): the plate, the spur gear `gears.spur` outlines, and the two
//! joined. Here rather than in `arrix-kernel` because the gear's outline
//! is the plugin's, and only this crate may use both.

use std::collections::{BTreeMap, BTreeSet};
// `arrix-cli` is never built for wasm, so the std clock is fine here
// (docs/CONCURRENCY-WASM.md §The browser build).
#[allow(clippy::disallowed_types, reason = "native-only measurement clock")]
use std::time::Instant;

use arrix_core::{
    CurveKey, DVec2, DVec3, FeatureId, Frame, Id, MeshDetail, PersistentName, Profile, ProfileLoop,
    ProfileSegment, RenderMesh, TopoKind,
};
use arrix_gears::involute::Spur;
use arrix_kernel::{Kernel, KernelBody, KernelOp};

const MM: f64 = 1e-3;
const PLATE: FeatureId = FeatureId(Id(1));
const GEAR: FeatureId = FeatureId(Id(2));
const JOIN: FeatureId = FeatureId(Id(3));

/// The slice's plate: `w` × `h` × `t` = 40 × 30 × 5 mm from the origin.
fn plate(k: &mut Kernel) -> KernelBody {
    let p = |x: f64, y: f64| DVec2::new(x * MM, y * MM);
    let line = |n, to| ProfileSegment::Line {
        key: CurveKey(Id(n)),
        to,
    };
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

/// The slice's gear: 20 teeth of module 1 mm, 8 mm wide, standing on the
/// plate's top face over its centre.
fn gear(k: &mut Kernel) -> KernelBody {
    let spur = Spur {
        teeth: 20,
        module: MM,
        pressure_angle: 20f64.to_radians(),
    };
    let plane = Frame::new(
        DVec3::new(20.0 * MM, 15.0 * MM, 5.0 * MM),
        DVec3::X,
        DVec3::Z,
    )
    .unwrap();
    let profile = Profile::new(plane, spur.outline().unwrap(), vec![]).unwrap();
    k.extrude(GEAR, 0, &profile, 8.0 * MM).unwrap()
}

fn bodies(k: &mut Kernel) -> [(&'static str, KernelBody); 3] {
    let plate = plate(k);
    let gear = gear(k);
    let joined = k.fuse(JOIN, 0, plate, gear).unwrap();
    [("plate", plate), ("gear", gear), ("joined", joined)]
}

/// Each triangle-buffer vertex welded to the others at its position:
/// faces meet along equal positions (`RenderMesh`), so this is the
/// watertight mesh's index.
fn welded(m: &RenderMesh) -> Vec<u32> {
    let mut ids: BTreeMap<[u32; 3], u32> = BTreeMap::new();
    m.positions
        .iter()
        .map(|p| {
            let n = u32::try_from(ids.len()).unwrap();
            *ids.entry(p.map(f32::to_bits)).or_insert(n)
        })
        .collect()
}

/// Every directed edge of the welded triangles once, and its opposite once.
fn is_closed(m: &RenderMesh) -> bool {
    let id = welded(m);
    let mut count: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for t in &m.triangles {
        let [a, b, c] = t.map(|i| id[i as usize]);
        for e in [(a, b), (b, c), (c, a)] {
            if e.0 == e.1 {
                return false;
            }
            *count.entry(e).or_default() += 1;
        }
    }
    !count.is_empty()
        && count
            .iter()
            .all(|(&(a, b), &n)| n == 1 && count.get(&(b, a)) == Some(&1))
}

/// The enclosed volume, about the mesh's origin, in f64.
fn volume(m: &RenderMesh) -> f64 {
    let v = |i: u32| {
        let p = m.positions[i as usize];
        DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]))
    };
    m.triangles
        .iter()
        .map(|t| v(t[0]).dot(v(t[1]).cross(v(t[2]))))
        .sum::<f64>()
        / 6.0
}

#[test]
fn every_body_meshes_closed_with_every_range_named_once() {
    let mut k = Kernel::new();
    for (what, body) in bodies(&mut k) {
        for detail in [MeshDetail::Coarse, MeshDetail::Refined] {
            let m = k.tessellate(body, detail).unwrap();
            assert!(is_closed(&m), "{what} {detail:?}: not closed");
            assert_eq!(m.positions.len(), m.normals.len());
            for n in &m.normals {
                let length = n.iter().map(|c| c * c).sum::<f32>().sqrt();
                assert!((length - 1.0).abs() < 1e-6, "{what}: a normal of {length}");
            }
            // The face ranges cover the triangles in order, the edge
            // ranges the edge points.
            let mut next = 0;
            for f in &m.faces {
                assert_eq!(f.start, next, "{what}: {} out of order", f.name);
                assert!(f.end > f.start, "{what}: {} has no triangle", f.name);
                next = f.end;
            }
            assert_eq!(next as usize, m.triangles.len());
            let mut next = 0;
            for e in &m.edges {
                assert_eq!(e.start, next);
                assert!(e.end >= e.start + 2, "{what}: {} is no polyline", e.name);
                next = e.end;
            }
            assert_eq!(next as usize, m.edge_points.len());
            // Each of the body's names once, in its kind's list.
            let names = k.names(body).unwrap();
            let mesh_names = |kind| -> Vec<&PersistentName> {
                match kind {
                    TopoKind::Face => m.faces.iter().map(|r| &r.name).collect(),
                    TopoKind::Edge => m.edges.iter().map(|r| &r.name).collect(),
                    TopoKind::Vertex => m.vertices.iter().map(|v| &v.name).collect(),
                }
            };
            for kind in [TopoKind::Face, TopoKind::Edge, TopoKind::Vertex] {
                let listed = mesh_names(kind);
                let set: BTreeSet<_> = listed.iter().copied().collect();
                assert_eq!(set.len(), listed.len(), "{what}: a {kind:?} twice");
                assert_eq!(
                    set,
                    names.of_kind(kind).collect(),
                    "{what}: the {kind:?}s differ"
                );
            }
        }
    }
}

#[test]
fn two_runs_give_byte_identical_meshes() {
    let run = || {
        let mut k = Kernel::new();
        bodies(&mut k).map(|(_, body)| {
            [MeshDetail::Coarse, MeshDetail::Refined]
                .map(|d| serde_json::to_string(&k.tessellate(body, d).unwrap()).unwrap())
        })
    };
    assert_eq!(run(), run());
}

#[test]
fn a_mesh_s_volume_is_within_its_chord_of_the_body_s() {
    let mut k = Kernel::new();
    for (what, body) in bodies(&mut k) {
        let exact = k.mass_properties(body).unwrap();
        for detail in [MeshDetail::Coarse, MeshDetail::Refined] {
            let m = k.tessellate(body, detail).unwrap();
            // Every point of the surface within a chord of the mesh: the
            // volume between the two is at most the area times the chord.
            let bound = exact.area * m.chord;
            let error = (volume(&m) - exact.volume).abs();
            assert!(
                error <= bound,
                "{what} {detail:?}: {error:e} m³ off, past {bound:e}"
            );
        }
    }
}

#[test]
fn a_mesh_is_recorded_at_the_chord_its_detail_came_to() {
    let mut k = Kernel::new();
    let [_, (_, gear), _] = bodies(&mut k);
    let coarse = k.tessellate(gear, MeshDetail::Coarse).unwrap();
    let refined = k.tessellate(gear, MeshDetail::Refined).unwrap();
    let records = k.records();
    let [.., c, r] = records else { unreachable!() };
    assert_eq!(
        c.op,
        KernelOp::Tessellate {
            detail: MeshDetail::Coarse,
            chord: coarse.chord
        }
    );
    assert_eq!(
        r.op,
        KernelOp::Tessellate {
            detail: MeshDetail::Refined,
            chord: refined.chord
        }
    );
    assert_eq!(
        c.sources,
        [arrix_kernel::CallIndex(1)],
        "the gear's extrude"
    );
    // The gear's box is its tip circle by its width: 22 × 22 × 8 mm.
    let diagonal = (22f64.powi(2) * 2.0 + 8f64.powi(2)).sqrt() * MM;
    assert!((coarse.chord - 1e-3 * diagonal).abs() < 1e-12);
    assert!((refined.chord - 1e-4 * diagonal).abs() < 1e-12);
    assert!(refined.triangles.len() >= coarse.triangles.len());
    // The mesh's box is inside it by at most a chord: a tip arc's
    // extreme need not be a sample.
    let (lo, hi) = coarse.bounds().unwrap();
    let short = DVec3::new(22.0, 22.0, 8.0) * MM - (hi - lo);
    assert!(
        short.min_element() >= -1e-9 && short.max_element() <= coarse.chord,
        "{short:?}"
    );
}

/// The counts and times the chords were chosen from (`MeshDetail`),
/// printed: `-- --nocapture` shows them. Debug builds of Arris check the
/// body before meshing it, so a release build is faster still.
#[test]
fn the_slice_s_meshes_are_measured() {
    let mut k = Kernel::new();
    for (what, body) in bodies(&mut k) {
        for detail in [MeshDetail::Coarse, MeshDetail::Refined] {
            let start = Instant::now();
            let m = k.tessellate(body, detail).unwrap();
            let took = start.elapsed();
            println!(
                "{what} {detail:?}: {} triangles, {} edge points, chord {:.2e} m, {took:?}",
                m.triangles.len(),
                m.edge_points.len(),
                m.chord,
            );
            assert!(took.as_millis() < 150, "{what} {detail:?}: {took:?}");
        }
    }
}
