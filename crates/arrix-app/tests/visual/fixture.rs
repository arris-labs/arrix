//! A hand-made [`RenderMesh`]: a box, for drawing the viewport before a
//! document can be opened, and for pinning the renderer apart from the
//! kernel.

use arrix_core::{
    CurveKey, DVec3, FeatureId, Id, MeshPoint, MeshRange, NameRoot, PartId, PersistentName,
    RenderMesh, SlotName, SweepPartName, TopoKind,
};
use arrix_viewport::BodyKey;

/// The key the fixture's body is shown under.
pub fn box_key() -> BodyKey {
    BodyKey {
        part: PartId(Id(1)),
        feature: FeatureId(Id(1)),
        slot: SlotName::new("body").expect("a valid slot name"),
    }
}

fn name(kind: TopoKind, i: usize) -> PersistentName {
    PersistentName {
        kind,
        root: NameRoot::Sweep {
            feature: FeatureId(Id(1)),
            index: 0,
            part: SweepPartName::Side(CurveKey(Id(i as u64 + 1))),
        },
        chain: vec![],
    }
}

/// A box of `size` metres centred on the world origin: six faces of two
/// triangles each with their own corners and outward normal, twelve
/// straight edges and eight vertices, every range named.
pub fn box_mesh(size: [f32; 3]) -> RenderMesh {
    let h = size.map(|s| s * 0.5);
    let corner = |i: usize| {
        [
            if i & 1 == 0 { -h[0] } else { h[0] },
            if i & 2 == 0 { -h[1] } else { h[1] },
            if i & 4 == 0 { -h[2] } else { h[2] },
        ]
    };
    // Each face's corners, anticlockwise seen from outside, and its normal.
    let faces: [([usize; 4], [f32; 3]); 6] = [
        ([0, 4, 6, 2], [-1.0, 0.0, 0.0]),
        ([1, 3, 7, 5], [1.0, 0.0, 0.0]),
        ([0, 1, 5, 4], [0.0, -1.0, 0.0]),
        ([2, 6, 7, 3], [0.0, 1.0, 0.0]),
        ([0, 2, 3, 1], [0.0, 0.0, -1.0]),
        ([4, 5, 7, 6], [0.0, 0.0, 1.0]),
    ];
    let mut mesh = RenderMesh {
        origin: DVec3::ZERO,
        chord: 1e-3,
        positions: vec![],
        normals: vec![],
        triangles: vec![],
        faces: vec![],
        edge_points: vec![],
        edges: vec![],
        vertices: vec![],
    };
    for (f, (corners, normal)) in faces.iter().enumerate() {
        let base = mesh.positions.len() as u32;
        mesh.positions.extend(corners.map(corner));
        mesh.normals.extend([*normal; 4]);
        let start = mesh.triangles.len() as u32;
        mesh.triangles
            .extend([[base, base + 1, base + 2], [base, base + 2, base + 3]]);
        mesh.faces.push(MeshRange {
            name: name(TopoKind::Face, f),
            start,
            end: start + 2,
        });
    }
    // Corners that differ in one coordinate bit.
    let edges = (0..8usize)
        .flat_map(|a| [1, 2, 4].map(|bit| (a, a | bit)))
        .filter(|&(a, b)| a != b);
    for (e, (a, b)) in edges.enumerate() {
        let start = mesh.edge_points.len() as u32;
        mesh.edge_points.extend([corner(a), corner(b)]);
        mesh.edges.push(MeshRange {
            name: name(TopoKind::Edge, e),
            start,
            end: start + 2,
        });
    }
    mesh.vertices = (0..8)
        .map(|i| MeshPoint {
            name: name(TopoKind::Vertex, i),
            position: corner(i),
        })
        .collect();
    mesh
}

#[test]
fn the_box_is_whole() {
    let mesh = box_mesh([1.0, 2.0, 3.0]);
    assert_eq!(
        (mesh.faces.len(), mesh.edges.len(), mesh.vertices.len()),
        (6, 12, 8)
    );
    assert_eq!(mesh.triangles.len(), 12);
    assert_eq!(
        mesh.bounds(),
        Some((DVec3::new(-0.5, -1.0, -1.5), DVec3::new(0.5, 1.0, 1.5)))
    );
    // Every triangle winds anticlockwise about its face's normal.
    for t in &mesh.triangles {
        let v = |p: [f32; 3]| DVec3::from(p.map(f64::from));
        let [a, b, c] = t.map(|i| v(mesh.positions[i as usize]));
        let n = v(mesh.normals[t[0] as usize]);
        assert!((b - a).cross(c - a).dot(n) > 0.0, "{t:?}");
    }
}
