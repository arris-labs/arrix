//! The scene: what the viewport draws, one entry per body slot, each with
//! its own uploaded mesh, drawn once per instance (docs/UI-RENDERING.md
//! §The viewport, ADR-0030 §2).
//!
//! A mesh event touches exactly its body's entry, so a document of N bodies
//! costs one upload per event, and a mesh that did not change since the last
//! upload costs none: the evaluator sends every final body's mesh again on
//! each generation, and an edit that left a body alone must not re-upload it.

use std::collections::{BTreeMap, BTreeSet};
use std::hash::{DefaultHasher, Hash, Hasher};

use arrix_core::{
    ComponentId, DVec3, FeatureId, Frame, PartId, PersistentName, RenderMesh, SlotName, TopoKind,
};
use glam::{DMat4, Mat4, Vec3};

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
            .chain(mesh.vertices.iter().map(|v| v.position));
        Aabb::of_points(points)
    }

    /// The box of `points`; `None` for none.
    fn of_points(points: impl IntoIterator<Item = [f32; 3]>) -> Option<Aabb> {
        points
            .into_iter()
            .map(Vec3::from_array)
            .fold(None, |b: Option<Aabb>, p| {
                Some(match b {
                    None => Aabb { min: p, max: p },
                    Some(b) => Aabb {
                        min: b.min.min(p),
                        max: b.max.max(p),
                    },
                })
            })
    }

    /// The box of a mesh's faces, one per feature their names start at,
    /// whatever later features did to them: where a feature's own faces
    /// are on a body it was joined into (a frozen feature's badge).
    pub fn of_features(mesh: &RenderMesh) -> BTreeMap<FeatureId, Aabb> {
        let mut out: BTreeMap<FeatureId, Aabb> = BTreeMap::new();
        for face in &mesh.faces {
            let points = mesh.triangles[face.range()]
                .iter()
                .flatten()
                .filter_map(|&i| mesh.positions.get(i as usize).copied());
            let Some(b) = Aabb::of_points(points) else {
                continue;
            };
            out.entry(face.name.root.feature())
                .and_modify(|a| *a = a.union(b))
                .or_insert(b);
        }
        out
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

    /// The index of the entity `name` names, in the list of its kind.
    pub fn find(&self, name: &PersistentName) -> Option<usize> {
        let list = match name.kind {
            TopoKind::Face => &self.faces,
            TopoKind::Edge => &self.edges,
            TopoKind::Vertex => &self.vertices,
        };
        list.iter().position(|n| n == name)
    }
}

/// Where a body draws in an assembly's context: once per component that
/// instances its part, at the component's world pose (ADR-0030 §2). A
/// part's context has none: each body draws once, at its own origin.
#[derive(Clone, Debug, PartialEq)]
pub struct Instance {
    /// The components from the context's assembly down to the part.
    pub path: Vec<ComponentId>,
    pub key: BodyKey,
    /// Where the part's origin and axes land in the world.
    pub pose: Frame,
}

/// An entity of a component: what a pick resolves to, and what hover,
/// selection and a form's marks name. In a part's context the path is
/// empty.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Hit {
    pub path: Vec<ComponentId>,
    pub name: PersistentName,
}

impl Hit {
    /// `name` in a part's context, of no component.
    pub fn bare(name: PersistentName) -> Self {
        Hit {
            path: Vec::new(),
            name,
        }
    }
}

/// One instance of a body as a frame draws it: the component path it is
/// of, the body, and its mesh coordinates to the world.
pub struct Drawn<'a, M> {
    pub path: &'a [ComponentId],
    pub entry: &'a BodyEntry<M>,
    pub model: Mat4,
}

impl<M> Drawn<'_, M> {
    /// The instance's box in the world.
    pub fn world_bounds(&self) -> Option<Aabb> {
        self.entry.bounds.map(|b| world(self.model, b))
    }

    /// The box in the world of the faces whose names start at `feature`;
    /// `None` when the body has none.
    pub fn feature_bounds(&self, feature: FeatureId) -> Option<Aabb> {
        self.entry
            .features
            .get(&feature)
            .map(|&b| world(self.model, b))
    }
}

/// The world box of the corners of `b`, moved by `model`.
fn world(model: Mat4, b: Aabb) -> Aabb {
    let corners = (0..8).map(|i| {
        let pick = |bit: usize, lo: f32, hi: f32| if i & bit == 0 { lo } else { hi };
        model
            .transform_point3(Vec3::new(
                pick(1, b.min.x, b.max.x),
                pick(2, b.min.y, b.max.y),
                pick(4, b.min.z, b.max.z),
            ))
            .to_array()
    });
    Aabb::of_points(corners).expect("a box has corners")
}

/// Mesh coordinates to the world for a mesh at `origin` in its part, the
/// part placed at `pose`: composed in `f64`, cast last.
fn instance_model(pose: &Frame, origin: DVec3) -> Mat4 {
    let place = DMat4::from_cols(
        pose.x_axis().extend(0.0),
        pose.y_axis().extend(0.0),
        pose.z_axis().extend(0.0),
        pose.origin().extend(1.0),
    );
    (place * DMat4::from_translation(origin)).as_mat4()
}

/// Where a named entity is drawn: its instance's index among the visible
/// instances and its index in the body's faces, edges or vertices, the
/// indices a highlight range is stated in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Located {
    pub body: usize,
    pub kind: TopoKind,
    pub index: usize,
}

/// The instances a pick pass drew, in its body-index order, each with the
/// fingerprint of the mesh it drew: a pick resolves only against the very
/// meshes it was drawn from, since the read-back arrives frames later and a
/// refined mesh may have landed in between.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PickBodies(Vec<(Vec<ComponentId>, BodyKey, u64)>);

/// One body: its uploaded payload, where it sits, and whether it draws.
pub struct BodyEntry<M> {
    pub key: BodyKey,
    pub mesh: M,
    /// The position in its part its mesh's coordinates are relative to.
    pub origin: DVec3,
    /// Mesh coordinates to its part's: a translation by `origin`, the
    /// model of a part's context.
    pub model: Mat4,
    pub visible: bool,
    /// In the mesh's own coordinates.
    pub bounds: Option<Aabb>,
    /// The chord of the mesh uploaded.
    pub chord: f64,
    /// Its entities by name, for picking.
    pub names: EntityNames,
    /// The box of its faces by the feature their names start at, in the
    /// mesh's own coordinates.
    pub features: BTreeMap<FeatureId, Aabb>,
    fingerprint: u64,
}

impl<M> BodyEntry<M> {
    /// The body's box in its part, at its own origin.
    pub fn world_bounds(&self) -> Option<Aabb> {
        self.bounds.map(|b| world(self.model, b))
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
/// the order [`Scene::visible`] hands them to the render pass; or, with
/// instances set, each instance of a body in the instances' order. A mesh
/// is uploaded once however many instances draw it.
///
/// A preview (ADR-0019) is a second list beside the document's bodies: its
/// bodies are drawn tinted, in place of the document's bodies it replaces,
/// and never picked, located or framed. Those stay the document's: a pick
/// while a preview shows lands on what the document holds.
pub struct Scene<M> {
    bodies: Vec<BodyEntry<M>>,
    preview: Vec<BodyEntry<M>>,
    /// The document's bodies the preview replaces: not drawn while it
    /// shows, still picked.
    replaced: BTreeSet<BodyKey>,
    /// Where the bodies draw in an assembly's context; `None` in a part's,
    /// where each draws once at its own origin.
    instances: Option<Vec<Instance>>,
}

impl<M> Default for Scene<M> {
    fn default() -> Self {
        Self {
            bodies: Vec::new(),
            preview: Vec::new(),
            replaced: BTreeSet::new(),
            instances: None,
        }
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

/// Uploads `mesh` as `key`'s entry of `bodies`, unless the entry already
/// holds this very mesh.
fn upload<M: BodyPayload>(
    bodies: &mut Vec<BodyEntry<M>>,
    ctx: &M::Context,
    key: &BodyKey,
    mesh: &RenderMesh,
) -> Upload {
    let print = fingerprint(mesh);
    let existing = bodies.iter_mut().find(|b| &b.key == key);
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
        entry.features = Aabb::of_features(mesh);
        entry.fingerprint = print;
        return Upload::Uploaded;
    }
    bodies.push(BodyEntry {
        key: key.clone(),
        mesh: M::upload(ctx, key, mesh),
        origin: mesh.origin,
        model: model(mesh.origin),
        visible: true,
        bounds: Aabb::of_mesh(mesh),
        chord: mesh.chord,
        names: EntityNames::of_mesh(mesh),
        features: Aabb::of_features(mesh),
        fingerprint: print,
    });
    Upload::Uploaded
}

impl<M: BodyPayload> Scene<M> {
    /// Uploads `mesh` as `key`'s geometry, replacing what the body held and
    /// leaving every other body's buffers alone, unless the body already
    /// holds this very mesh. A body that exists keeps its visibility, so a
    /// refined mesh does not un-hide a hidden body.
    pub fn set_body(&mut self, ctx: &M::Context, key: &BodyKey, mesh: &RenderMesh) -> Upload {
        upload(&mut self.bodies, ctx, key, mesh)
    }

    /// Uploads `mesh` as the preview's body `key`, as [`Scene::set_body`]
    /// does the document's.
    pub fn set_preview_body(
        &mut self,
        ctx: &M::Context,
        key: &BodyKey,
        mesh: &RenderMesh,
    ) -> Upload {
        upload(&mut self.preview, ctx, key, mesh)
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
        self.clear_preview();
    }

    /// Draws the bodies as `instances` (an assembly's context), or each
    /// once at its own origin with `None` (a part's). Uploads nothing: an
    /// instance of a body not in the scene draws nothing until it is.
    pub fn set_instances(&mut self, instances: Option<Vec<Instance>>) {
        self.instances = instances;
    }

    pub fn instances(&self) -> Option<&[Instance]> {
        self.instances.as_deref()
    }

    /// Starts a new preview: its bodies are the ones `keep` names of those
    /// already uploaded (a body it shares with the last keeps its buffers),
    /// and it replaces the document's bodies `replaced` names.
    pub fn start_preview(
        &mut self,
        mut keep: impl FnMut(&BodyKey) -> bool,
        replaced: BTreeSet<BodyKey>,
    ) {
        self.preview.retain(|b| keep(&b.key));
        self.replaced = replaced;
    }

    /// Drops the preview: the document's bodies are drawn again.
    pub fn clear_preview(&mut self) {
        self.preview.clear();
        self.replaced.clear();
    }

    /// The preview's bodies, in the order they were uploaded.
    pub fn preview(&self) -> impl Iterator<Item = &BodyEntry<M>> + '_ {
        self.preview.iter()
    }

    /// Whether the preview replaces the document's body `key`.
    pub fn replaced(&self, key: &BodyKey) -> bool {
        self.replaced.contains(key)
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

    /// The instances that draw this frame, in scene order: each visible
    /// body once in a part's context, each instance of a visible body in an
    /// assembly's. An instance's position here is the index every highlight
    /// range, uniform offset and pick id is stated in.
    pub fn visible(&self) -> impl Iterator<Item = Drawn<'_, M>> + '_ {
        let bodies = self.instances.is_none().then(|| {
            self.bodies.iter().filter(|b| b.visible).map(|entry| Drawn {
                path: &[],
                entry,
                model: entry.model,
            })
        });
        let instances = self.instances.iter().flatten().filter_map(|i| {
            let entry = self.bodies.iter().find(|b| b.key == i.key && b.visible)?;
            Some(Drawn {
                path: &i.path,
                entry,
                model: instance_model(&i.pose, entry.origin),
            })
        });
        bodies.into_iter().flatten().chain(instances)
    }

    /// The first visible instance of body `key`, with its index among the
    /// visible ones.
    pub fn visible_body(&self, key: &BodyKey) -> Option<(usize, Drawn<'_, M>)> {
        self.visible()
            .enumerate()
            .find(|(_, d)| &d.entry.key == key)
    }

    /// The instances a pick pass draws now: the visible ones, as many as a
    /// pick id can tell apart.
    pub fn pick_bodies(&self) -> PickBodies {
        PickBodies(
            self.visible()
                .take(MAX_PICK_BODIES)
                .map(|d| (d.path.to_vec(), d.entry.key.clone(), d.entry.fingerprint))
                .collect(),
        )
    }

    /// What `hit` landed on in a pass that drew `bodies`, its component
    /// path and name, if that body still holds the mesh it was drawn with.
    pub fn resolve(&self, bodies: &PickBodies, hit: PickHit) -> Option<Hit> {
        let (path, key, print) = bodies.0.get(hit.body)?;
        let entry = self.bodies.iter().find(|b| &b.key == key)?;
        if entry.fingerprint != *print {
            return None;
        }
        Some(Hit {
            path: path.clone(),
            name: entry.names.get(hit.kind, hit.index)?.clone(),
        })
    }

    /// Where the entity `name` of the instance at `path` is drawn, if a
    /// visible instance holds it: a name the scene does not hold (its body
    /// hidden or gone, or of another component) highlights nothing, and is
    /// never re-bound to another entity.
    pub fn locate(&self, path: &[ComponentId], name: &PersistentName) -> Option<Located> {
        self.visible()
            .enumerate()
            .filter(|(_, d)| d.path == path)
            .find_map(|(body, d)| {
                Some(Located {
                    body,
                    kind: name.kind,
                    index: d.entry.names.find(name)?,
                })
            })
    }

    /// The body that holds the entity `name` of the instance at `path`, if
    /// a visible one does.
    pub fn body_of(&self, path: &[ComponentId], name: &PersistentName) -> Option<&BodyKey> {
        self.visible()
            .find(|d| d.path == path && d.entry.names.find(name).is_some())
            .map(|d| &d.entry.key)
    }

    /// Each visible instance that has faces whose names start at
    /// `feature`, with their box in the world.
    pub fn feature_bounds(
        &self,
        feature: FeatureId,
    ) -> impl Iterator<Item = (&BodyKey, Aabb)> + '_ {
        self.visible()
            .filter_map(move |d| Some((&d.entry.key, d.feature_bounds(feature)?)))
    }

    /// The box of every visible instance in the world.
    pub fn bounds(&self) -> Option<Aabb> {
        self.visible()
            .filter_map(|d| d.world_bounds())
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
        let (_, drawn) = scene.visible_body(&key(2)).expect("still in the scene");
        assert_eq!(drawn.entry.mesh.faces, 9, "the refined mesh landed");
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

    fn bare(face_feature: u64) -> Hit {
        Hit::bare(face(face_feature).name)
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

        assert_eq!(scene.resolve(&drawn, face_hit(1, 2)), Some(bare(2)));
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
        assert_eq!(scene.resolve(&drawn, face_hit(1, 0)), Some(bare(0)));
        assert_eq!(
            scene.resolve(&scene.pick_bodies(), face_hit(0, 0)),
            Some(bare(0)),
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
    fn a_preview_beside_the_document_is_never_picked_or_located() {
        let ctx = Uploads::default();
        let mut scene = scene_of(&ctx, 2);
        scene.set_preview_body(&ctx, &key(0), &mesh(3));
        scene.set_preview_body(&ctx, &key(5), &mesh(1));
        scene.start_preview(|_| true, [key(0)].into());
        assert_eq!(scene.len(), 2, "the document's bodies are its own");
        assert_eq!(scene.preview().count(), 2);
        assert!(scene.replaced(&key(0)) && !scene.replaced(&key(1)));
        // A pick and a name are the document's, replaced bodies included.
        let drawn = scene.pick_bodies();
        assert_eq!(scene.resolve(&drawn, face_hit(0, 0)), Some(bare(0)));
        assert_eq!(
            scene.locate(&[], &face(2).name),
            None,
            "the preview's third face"
        );

        // The next preview keeps what it shares; a clear drops the rest.
        let uploads = ctx.0.get();
        scene.start_preview(|k| k == &key(5), BTreeSet::new());
        assert_eq!(scene.preview().count(), 1);
        assert_eq!(
            scene.set_preview_body(&ctx, &key(5), &mesh(1)),
            Upload::Unchanged
        );
        assert_eq!(ctx.0.get(), uploads);
        scene.clear_preview();
        assert_eq!(scene.preview().count(), 0);
        assert!(!scene.replaced(&key(0)));
    }

    #[test]
    fn a_mesh_with_no_geometry_has_no_bounds() {
        assert_eq!(Aabb::of_mesh(&mesh(2)), None);
    }

    #[test]
    fn a_name_locates_in_the_visible_bodies_and_nowhere_else() {
        let ctx = Uploads::default();
        let mut scene = scene_of(&ctx, 3);
        scene.set_body(&ctx, &key(2), &mesh(4));
        let at = |body, index| Located {
            body,
            kind: TopoKind::Face,
            index,
        };
        assert_eq!(scene.locate(&[], &face(3).name), Some(at(2, 3)));
        assert_eq!(scene.locate(&[], &face(9).name), None, "no such face");

        // Hiding a body renumbers the ones after it, as a pick pass does.
        scene.set_body_visible(&key(0), false);
        assert_eq!(scene.locate(&[], &face(3).name), Some(at(1, 3)));
        scene.set_body_visible(&key(2), false);
        assert_eq!(scene.locate(&[], &face(3).name), None, "its body is hidden");
        let mut edge = face(0).name;
        edge.kind = TopoKind::Edge;
        assert_eq!(
            scene.locate(&[], &edge),
            None,
            "a face's name is not an edge's"
        );
    }

    /// A face of `feature` that is one triangle over `points`.
    fn triangle_face(feature: u64, at: u32) -> MeshRange {
        MeshRange {
            start: at,
            end: at + 1,
            ..face(feature)
        }
    }

    #[test]
    fn a_bodys_faces_are_boxed_by_the_feature_their_names_start_at() {
        let mut two = mesh(0);
        two.origin = DVec3::new(10.0, 0.0, 0.0);
        two.positions = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [5.0, 5.0, 5.0],
            [6.0, 5.0, 5.0],
            [5.0, 6.0, 7.0],
        ];
        two.normals = vec![[0.0, 0.0, 1.0]; 6];
        two.triangles = vec![[0, 1, 2], [3, 4, 5]];
        two.faces = vec![triangle_face(1, 0), triangle_face(2, 1)];
        let ctx = Uploads::default();
        let mut scene = Scene::<TestBody>::default();
        scene.set_body(&ctx, &key(9), &two);

        let of = |scene: &Scene<TestBody>, f: u64| {
            scene
                .feature_bounds(FeatureId(Id(f)))
                .map(|(k, b)| (k.clone(), b.min, b.max))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            of(&scene, 1),
            [(key(9), Vec3::new(10.0, 0.0, 0.0), Vec3::new(11.0, 1.0, 0.0))]
        );
        assert_eq!(
            of(&scene, 2),
            [(key(9), Vec3::new(15.0, 5.0, 5.0), Vec3::new(16.0, 6.0, 7.0))]
        );
        assert!(of(&scene, 3).is_empty(), "no face of feature 3");
        // A hidden body has none to show.
        scene.set_body_visible(&key(9), false);
        assert!(of(&scene, 1).is_empty());
    }

    fn component(n: u64) -> ComponentId {
        ComponentId(Id(n))
    }

    /// `key(0)` drawn twice: at the world origin as component 1, and
    /// turned a quarter about Z and moved 10 along X as component 2.
    fn two_instances(scene: &mut Scene<TestBody>) {
        let turned = Frame::new(DVec3::new(10.0, 0.0, 0.0), DVec3::Y, DVec3::Z).unwrap();
        scene.set_instances(Some(vec![
            Instance {
                path: vec![component(1)],
                key: key(0),
                pose: Frame::WORLD_XY,
            },
            Instance {
                path: vec![component(2)],
                key: key(0),
                pose: turned,
            },
        ]));
    }

    #[test]
    fn instances_draw_one_mesh_at_each_pose_from_one_upload() {
        let ctx = Uploads::default();
        let mut scene: Scene<TestBody> = Scene::default();
        let mut body = unit(DVec3::new(1.0, 0.0, 0.0));
        body.faces = vec![face(0)];
        scene.set_body(&ctx, &key(0), &body);
        two_instances(&mut scene);
        assert_eq!(ctx.0.get(), 1, "one mesh, uploaded once");

        let drawn: Vec<_> = scene.visible().collect();
        assert_eq!(drawn.len(), 2);
        assert_eq!(drawn[0].path, [component(1)]);
        assert_eq!(drawn[1].path, [component(2)]);
        // The mesh's origin is inside its part, so the turn carries it too.
        let at = |d: &Drawn<'_, TestBody>| d.model.transform_point3(Vec3::ZERO);
        assert!(at(&drawn[0]).abs_diff_eq(Vec3::new(1.0, 0.0, 0.0), 1e-6));
        assert!(at(&drawn[1]).abs_diff_eq(Vec3::new(10.0, 1.0, 0.0), 1e-6));
        let b = drawn[1].world_bounds().unwrap();
        assert!(b.min.abs_diff_eq(Vec3::new(9.0, 0.0, -1.0), 1e-6), "{b:?}");
        assert!(b.max.abs_diff_eq(Vec3::new(11.0, 2.0, 1.0), 1e-6), "{b:?}");
        let all = scene.bounds().unwrap();
        assert!(all.min.abs_diff_eq(Vec3::new(0.0, -1.0, -1.0), 1e-6));
        assert!(all.max.abs_diff_eq(Vec3::new(11.0, 2.0, 1.0), 1e-6));
        assert_eq!(scene.feature_bounds(FeatureId(Id(0))).count(), 0);

        // Moving an instance uploads nothing; a refined mesh lands in both.
        two_instances(&mut scene);
        assert_eq!(scene.set_body(&ctx, &key(0), &mesh(3)), Upload::Uploaded);
        assert_eq!(ctx.0.get(), 2);
        assert!(scene.visible().all(|d| d.entry.mesh.faces == 3));
    }

    #[test]
    fn a_pick_and_a_name_are_of_one_instance() {
        let ctx = Uploads::default();
        let mut scene = scene_of(&ctx, 1);
        scene.set_body(&ctx, &key(0), &mesh(2));
        two_instances(&mut scene);
        let drawn = scene.pick_bodies();
        let of = |c: u64, f: u64| Hit {
            path: vec![component(c)],
            name: face(f).name,
        };
        assert_eq!(scene.resolve(&drawn, face_hit(1, 1)), Some(of(2, 1)));
        assert_eq!(scene.resolve(&drawn, face_hit(0, 1)), Some(of(1, 1)));
        let at = |body| Located {
            body,
            kind: TopoKind::Face,
            index: 1,
        };
        let name = face(1).name;
        assert_eq!(scene.locate(&[component(2)], &name), Some(at(1)));
        assert_eq!(scene.locate(&[component(1)], &name), Some(at(0)));
        assert_eq!(
            scene.locate(&[component(3)], &name),
            None,
            "no such instance"
        );
        assert_eq!(scene.locate(&[], &name), None, "a bare name is a part's");
        assert_eq!(scene.body_of(&[component(2)], &name), Some(&key(0)));

        // An instance of a body not in the scene, or hidden, draws nothing.
        let mut more = scene.instances().unwrap().to_vec();
        more.insert(
            0,
            Instance {
                path: vec![component(9)],
                key: key(7),
                pose: Frame::WORLD_XY,
            },
        );
        scene.set_instances(Some(more));
        assert_eq!(scene.visible().count(), 2);
        scene.set_body_visible(&key(0), false);
        assert_eq!(scene.visible().count(), 0);
        scene.set_body_visible(&key(0), true);

        // Back to a part's context: each body once, of no component.
        scene.set_instances(None);
        let drawn = scene.pick_bodies();
        assert_eq!(scene.resolve(&drawn, face_hit(0, 1)), Some(bare(1)));
        assert_eq!(scene.resolve(&drawn, face_hit(1, 0)), None);
    }
}
