//! The scene: what the viewport draws, one entry per body slot, each with
//! its own uploaded mesh (docs/UI-RENDERING.md §The viewport).
//!
//! A mesh event touches exactly its body's entry, so a document of N bodies
//! costs one upload per event, and a mesh that did not change since the last
//! upload costs none: the evaluator sends every final body's mesh again on
//! each generation, and an edit that left a body alone must not re-upload it.

use std::hash::{DefaultHasher, Hash, Hasher};

use arrix_core::{DVec3, FeatureId, PartId, PersistentName, RenderMesh, SlotName, TopoKind};
use glam::{Mat4, Vec3};

use crate::pick::{MAX_PICK_BODIES, PickHit};

/// A body in the scene: the slot of a part that its mesh is the final
/// version of, named by the feature that made it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BodyKey {
    pub part: PartId,
    pub feature: FeatureId,
    pub slot: SlotName,
}

/// How a [`RenderMesh`] becomes whatever a [`Scene`] holds per body.
///
/// The real implementation is the GPU buffers (`Context = wgpu::Device`);
/// the indirection lets the incremental bookkeeping, the part with the
/// invariants worth testing, run without a GPU.
pub trait BodyPayload: Sized {
    type Context: ?Sized;

    /// Uploads `mesh` as `key`'s payload.
    fn upload(ctx: &Self::Context, key: &BodyKey, mesh: &RenderMesh) -> Self;
}

/// An axis-aligned box in `f32`, kept beside a payload so "frame the
/// bodies" does not need the CPU-side positions back.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    /// The box of every position a mesh draws with: surface vertices, edge
    /// polylines and vertices alike, relative to the mesh's origin. `None`
    /// for a mesh with no geometry at all.
    pub fn of_mesh(mesh: &RenderMesh) -> Option<Aabb> {
        let points = mesh
            .positions
            .iter()
            .chain(&mesh.edge_points)
            .copied()
            .chain(mesh.vertices.iter().map(|v| v.position))
            .map(Vec3::from_array);
        points.fold(None, |b: Option<Aabb>, p| {
            Some(match b {
                None => Aabb { min: p, max: p },
                Some(b) => Aabb {
                    min: b.min.min(p),
                    max: b.max.max(p),
                },
            })
        })
    }

    fn union(self, other: Aabb) -> Aabb {
        Aabb {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    pub fn center(self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    /// The radius of the sphere about the box's centre that holds it.
    pub fn radius(self) -> f32 {
        (self.max - self.min).length() * 0.5
    }
}

/// A mesh's faces, edges and vertices by name, in the order of its range
/// tables: the index a pick id carries is the index here.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EntityNames {
    pub faces: Vec<PersistentName>,
    pub edges: Vec<PersistentName>,
    pub vertices: Vec<PersistentName>,
}

impl EntityNames {
    pub fn of_mesh(mesh: &RenderMesh) -> Self {
        let names =
            |ranges: &[arrix_core::MeshRange]| ranges.iter().map(|r| r.name.clone()).collect();
        EntityNames {
            faces: names(&mesh.faces),
            edges: names(&mesh.edges),
            vertices: mesh.vertices.iter().map(|v| v.name.clone()).collect(),
        }
    }

    pub fn get(&self, kind: TopoKind, index: usize) -> Option<&PersistentName> {
        match kind {
            TopoKind::Face => self.faces.get(index),
            TopoKind::Edge => self.edges.get(index),
            TopoKind::Vertex => self.vertices.get(index),
        }
    }
}

/// The bodies a pick pass drew, in its body-index order, each with the
/// fingerprint of the mesh it drew: a pick resolves only against the very
/// meshes it was drawn from, since the read-back arrives frames later and a
/// refined mesh may have landed in between.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PickBodies(Vec<(BodyKey, u64)>);

/// One body: its uploaded payload, where it sits, and whether it draws.
pub struct BodyEntry<M> {
    pub key: BodyKey,
    pub mesh: M,
    /// The world position its mesh's coordinates are relative to.
    pub origin: DVec3,
    /// Mesh coordinates to the world: a translation by `origin`.
    pub model: Mat4,
    pub visible: bool,
    /// In the mesh's own coordinates.
    pub bounds: Option<Aabb>,
    /// The chord of the mesh uploaded.
    pub chord: f64,
    /// Its entities by name, for picking.
    pub names: EntityNames,
    fingerprint: u64,
}

impl<M> BodyEntry<M> {
    /// The body's box in the world.
    pub fn world_bounds(&self) -> Option<Aabb> {
        let b = self.bounds?;
        let shift = self.model.transform_point3(Vec3::ZERO);
        Some(Aabb {
            min: b.min + shift,
            max: b.max + shift,
        })
    }
}

/// What [`Scene::set_body`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Upload {
    /// A new payload was uploaded.
    Uploaded,
    /// The body already held this mesh: nothing was uploaded.
    Unchanged,
}

/// Every body on screen, in insertion order, which is also draw order and
/// the order [`Scene::visible`] hands them to the render pass.
pub struct Scene<M> {
    bodies: Vec<BodyEntry<M>>,
}

impl<M> Default for Scene<M> {
    fn default() -> Self {
        Self { bodies: Vec::new() }
    }
}

/// A hash of what a mesh draws: equal meshes give equal prints, and a mesh
/// that differs in a position, a normal or a triangle gives another.
fn fingerprint(mesh: &RenderMesh) -> u64 {
    let mut h = DefaultHasher::new();
    for v in [mesh.origin.x, mesh.origin.y, mesh.origin.z] {
        v.to_bits().hash(&mut h);
    }
    for list in [&mesh.positions, &mesh.normals, &mesh.edge_points] {
        list.len().hash(&mut h);
        for p in list {
            p.map(f32::to_bits).hash(&mut h);
        }
    }
    mesh.triangles.hash(&mut h);
    for r in mesh.faces.iter().chain(&mesh.edges) {
        (&r.name, r.start, r.end).hash(&mut h);
    }
    for v in &mesh.vertices {
        (&v.name, v.position.map(f32::to_bits)).hash(&mut h);
    }
    h.finish()
}

impl<M: BodyPayload> Scene<M> {
    /// Uploads `mesh` as `key`'s geometry, replacing what the body held and
    /// leaving every other body's buffers alone, unless the body already
    /// holds this very mesh. A body that exists keeps its visibility, so a
    /// refined mesh does not un-hide a hidden body.
    pub fn set_body(&mut self, ctx: &M::Context, key: &BodyKey, mesh: &RenderMesh) -> Upload {
        let print = fingerprint(mesh);
        let existing = self.bodies.iter_mut().find(|b| &b.key == key);
        if let Some(entry) = existing {
            entry.chord = mesh.chord;
            if entry.fingerprint == print {
                return Upload::Unchanged;
            }
            entry.mesh = M::upload(ctx, key, mesh);
            entry.origin = mesh.origin;
            entry.model = model(mesh.origin);
            entry.bounds = Aabb::of_mesh(mesh);
            entry.names = EntityNames::of_mesh(mesh);
            entry.fingerprint = print;
            return Upload::Uploaded;
        }
        self.bodies.push(BodyEntry {
            key: key.clone(),
            mesh: M::upload(ctx, key, mesh),
            origin: mesh.origin,
            model: model(mesh.origin),
            visible: true,
            bounds: Aabb::of_mesh(mesh),
            chord: mesh.chord,
            names: EntityNames::of_mesh(mesh),
            fingerprint: print,
        });
        Upload::Uploaded
    }
}

/// The world translation of a mesh's origin, in `f32`: the world is metres
/// and a document fits it, so the cast keeps what a screen shows.
fn model(origin: DVec3) -> Mat4 {
    Mat4::from_translation(origin.as_vec3())
}

impl<M> Scene<M> {
    /// Drops a body's entry and its buffers. `true` if it was there.
    pub fn remove_body(&mut self, key: &BodyKey) -> bool {
        let before = self.bodies.len();
        self.bodies.retain(|b| &b.key != key);
        self.bodies.len() != before
    }

    /// Keeps the bodies `keep` names and drops the rest: the bodies a newer
    /// evaluation no longer ends with.
    pub fn retain(&mut self, mut keep: impl FnMut(&BodyKey) -> bool) {
        self.bodies.retain(|b| keep(&b.key));
    }

    /// Shows or hides a body without touching its buffers.
    pub fn set_body_visible(&mut self, key: &BodyKey, visible: bool) -> bool {
        match self.bodies.iter_mut().find(|b| &b.key == key) {
            Some(entry) => {
                entry.visible = visible;
                true
            }
            None => false,
        }
    }

    pub fn clear(&mut self) {
        self.bodies.clear();
    }

    pub fn contains(&self, key: &BodyKey) -> bool {
        self.bodies.iter().any(|b| &b.key == key)
    }

    pub fn len(&self) -> usize {
        self.bodies.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bodies.is_empty()
    }

    /// Every body in draw order, hidden ones included.
    pub fn entries(&self) -> impl Iterator<Item = &BodyEntry<M>> + '_ {
        self.bodies.iter()
    }

    pub fn keys(&self) -> impl Iterator<Item = &BodyKey> + '_ {
        self.bodies.iter().map(|b| &b.key)
    }

    /// The bodies that draw this frame, in scene order. A body's position
    /// here is the index every highlight range and uniform offset is stated
    /// in.
    pub fn visible(&self) -> impl Iterator<Item = &BodyEntry<M>> + '_ {
        self.bodies.iter().filter(|b| b.visible)
    }

    /// The visible body `key` names, with its index among the visible ones.
    pub fn visible_body(&self, key: &BodyKey) -> Option<(usize, &BodyEntry<M>)> {
        self.visible().enumerate().find(|(_, b)| &b.key == key)
    }

    /// The bodies a pick pass draws now: the visible ones, as many as a
    /// pick id can tell apart.
    pub fn pick_bodies(&self) -> PickBodies {
        PickBodies(
            self.visible()
                .take(MAX_PICK_BODIES)
                .map(|b| (b.key.clone(), b.fingerprint))
                .collect(),
        )
    }

    /// The name of what `hit` landed on in a pass that drew `bodies`, if
    /// that body still holds the mesh it was drawn with.
    pub fn resolve(&self, bodies: &PickBodies, hit: PickHit) -> Option<&PersistentName> {
        let (key, print) = bodies.0.get(hit.body)?;
        let entry = self.bodies.iter().find(|b| &b.key == key)?;
        if entry.fingerprint != *print {
            return None;
        }
        entry.names.get(hit.kind, hit.index)
    }

    /// The box of every visible body in the world.
    pub fn bounds(&self) -> Option<Aabb> {
        self.visible()
            .filter_map(BodyEntry::world_bounds)
            .reduce(Aabb::union)
    }

    /// The centre and radius of the sphere that holds every visible body,
    /// for framing: from the union box's half-diagonal, a fit never too
    /// tight.
    pub fn bounding_sphere(&self) -> Option<(Vec3, f32)> {
        self.bounds().map(|b| (b.center(), b.radius()))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use arrix_core::{Id, MeshRange, NameRoot, PersistentName, SweepPartName, TopoKind};

    use super::*;

    /// Counts uploads instead of touching a GPU: what is under test is how
    /// many bodies an edit re-uploads, which is bookkeeping.
    #[derive(Default)]
    struct Uploads(Cell<usize>);

    struct TestBody {
        faces: usize,
    }

    impl BodyPayload for TestBody {
        type Context = Uploads;

        fn upload(ctx: &Uploads, _: &BodyKey, mesh: &RenderMesh) -> Self {
            ctx.0.set(ctx.0.get() + 1);
            TestBody {
                faces: mesh.faces.len(),
            }
        }
    }

    fn key(n: u64) -> BodyKey {
        BodyKey {
            part: PartId(Id(1)),
            feature: FeatureId(Id(n)),
            slot: SlotName::new("body").unwrap(),
        }
    }

    fn face(n: u64) -> MeshRange {
        let name = PersistentName {
            kind: TopoKind::Face,
            root: NameRoot::Sweep {
                feature: FeatureId(Id(n)),
                index: 0,
                part: SweepPartName::StartCap,
            },
            chain: vec![],
        };
        MeshRange {
            name,
            start: 0,
            end: 0,
        }
    }

    fn mesh(faces: usize) -> RenderMesh {
        RenderMesh {
            origin: DVec3::ZERO,
            chord: 1e-3,
            positions: vec![],
            normals: vec![],
            triangles: vec![],
            faces: (0..faces as u64).map(face).collect(),
            edge_points: vec![],
            edges: vec![],
            vertices: vec![],
        }
    }

    fn unit(origin: DVec3) -> RenderMesh {
        RenderMesh {
            origin,
            positions: vec![[-1.0, -1.0, -1.0], [1.0, 1.0, 1.0]],
            normals: vec![[0.0, 0.0, 1.0]; 2],
            ..mesh(0)
        }
    }

    fn scene_of(ctx: &Uploads, n: u64) -> Scene<TestBody> {
        let mut scene = Scene::default();
        for i in 0..n {
            scene.set_body(ctx, &key(i), &mesh(1));
        }
        scene
    }

    #[test]
    fn refining_one_body_uploads_only_that_body() {
        let ctx = Uploads::default();
        let mut scene = scene_of(&ctx, 4);
        assert_eq!(ctx.0.get(), 4, "four bodies, four uploads");

        assert_eq!(scene.set_body(&ctx, &key(2), &mesh(9)), Upload::Uploaded);

        assert_eq!(ctx.0.get(), 5, "the refined body, and nothing else");
        assert_eq!(scene.visible().count(), 4, "no body was duplicated");
        let (_, entry) = scene.visible_body(&key(2)).expect("still in the scene");
        assert_eq!(entry.mesh.faces, 9, "the refined mesh landed");
    }

    #[test]
    fn a_mesh_the_body_already_holds_uploads_nothing() {
        let ctx = Uploads::default();
        let mut scene = scene_of(&ctx, 2);
        for i in 0..2 {
            assert_eq!(scene.set_body(&ctx, &key(i), &mesh(1)), Upload::Unchanged);
        }
        assert_eq!(ctx.0.get(), 2, "the same meshes again cost nothing");

        // A mesh that moved, though its triangles did not, is another mesh.
        let moved = RenderMesh {
            origin: DVec3::X,
            ..mesh(1)
        };
        assert_eq!(scene.set_body(&ctx, &key(0), &moved), Upload::Uploaded);
        assert_eq!(ctx.0.get(), 3);
    }

    #[test]
    fn visibility_uploads_nothing_and_survives_a_refined_mesh() {
        let ctx = Uploads::default();
        let mut scene = scene_of(&ctx, 3);
        assert!(scene.set_body_visible(&key(1), false));
        assert!(!scene.set_body_visible(&key(9), false), "no such body");
        assert_eq!(ctx.0.get(), 3, "still just the three initial uploads");
        assert_eq!(scene.visible().count(), 2);
        assert!(scene.visible_body(&key(1)).is_none());

        scene.set_body(&ctx, &key(1), &mesh(4));
        assert!(scene.visible_body(&key(1)).is_none(), "still hidden");
        assert_eq!(scene.entries().nth(1).unwrap().mesh.faces, 4);
    }

    #[test]
    fn removing_and_retaining_drop_bodies_from_the_draw_order() {
        let ctx = Uploads::default();
        let mut scene = scene_of(&ctx, 4);

        assert!(scene.remove_body(&key(1)));
        assert!(!scene.remove_body(&key(1)), "already gone");
        assert_eq!(ctx.0.get(), 4, "removal uploads nothing either");
        assert_eq!(
            scene.keys().cloned().collect::<Vec<_>>(),
            vec![key(0), key(2), key(3)]
        );

        scene.retain(|k| k != &key(2));
        assert_eq!(scene.len(), 2);
        assert!(scene.contains(&key(0)) && !scene.contains(&key(2)));
        scene.clear();
        assert!(scene.is_empty());
    }

    #[test]
    fn visible_index_skips_hidden_bodies() {
        let ctx = Uploads::default();
        let mut scene = scene_of(&ctx, 3);
        assert_eq!(scene.visible_body(&key(2)).map(|(i, _)| i), Some(2));
        scene.set_body_visible(&key(0), false);
        assert_eq!(scene.visible_body(&key(2)).map(|(i, _)| i), Some(1));
    }

    #[test]
    fn bounds_cover_every_visible_body_at_its_origin() {
        let ctx = Uploads::default();
        let mut scene: Scene<TestBody> = Scene::default();
        scene.set_body(&ctx, &key(0), &unit(DVec3::ZERO));
        scene.set_body(&ctx, &key(1), &unit(DVec3::new(8.0, 0.0, 0.0)));

        let bounds = scene.bounds().expect("two bodies");
        assert_eq!(bounds.min, Vec3::new(-1.0, -1.0, -1.0));
        assert_eq!(bounds.max, Vec3::new(9.0, 1.0, 1.0));
        let (center, radius) = scene.bounding_sphere().unwrap();
        assert!((center.x - 4.0).abs() < 1e-6, "midway between them");
        assert!((radius - Vec3::new(10.0, 2.0, 2.0).length() * 0.5).abs() < 1e-6);

        scene.set_body_visible(&key(1), false);
        let (center, _) = scene.bounding_sphere().expect("one body left");
        assert!(center.x.abs() < 1e-6, "back to the body at the origin");
        scene.set_body_visible(&key(0), false);
        assert!(scene.bounds().is_none());
    }

    fn face_hit(body: usize, index: usize) -> PickHit {
        PickHit {
            kind: TopoKind::Face,
            body,
            index,
        }
    }

    #[test]
    fn a_pick_resolves_to_the_name_in_the_mesh_it_was_drawn_from() {
        let ctx = Uploads::default();
        let mut scene = scene_of(&ctx, 3);
        scene.set_body(&ctx, &key(1), &mesh(3));
        let drawn = scene.pick_bodies();

        assert_eq!(scene.resolve(&drawn, face_hit(1, 2)), Some(&face(2).name));
        assert_eq!(
            scene.resolve(&drawn, face_hit(1, 3)),
            None,
            "no fourth face"
        );
        assert_eq!(
            scene.resolve(&drawn, face_hit(7, 0)),
            None,
            "no eighth body"
        );
        let edge = PickHit {
            kind: TopoKind::Edge,
            ..face_hit(1, 0)
        };
        assert_eq!(scene.resolve(&drawn, edge), None, "the mesh has no edges");

        // Hiding a body after the pass changes nothing about the answer: the
        // pass's own body order is what the index means.
        scene.set_body_visible(&key(0), false);
        assert_eq!(scene.resolve(&drawn, face_hit(1, 0)), Some(&face(0).name));
        assert_eq!(
            scene.resolve(&scene.pick_bodies(), face_hit(0, 0)),
            Some(&face(0).name),
            "a new pass numbers the visible bodies again"
        );
    }

    #[test]
    fn a_pick_drawn_from_a_mesh_since_replaced_resolves_to_nothing() {
        let ctx = Uploads::default();
        let mut scene = scene_of(&ctx, 1);
        let drawn = scene.pick_bodies();
        scene.set_body(&ctx, &key(0), &mesh(2));
        assert_eq!(scene.resolve(&drawn, face_hit(0, 0)), None);
        scene.remove_body(&key(0));
        assert_eq!(scene.resolve(&drawn, face_hit(0, 0)), None);
    }

    #[test]
    fn a_mesh_with_no_geometry_has_no_bounds() {
        assert_eq!(Aabb::of_mesh(&mesh(2)), None);
    }
}
