//! Render meshes: a body's triangles, edge polylines and vertex points for
//! drawing and picking, as plain data (docs/UI-RENDERING.md §The viewport).
//! `arrix-kernel` builds them from Arris's tessellation; the viewport
//! uploads them as they stand and names no kernel type.

use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::{DVec3, PersistentName};

/// How fine a mesh is asked for: a coarse one first so a body shows at
/// once, then a refined one (docs/CONCURRENCY-WASM.md §Tessellation).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum MeshDetail {
    Coarse,
    Refined,
}

impl MeshDetail {
    /// The chord, how far a triangle may stray from its surface, as a
    /// fraction of the diagonal of the body's box, so a gear and a bridge
    /// get the same number of triangles across them: a circle the size of
    /// its body is about 42 segments coarse and 130 refined. Measured on
    /// the M1 slice, whose gear's flanks are short arcs already, so its
    /// count barely moves with the chord (about 1,800 triangles either
    /// way, the refined mesh in under 10 ms).
    pub const fn chord_fraction(self) -> f64 {
        match self {
            MeshDetail::Coarse => 1e-3,
            MeshDetail::Refined => 1e-4,
        }
    }
}

/// A run of a mesh's triangles (a face), edge points (an edge) and the
/// entity it draws, by name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct MeshRange {
    pub name: PersistentName,
    pub start: u32,
    pub end: u32,
}

impl MeshRange {
    pub fn range(&self) -> Range<usize> {
        self.start as usize..self.end as usize
    }
}

/// A body's vertex, by name, at its place relative to the mesh's origin.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct MeshPoint {
    pub name: PersistentName,
    pub position: [f32; 3],
}

/// One body's mesh. Positions are `f32` metres relative to `origin`, the
/// centre of the body's box, so the cast from the kernel's `f64` loses
/// nothing a screen shows wherever the body stands. A vertex of the
/// triangle buffer belongs to one face: the faces meet along equal
/// positions, never shared vertices, so each carries its own face's
/// outward normal and a sharp edge stays sharp.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RenderMesh {
    /// In the world, metres.
    #[cfg_attr(feature = "schema", schemars(with = "[f64; 3]"))]
    pub origin: DVec3,
    /// The chord it was made at, metres.
    pub chord: f64,
    pub positions: Vec<[f32; 3]>,
    /// Unit and outward, parallel to `positions`.
    pub normals: Vec<[f32; 3]>,
    /// Indices into `positions`, anticlockwise seen from outside.
    pub triangles: Vec<[u32; 3]>,
    /// Each face's run of `triangles`; together they cover them, in order.
    pub faces: Vec<MeshRange>,
    /// The edges' polylines, one run each, end to end along the edge.
    pub edge_points: Vec<[f32; 3]>,
    /// Each edge's run of `edge_points`.
    pub edges: Vec<MeshRange>,
    pub vertices: Vec<MeshPoint>,
}

impl RenderMesh {
    /// The box of the triangles' positions, in the world, or `None` for a
    /// mesh with none.
    pub fn bounds(&self) -> Option<(DVec3, DVec3)> {
        let mut points = self.positions.iter().map(|&p| self.world(p));
        let first = points.next()?;
        Some(points.fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p))))
    }

    /// A position of this mesh in the world.
    pub fn world(&self, p: [f32; 3]) -> DVec3 {
        self.origin + DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mesh() -> RenderMesh {
        RenderMesh {
            origin: DVec3::new(1.0, 2.0, 3.0),
            chord: 1e-4,
            positions: vec![[-0.5, 0.0, 0.25], [0.5, 1.0, -0.25]],
            normals: vec![[0.0, 0.0, 1.0]; 2],
            triangles: vec![],
            faces: vec![],
            edge_points: vec![],
            edges: vec![],
            vertices: vec![],
        }
    }

    #[test]
    fn bounds_are_in_the_world() {
        let (lo, hi) = mesh().bounds().unwrap();
        assert_eq!(lo, DVec3::new(0.5, 2.0, 2.75));
        assert_eq!(hi, DVec3::new(1.5, 3.0, 3.25));
        let empty = RenderMesh {
            positions: vec![],
            ..mesh()
        };
        assert_eq!(empty.bounds(), None);
    }

    #[test]
    fn a_mesh_crosses_as_json() {
        let m = mesh();
        let text = serde_json::to_string(&m).unwrap();
        assert_eq!(serde_json::from_str::<RenderMesh>(&text).unwrap(), m);
        assert_eq!(
            serde_json::to_string(&MeshDetail::Refined).unwrap(),
            "\"refined\""
        );
    }
}
