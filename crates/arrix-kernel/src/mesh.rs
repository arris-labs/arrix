//! Render meshes from Arris's tessellation (docs/UI-RENDERING.md §The
//! viewport): the watertight buffer with its per-face corner block
//! (Arris ADR-0012), cast to `f32` about the body's centre, every face,
//! edge and vertex range carrying its persistent name.

use std::collections::BTreeMap;

use arris::mesh::{MeshRequest, TriMesh};
use arris::topo::{Body, EntityId, Model, NotFound};
use arrix_core::{DVec3, MeshDetail, MeshPoint, MeshRange, PersistentName, RenderMesh};

use crate::{BodyNames, KernelBody, KernelError, KernelOp, convert};

impl crate::Kernel {
    /// `body` as a render mesh at `detail`: the chord is the detail's
    /// fraction of the diagonal of the box of the body's edges and
    /// vertices, which is the body's box when no face bulges past its
    /// edges (every extrude), and a little short of it otherwise (a
    /// revolved torus): a coarser resolution, never a wrong mesh. Recorded with the chord it
    /// came to. An entity the body's names miss is `kernel.naming`.
    pub fn tessellate(
        &mut self,
        body: KernelBody,
        detail: MeshDetail,
    ) -> Result<RenderMesh, KernelError> {
        let target = self.entry(body)?.body;
        let diagonal = edge_box(&self.model, target)
            .map(|(lo, hi)| (hi - lo).length())
            .unwrap_or(0.0);
        let chord = detail.chord_fraction() * diagonal;
        let call = self.begin(KernelOp::Tessellate { detail, chord }, &[body])?;
        let request = MeshRequest::new(chord).with_corners();
        let mesh = self
            .controlled(|m, control| arris::mesh::tessellate_with(m, target, &request, control))
            .map_err(|e| KernelError::mesh(call, &e))?;
        render_mesh(&self.model, target, &self.entry(body)?.names, &mesh, chord)
            .map_err(|message| KernelError::Naming { call, message })
    }
}

/// The box of `body`'s edges' curves and its vertices, or `None` for a
/// body with neither.
fn edge_box(m: &Model, body: Body) -> Option<(DVec3, DVec3)> {
    let mut points = Vec::new();
    for edge in m.edges(body).ok()? {
        let curve = m.edge(edge.id).ok()?.curve();
        let bounds = curve.and_then(|(c, range)| m.curve(c).ok()?.bounds(range));
        if let Some(b) = bounds {
            points.push(DVec3::from_array(b.min));
            points.push(DVec3::from_array(b.max));
        }
    }
    for vertex in m.vertices(body).ok()? {
        points.push(convert::dpoint3(m.vertex(vertex.id).ok()?.point()));
    }
    let first = *points.first()?;
    Some(
        points
            .into_iter()
            .fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p))),
    )
}

fn render_mesh(
    m: &Model,
    body: Body,
    names: &BodyNames,
    mesh: &TriMesh,
    chord: f64,
) -> Result<RenderMesh, String> {
    let by_entity: BTreeMap<EntityId, &PersistentName> =
        names.pairs().map(|(name, id)| (id, name)).collect();
    let name = |id: EntityId| {
        by_entity
            .get(&id)
            .map(|&n| n.clone())
            .ok_or_else(|| format!("{id} has no name"))
    };
    let corners = mesh
        .corners()
        .ok_or("a mesh asked with corners came without")?;
    let origin = mesh
        .aabb()
        .map_or(DVec3::ZERO, |b| DVec3::from_array(b.center()));
    let shared = mesh.positions();
    let local = |i: u32| {
        let p = DVec3::from_array(shared[i as usize]) - origin;
        p.as_vec3().to_array()
    };
    let faces = mesh
        .faces()
        .iter()
        .map(|f| range(name(EntityId::Face(f.face))?, &f.triangles))
        .collect::<Result<_, String>>()?;
    let edges = mesh
        .edges()
        .iter()
        .map(|e| range(name(EntityId::Edge(e.edge))?, &e.indices))
        .collect::<Result<_, String>>()?;
    let mut vertices = Vec::new();
    for v in m.vertices(body).map_err(|e: NotFound| e.to_string())? {
        let point = m.vertex(v.id).map_err(|e| e.to_string())?.point();
        vertices.push(MeshPoint {
            name: name(EntityId::Vertex(v.id))?,
            position: (convert::dpoint3(point) - origin).as_vec3().to_array(),
        });
    }
    Ok(RenderMesh {
        origin,
        chord,
        positions: corners.positions().iter().map(|&i| local(i)).collect(),
        normals: corners
            .normals()
            .iter()
            .map(|n| DVec3::from_array(*n).as_vec3().to_array())
            .collect(),
        triangles: corners.triangles().to_vec(),
        faces,
        edge_points: mesh.edge_indices().iter().map(|&i| local(i)).collect(),
        edges,
        vertices,
    })
}

fn range(name: PersistentName, r: &std::ops::Range<usize>) -> Result<MeshRange, String> {
    let index = |i: usize| u32::try_from(i).map_err(|_| "a mesh past 2³² entries".to_owned());
    Ok(MeshRange {
        name,
        start: index(r.start)?,
        end: index(r.end)?,
    })
}
