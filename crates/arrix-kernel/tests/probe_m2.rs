//! The M2 probe, in SI (plans/c1-m2-naming step 2): what Arris records for
//! `revolve`, `fillet`, `chamfer`, `transform` and `mirror`, and how piece
//! `0` of a split behaves under the edits of the plan's acceptance, read
//! from Arris itself before `arrix-kernel` names any of it (ADR-0010).
//!
//! What it found, and what the steps after it lean on:
//!
//! - A sweep's every entity is `Generated` from exactly one role; a full
//!   turn has no caps and no `EndEdge`, its seams are `StartEdge`s, and a
//!   void is a `Cavity` role on a shell alone.
//! - A blend, a copy and a mirror split nothing: each origin they modify
//!   is one piece, so under ADR-0010 every trimmed neighbour keeps its name.
//! - A pattern's copies fused into one tool are left alone by the fuse, and
//!   each hole's wall through the cut is `Generated` from its own copy's
//!   wall alone, whatever the count.
//! - **Piece `0` of a face split in two follows the input ids**, which an
//!   extrude assigns in the profile's walk order: the same plate walked from
//!   another corner swaps the pieces. Widths, earlier features and the
//!   tool made first do not move it.
//! - A hole that crosses a fillet face and the side face below it fails in
//!   Arris 0.4.0 as `Internal(Split(Dangling))`.

use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::{FRAC_PI_2, TAU};

use arris::Control;
use arris::geom::{Profile, ProfileLoop, ProfileSegment};
use arris::math::{Axis, Frame, Isometry, Point2, Point3, Precision, Reflection, Vec3};
use arris::ops::{self, Fault, OpError, Reason, SplitFault};
use arris::topo::provenance::SweepPart;
use arris::topo::{Body, EntityId, Model, Orientation, Origin, Provenance, Role, Shape};

const MM: f64 = 1e-3;

/// A model in metres, at the kernel's micrometre tolerance.
fn model() -> Model {
    Model::new(Precision {
        default_tolerance: arrix_core::LENGTH_TOLERANCE,
        ..Precision::DEFAULT
    })
    .unwrap()
}

fn p2(x: f64, y: f64) -> Point2 {
    Point2::new(x * MM, y * MM)
}

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x * MM, y * MM, z * MM)
}

/// A closed polygon through `points` in mm, from the first.
fn polygon(points: &[(f64, f64)]) -> ProfileLoop {
    let (x0, y0) = points[0];
    ProfileLoop::Path {
        start: p2(x0, y0),
        segments: points[1..]
            .iter()
            .chain([&points[0]])
            .map(|&(x, y)| ProfileSegment::LineTo(p2(x, y)))
            .collect(),
    }
}

fn plane_at(z: f64) -> Frame {
    Frame::new(p3(0.0, 0.0, z), Vec3::z(), Vec3::x()).unwrap()
}

/// The XZ plane, `u` along X and `v` along Z.
fn xz() -> Frame {
    Frame::new(Point3::origin(), -Vec3::y(), Vec3::x()).unwrap()
}

fn extrude(m: &mut Model, plane: Frame, outer: ProfileLoop, height: f64) -> (Body, Provenance) {
    let normal = plane.z().into_inner();
    let profile = Profile {
        plane,
        outer,
        holes: vec![],
    };
    ops::extrude(m, &profile, normal, height * MM, &Control::NONE).unwrap()
}

/// A `w` × 30 × 5 mm plate, walked anticlockwise from corner `first` of
/// (0, 0), (w, 0), (w, 30), (0, 30): side `s` runs from corner `s`.
fn plate_from(m: &mut Model, w: f64, first: usize) -> (Body, Provenance) {
    let mut corners = vec![(0.0, 0.0), (w, 0.0), (w, 30.0), (0.0, 30.0)];
    corners.rotate_left(first);
    extrude(m, plane_at(0.0), polygon(&corners), 5.0)
}

/// A cylinder of radius `r` mm through the 5 mm plate at `(x, y)`, from
/// 1 mm below it to 1 mm above.
fn pin(m: &mut Model, (x, y): (f64, f64), r: f64) -> Body {
    let circle = ProfileLoop::Circle {
        center: p2(x, y),
        radius: r * MM,
    };
    extrude(m, plane_at(-1.0), circle, 7.0).0
}

/// Every face, edge and vertex of `body`.
fn entities(m: &Model, body: Body) -> BTreeSet<EntityId> {
    let c = m.closure(body).unwrap();
    let faces = c.faces.iter().map(|&f| EntityId::Face(f));
    let edges = c.edges.iter().map(|&e| EntityId::Edge(e));
    let vertices = c.vertices.iter().map(|&v| EntityId::Vertex(v));
    faces.chain(edges).chain(vertices).collect()
}

fn counts(m: &Model, body: Body) -> [usize; 3] {
    let c = m.closure(body).unwrap();
    [c.faces.len(), c.edges.len(), c.vertices.len()]
}

/// The role of every entity a sweep made, checking that each is made by
/// exactly one role and each role makes exactly one entity.
fn roles(m: &Model, body: Body, p: &Provenance) -> BTreeMap<EntityId, SweepPart> {
    let mut out = BTreeMap::new();
    for origin in p.origins_recorded() {
        let part = match origin {
            Origin::Role(Role::Extrude(part) | Role::Revolve(part)) => part,
            other => panic!("{other} is not a sweep's role"),
        };
        if matches!(
            part,
            SweepPart::Body | SweepPart::Shell | SweepPart::Cavity { .. }
        ) {
            continue;
        }
        let generated = p.generated_from(origin);
        assert_eq!(generated.len(), 1, "{origin} made {generated:?}");
        assert!(p.modified_from(origin).is_empty());
        assert!(out.insert(generated[0].id, part).is_none());
    }
    assert_eq!(
        out.keys().copied().collect::<BTreeSet<_>>(),
        entities(m, body)
    );
    out
}

/// The entity `role` made.
fn by_role(roles: &BTreeMap<EntityId, SweepPart>, role: SweepPart) -> EntityId {
    let mut found = roles.iter().filter(|(_, r)| **r == role).map(|(id, _)| *id);
    let id = found.next().unwrap_or_else(|| panic!("no {role:?}"));
    assert!(found.next().is_none());
    id
}

fn edge_handle(m: &Model, body: Body, id: EntityId) -> arris::topo::Edge {
    let EntityId::Edge(id) = id else {
        panic!("{id} is not an edge")
    };
    m.edges(body)
        .unwrap()
        .into_iter()
        .find(|e| e.id == id)
        .unwrap()
}

/// What one operation did to an output entity, by its record: `Kept` when
/// the record does not name it (its id is an input's), piece `k` of
/// `of_n` of one origin, or `Made` from origins.
#[derive(Debug, Clone, PartialEq)]
enum Fate {
    Kept,
    Piece { of: EntityId, k: usize, of_n: usize },
    Made(BTreeSet<EntityId>),
}

/// Every output's fate, checking what ADR-0010's naming leans on: an
/// output is a piece of at most one origin, and never both a piece and
/// made.
fn fates(m: &Model, body: Body, p: &Provenance) -> BTreeMap<EntityId, Fate> {
    let mut pieces: BTreeMap<EntityId, Vec<(EntityId, usize, usize)>> = BTreeMap::new();
    let mut made: BTreeMap<EntityId, BTreeSet<EntityId>> = BTreeMap::new();
    for origin in p.origins_recorded() {
        let Origin::Entity(from) = origin else {
            panic!("{origin} is a role")
        };
        let list = p.modified_from(origin);
        for (k, out) in list.iter().enumerate() {
            pieces
                .entry(out.id)
                .or_default()
                .push((from.id, k, list.len()));
        }
        for out in p.generated_from(origin) {
            made.entry(out.id).or_default().insert(from.id);
        }
    }
    entities(m, body)
        .into_iter()
        .map(|id| {
            let fate = match (pieces.get(&id), made.get(&id)) {
                (Some(list), None) => {
                    let [(of, k, of_n)] = list[..] else {
                        panic!("{id} is a piece of {list:?}")
                    };
                    Fate::Piece { of, k, of_n }
                }
                (None, Some(from)) => Fate::Made(from.clone()),
                (None, None) => Fate::Kept,
                (Some(_), Some(_)) => panic!("{id} is both a piece and made"),
            };
            (id, fate)
        })
        .collect()
}

/// How many origins were split into more than one piece, and how many
/// outputs each fate has: `[kept, pieces, made]`.
fn census(fates: &BTreeMap<EntityId, Fate>) -> (usize, [usize; 3]) {
    let mut split = BTreeSet::new();
    let mut n = [0; 3];
    for fate in fates.values() {
        match fate {
            Fate::Kept => n[0] += 1,
            Fate::Piece { of, of_n, .. } => {
                n[1] += 1;
                if *of_n > 1 {
                    split.insert(*of);
                }
            }
            Fate::Made(_) => n[2] += 1,
        }
    }
    (split.len(), n)
}

/// The most origins any one made entity has.
fn widest_made(fates: &BTreeMap<EntityId, Fate>) -> usize {
    fates
        .values()
        .filter_map(|f| match f {
            Fate::Made(from) => Some(from.len()),
            _ => None,
        })
        .max()
        .unwrap_or(0)
}

/// `origin` itself when the operation kept it, else its one piece.
fn through(fates: &BTreeMap<EntityId, Fate>, origin: EntityId) -> EntityId {
    if fates.get(&origin) == Some(&Fate::Kept) {
        return origin;
    }
    let [piece] = fates
        .iter()
        .filter(|(_, f)| matches!(f, Fate::Piece { of, .. } if *of == origin))
        .map(|(id, _)| *id)
        .collect::<Vec<_>>()[..]
    else {
        panic!("{origin} is not one piece")
    };
    piece
}

// ---------------------------------------------------------------- revolve

/// A stepped shaft's half-section in the XZ plane, mm: radius 10 up to
/// z = 5, then 6 up to z = 15, closed along the axis by segment 5.
fn stepped() -> ProfileLoop {
    polygon(&[
        (0.0, 0.0),
        (10.0, 0.0),
        (10.0, 5.0),
        (6.0, 5.0),
        (6.0, 15.0),
        (0.0, 15.0),
    ])
}

fn revolve(outer: ProfileLoop, holes: Vec<ProfileLoop>, angle: f64) -> (Model, Body, Provenance) {
    let mut m = model();
    let profile = Profile {
        plane: xz(),
        outer,
        holes,
    };
    let axis = Axis::z_at(Point3::origin());
    let (body, p) = ops::revolve(&mut m, &profile, axis, angle, &Control::NONE).unwrap();
    (m, body, p)
}

/// The parts one loop's indices give, for each index in `at`.
fn parts(
    l: usize,
    at: impl IntoIterator<Item = usize>,
    part: fn(usize, usize) -> SweepPart,
) -> BTreeSet<SweepPart> {
    at.into_iter().map(|i| part(l, i)).collect()
}

fn side(loop_index: usize, segment: usize) -> SweepPart {
    SweepPart::Side {
        loop_index,
        segment,
    }
}

fn start_edge(loop_index: usize, segment: usize) -> SweepPart {
    SweepPart::StartEdge {
        loop_index,
        segment,
    }
}

fn end_edge(loop_index: usize, segment: usize) -> SweepPart {
    SweepPart::EndEdge {
        loop_index,
        segment,
    }
}

fn rise(loop_index: usize, vertex: usize) -> SweepPart {
    SweepPart::Rise { loop_index, vertex }
}

fn start_vertex(loop_index: usize, vertex: usize) -> SweepPart {
    SweepPart::StartVertex { loop_index, vertex }
}

fn end_vertex(loop_index: usize, vertex: usize) -> SweepPart {
    SweepPart::EndVertex { loop_index, vertex }
}

fn role_set(roles: &BTreeMap<EntityId, SweepPart>) -> BTreeSet<SweepPart> {
    roles.values().copied().collect()
}

#[test]
fn a_full_turn_has_no_caps_and_its_seams_are_start_edges() {
    let (m, body, p) = revolve(stepped(), vec![], TAU);
    assert_eq!(counts(&m, body), [5, 6, 4]);
    // Two discs, two walls, the annulus at the step; a circle for each
    // vertex off the axis, a seam for each wall, none for a segment
    // square to the axis; nothing for the vertices on it.
    let mut want = parts(0, 0..5, side);
    want.extend(parts(0, 1..5, rise));
    want.extend(parts(0, [1, 3], start_edge));
    want.extend(parts(0, 1..5, start_vertex));
    assert_eq!(role_set(&roles(&m, body, &p)), want);
}

#[test]
fn a_partial_turn_has_both_caps_and_every_end() {
    let (m, body, p) = revolve(stepped(), vec![], FRAC_PI_2);
    assert_eq!(counts(&m, body), [7, 15, 10]);
    // The segment along the axis is an edge both caps share, its
    // `StartEdge`; the vertices on the axis are start vertices alone.
    let mut want = BTreeSet::from([SweepPart::StartCap, SweepPart::EndCap]);
    want.extend(parts(0, 0..5, side));
    want.extend(parts(0, 1..5, rise));
    want.extend(parts(0, 0..6, start_edge));
    want.extend(parts(0, 0..5, end_edge));
    want.extend(parts(0, 0..6, start_vertex));
    want.extend(parts(0, 1..5, end_vertex));
    assert_eq!(role_set(&roles(&m, body, &p)), want);
}

#[test]
fn a_hole_in_a_full_turn_is_a_cavity_shell_named_by_its_loop() {
    let ring = polygon(&[(2.0, 0.0), (10.0, 0.0), (10.0, 10.0), (2.0, 10.0)]);
    let hole = polygon(&[(4.0, 3.0), (8.0, 3.0), (8.0, 7.0), (4.0, 7.0)]);
    let (m, body, p) = revolve(ring, vec![hole], TAU);
    assert_eq!(counts(&m, body), [8, 12, 8]);
    let mut want = BTreeSet::new();
    for l in 0..2 {
        want.extend(parts(l, 0..4, side));
        want.extend(parts(l, 0..4, rise));
        want.extend(parts(l, [1, 3], start_edge));
        want.extend(parts(l, 0..4, start_vertex));
    }
    assert_eq!(role_set(&roles(&m, body, &p)), want);
    let cavity = Role::Revolve(SweepPart::Cavity {
        loop_index: 1,
        segment: 0,
    });
    let shells = m.closure(body).unwrap().shells;
    assert_eq!(shells.len(), 2);
    let [made] = p.generated_from(cavity) else {
        panic!("one cavity shell")
    };
    assert_eq!(made.id, EntityId::Shell(shells[1]));
}

// ---------------------------------------------------------------- blends

/// The 40 × 30 × 5 mm plate's top edge along side `s`.
fn top(s: usize) -> SweepPart {
    end_edge(0, s)
}

type Blend = fn(
    &mut Model,
    Body,
    &[arris::topo::Edge],
    f64,
    &Control<'_>,
) -> Result<(Body, Provenance), OpError>;

/// The plate with `parts` blended at `size` mm: the model, the blended
/// body, the record.
fn blend_plate(
    blend: Blend,
    parts: &[SweepPart],
    size: f64,
) -> Result<(Model, Body, Provenance), OpError> {
    let mut m = model();
    let (plate, p) = plate_from(&mut m, 40.0, 0);
    let roles = roles(&m, plate, &p);
    let edges: Vec<_> = parts
        .iter()
        .map(|&r| edge_handle(&m, plate, by_role(&roles, r)))
        .collect();
    let (body, p) = blend(&mut m, plate, &edges, size * MM, &Control::NONE)?;
    Ok((m, body, p))
}

/// What a blend's result is expected to be: its face, edge and vertex
/// counts, its `[kept, pieces, made]`, the most edges one made entity
/// comes from, and how many inputs it deleted.
struct Expect {
    counts: [usize; 3],
    fates: [usize; 3],
    widest: usize,
    deleted: usize,
}

fn check_blend(what: &str, blend: Blend, edges: &[SweepPart], want: Expect) {
    let (m, body, p) = blend_plate(blend, edges, 2.0).unwrap();
    assert_eq!(counts(&m, body), want.counts, "{what}");
    let f = fates(&m, body, &p);
    assert_eq!(census(&f), (0, want.fates), "{what}: no origin split");
    assert_eq!(widest_made(&f), want.widest, "{what}");
    // The edges and their corner vertices; never a face.
    assert_eq!(p.deleted().count(), want.deleted, "{what}");
    assert!(p.deleted().all(|s| !matches!(s.id, EntityId::Face(_))));
}

#[test]
fn a_blend_splits_nothing_and_makes_its_entities_from_the_edges() {
    let one = Expect {
        counts: [7, 15, 10],
        fates: [15, 8, 9],
        widest: 1,
        deleted: 3,
    };
    check_blend("fillet one", ops::fillet, &[top(1)], one);
    let miter = Expect {
        counts: [8, 17, 11],
        fates: [11, 10, 15],
        widest: 2,
        deleted: 5,
    };
    check_blend("fillet miter", ops::fillet, &[top(1), top(2)], miter);
    let corner = [top(1), top(2), rise(0, 2)];
    let fillet_corner = Expect {
        counts: [10, 22, 13],
        fates: [7, 12, 26],
        widest: 3,
        deleted: 7,
    };
    check_blend("fillet corner", ops::fillet, &corner, fillet_corner);
    let one = Expect {
        counts: [7, 15, 10],
        fates: [15, 8, 9],
        widest: 1,
        deleted: 3,
    };
    check_blend("chamfer one", ops::chamfer, &[top(1)], one);
    let chamfer_corner = Expect {
        counts: [10, 21, 13],
        fates: [7, 12, 25],
        widest: 3,
        deleted: 7,
    };
    check_blend("chamfer corner", ops::chamfer, &corner, chamfer_corner);
}

#[test]
fn an_oversize_fillet_is_blend_too_large() {
    for size in [5.0, 6.0, 30.0] {
        let err = blend_plate(ops::fillet, &[top(1)], size).err().unwrap();
        assert!(
            matches!(
                err,
                OpError::Degenerate {
                    reason: Reason::BlendTooLarge,
                    ..
                }
            ),
            "{size} mm: {err:?}"
        );
    }
}

/// The plate with its top edge along side 1 filleted at 2 mm, then the
/// edge `second` made, through the first fillet, filleted too.
fn second_fillet(second: SweepPart) -> Result<(BTreeMap<EntityId, Fate>, EntityId), OpError> {
    let mut m = model();
    let (plate, p) = plate_from(&mut m, 40.0, 0);
    let roles = roles(&m, plate, &p);
    let first = edge_handle(&m, plate, by_role(&roles, top(1)));
    let (once, p1) = ops::fillet(&mut m, plate, &[first], 2.0 * MM, &Control::NONE).unwrap();
    let f1 = fates(&m, once, &p1);
    let [blend] = f1
        .iter()
        .filter(|(id, f)| matches!(id, EntityId::Face(_)) && matches!(f, Fate::Made(_)))
        .map(|(id, _)| *id)
        .collect::<Vec<_>>()[..]
    else {
        panic!("one blend face")
    };
    let edge = edge_handle(&m, once, through(&f1, by_role(&roles, second)));
    let (twice, p2) = ops::fillet(&mut m, once, &[edge], 2.0 * MM, &Control::NONE)?;
    Ok((fates(&m, twice, &p2), blend))
}

#[test]
fn a_second_fillet_leaves_the_first_alone_or_meets_it_as_a_tangent_chain() {
    // The opposite edge: the first blend face is not in its record.
    let (f, blend) = second_fillet(top(3)).unwrap();
    assert_eq!(census(&f), (0, [21, 8, 9]));
    assert_eq!(f[&blend], Fate::Kept);
    // An edge the first shortened, ending on its contact: refused.
    let err = second_fillet(top(0)).err().unwrap();
    assert!(
        matches!(
            err,
            OpError::Degenerate {
                reason: Reason::TangentChain,
                ..
            }
        ),
        "{err:?}"
    );
}

// ---------------------------------------------------------------- copies

#[test]
fn a_transform_and_a_mirror_modify_every_entity_one_to_one() {
    let mut m = model();
    let (plate, _) = plate_from(&mut m, 40.0, 0);
    let motion = Isometry::from_translation(Vec3::new(0.05, 0.0, 0.0));
    let plane = Reflection::new(p3(50.0, 0.0, 0.0), Vec3::x()).unwrap();
    let (moved, pt) = ops::transform(&mut m, plate, &motion, &Control::NONE).unwrap();
    let (image, pm) = ops::mirror(&mut m, plate, &plane, &Control::NONE).unwrap();
    for (body, p) in [(moved, pt), (image, pm)] {
        let f = fates(&m, body, &p);
        assert_eq!(census(&f), (0, [0, 26, 0]));
        let origins: BTreeSet<_> = f
            .values()
            .map(|f| match f {
                Fate::Piece { of, k: 0, of_n: 1 } => *of,
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(origins, entities(&m, plate));
    }
}

#[test]
fn a_pattern_s_holes_come_each_from_its_own_copy_at_any_count() {
    for n in 1..=4 {
        let mut m = model();
        let (plate, p) = plate_from(&mut m, 40.0, 0);
        let cap = by_role(&roles(&m, plate, &p), SweepPart::EndCap);
        let seed = pin(&mut m, (5.0, 15.0), 2.0);
        let seed_wall = EntityId::Face(m.closure(seed).unwrap().faces[2]);
        // Copy `i`'s wall, and the copy.
        let mut walls = Vec::new();
        for i in 1..=n {
            let motion = Isometry::from_translation(Vec3::new(8.0 * MM * i as f64, 0.0, 0.0));
            let (copy, p) = ops::transform(&mut m, seed, &motion, &Control::NONE).unwrap();
            let [wall] = p.modified_from(Shape::new(seed_wall, Orientation::Forward)) else {
                panic!("one wall")
            };
            walls.push((wall.id, copy));
        }
        // One tool: the fuse of disjoint copies keeps every id, a lump each.
        let mut tool = walls[0].1;
        for &(_, copy) in &walls[1..] {
            let before = entities(&m, tool).len() + entities(&m, copy).len();
            let (t, pt) = ops::fuse(&mut m, tool, copy, &Control::NONE).unwrap();
            assert_eq!(census(&fates(&m, t, &pt)), (0, [before, 0, 0]));
            tool = t;
        }
        assert_eq!(m.closure(tool).unwrap().shells.len(), n);
        let (cut, pc) = ops::cut(&mut m, plate, tool, &Control::NONE).unwrap();
        let f = fates(&m, cut, &pc);
        // The top and the bottom holed, each one piece; per hole a wall,
        // two rims, a seam and its two ends made.
        assert_eq!(census(&f), (0, [24, 2, 6 * n]), "n = {n}");
        assert_eq!(pieces_of(&m, &f, cap).len(), 1);
        for (wall, _) in walls {
            let from_wall: Vec<_> = f
                .iter()
                .filter(|(id, fate)| {
                    matches!(id, EntityId::Face(_))
                        && matches!(fate, Fate::Made(o) if o.contains(&wall))
                })
                .collect();
            let [(_, Fate::Made(origins))] = from_wall[..] else {
                panic!("n = {n}: {from_wall:?}")
            };
            assert_eq!(origins, &BTreeSet::from([wall]), "n = {n}");
        }
    }
}

// ---------------------------------------------------------------- piece 0

/// Faces' centres, mm, rounded.
type Centres = Vec<[i64; 3]>;

/// The point a face's vertices average to, mm, rounded.
fn centre(m: &Model, face: EntityId) -> [i64; 3] {
    let EntityId::Face(face) = face else {
        panic!("{face} is not a face")
    };
    let mut points = BTreeSet::new();
    for l in m.face(face).unwrap().loops() {
        for c in l.coedges() {
            let e = m.edge(c.edge()).unwrap();
            points.extend([e.start(), e.end()]);
        }
    }
    let sum = points
        .iter()
        .map(|&v| m.vertex(v).unwrap().point().coords)
        .fold(Vec3::zeros(), |a, b| a + b);
    let c = sum / points.len() as f64 / MM;
    [c.x, c.y, c.z].map(|x| x.round() as i64)
}

/// The pieces of `origin`'s face in split order, as centres.
fn pieces_of(m: &Model, fates: &BTreeMap<EntityId, Fate>, origin: EntityId) -> Centres {
    let mut out: Vec<(usize, [i64; 3])> = fates
        .iter()
        .filter_map(|(id, f)| match f {
            Fate::Piece { of, k, .. } if *of == origin => Some((*k, centre(m, *id))),
            _ => None,
        })
        .collect();
    if fates.get(&origin) == Some(&Fate::Kept) {
        out.push((0, centre(m, origin)));
    }
    out.sort();
    out.into_iter().map(|(_, c)| c).collect()
}

/// The plate, its top edge along side 1 (x = 40) filleted at 3 mm, drilled
/// at `(x, 15)` by a 2 mm hole: the pieces of the top face and of side 1.
fn hole_by_a_fillet(x: f64) -> Result<[Centres; 2], OpError> {
    let mut m = model();
    let (plate, p) = plate_from(&mut m, 40.0, 0);
    let roles = roles(&m, plate, &p);
    let edge = edge_handle(&m, plate, by_role(&roles, top(1)));
    let (filleted, pf) = ops::fillet(&mut m, plate, &[edge], 3.0 * MM, &Control::NONE).unwrap();
    let ff = fates(&m, filleted, &pf);
    let cap = through(&ff, by_role(&roles, SweepPart::EndCap));
    let wall = through(&ff, by_role(&roles, side(0, 1)));
    let tool = pin(&mut m, (x, 15.0), 2.0);
    let (cut, pc) = ops::cut(&mut m, filleted, tool, &Control::NONE)?;
    let fc = fates(&m, cut, &pc);
    Ok([pieces_of(&m, &fc, cap), pieces_of(&m, &fc, wall)])
}

#[test]
fn a_hole_dragged_across_a_fillet_leaves_the_top_face_one_piece() {
    // The fillet's contact on the top is at x = 37 and on side 1 at z = 2.
    let whole_side = vec![[40, 15, 1]];
    let split_side = vec![[40, 7, 1], [40, 23, 1]];
    for (x, side) in [
        (20.0, &whole_side),
        (33.0, &whole_side),
        (36.0, &whole_side),
        (37.0, &whole_side),
        // Through side 1 too, which it splits, the piece at y < 13 first.
        (38.5, &split_side),
        (41.0, &split_side),
    ] {
        let [top, wall] = hole_by_a_fillet(x).unwrap();
        assert_eq!(top.len(), 1, "x = {x}: {top:?}");
        assert_eq!(&wall, side, "x = {x}");
    }
    // An Arris gap, met here: between those two, Arris 0.4.0's cut fails.
    // The recipe is in this test; Arris's own fixture follows from step 8's
    // export (plans/c1-m2-naming, Open questions).
    let err = hole_by_a_fillet(39.0).err().unwrap();
    assert!(
        matches!(
            err,
            OpError::Internal(Fault::Split(SplitFault::Dangling { .. }))
        ),
        "{err:?}"
    );
}

/// The plate walked from corner `first`, `before` applied, then a 4 mm
/// channel 2 mm deep cut across it at x = 18 mm, which splits the top in
/// two: the top face's pieces in split order.
fn channel(
    w: f64,
    first: usize,
    before: impl FnOnce(&mut Model, Body) -> (Body, Option<Provenance>),
) -> Centres {
    let mut m = model();
    let (plate, p) = plate_from(&mut m, w, first);
    let mut cap = by_role(&roles(&m, plate, &p), SweepPart::EndCap);
    let (body, p) = before(&mut m, plate);
    if let Some(p) = p {
        cap = through(&fates(&m, body, &p), cap);
    }
    let tool = extrude(
        &mut m,
        plane_at(3.0),
        polygon(&[(18.0, -1.0), (22.0, -1.0), (22.0, 31.0), (18.0, 31.0)]),
        3.0,
    )
    .0;
    let (cut, pc) = ops::cut(&mut m, body, tool, &Control::NONE).unwrap();
    pieces_of(&m, &fates(&m, cut, &pc), cap)
}

fn nothing(_: &mut Model, b: Body) -> (Body, Option<Provenance>) {
    (b, None)
}

/// Which piece is `0`: the one right of the channel or the one left.
fn first_piece(pieces: &[[i64; 3]]) -> &'static str {
    let [a, b] = pieces else {
        panic!("two pieces: {pieces:?}")
    };
    if a[0] > b[0] { "right" } else { "left" }
}

#[test]
fn piece_0_of_a_split_holds_under_widths_and_earlier_features() {
    let drill = |at: (f64, f64)| {
        move |m: &mut Model, b: Body| {
            let tool = pin(m, at, 1.0);
            let (b, p) = ops::cut(m, b, tool, &Control::NONE).unwrap();
            (b, Some(p))
        }
    };
    let boss = |m: &mut Model, b: Body| {
        let tool = pin(m, (5.0, 5.0), 1.0);
        let (b, p) = ops::fuse(m, b, tool, &Control::NONE).unwrap();
        (b, Some(p))
    };
    let unrelated = |m: &mut Model, b: Body| {
        pin(m, (100.0, 100.0), 1.0);
        (b, None)
    };
    for (what, pieces) in [
        ("as drawn", channel(40.0, 0, nothing)),
        ("wider", channel(60.0, 0, nothing)),
        ("after an unrelated body", channel(40.0, 0, unrelated)),
        (
            "after a hole on the left",
            channel(40.0, 0, drill((5.0, 5.0))),
        ),
        (
            "after a hole on the right",
            channel(40.0, 0, drill((35.0, 25.0))),
        ),
        ("after a boss", channel(40.0, 0, boss)),
    ] {
        assert_eq!(first_piece(&pieces), "right", "{what}: {pieces:?}");
    }
}

#[test]
fn piece_0_of_a_split_holds_when_the_tool_is_made_first() {
    let mut m = model();
    let tool = extrude(
        &mut m,
        plane_at(3.0),
        polygon(&[(18.0, -1.0), (22.0, -1.0), (22.0, 31.0), (18.0, 31.0)]),
        3.0,
    )
    .0;
    let (plate, p) = plate_from(&mut m, 40.0, 0);
    let cap = by_role(&roles(&m, plate, &p), SweepPart::EndCap);
    let (cut, pc) = ops::cut(&mut m, plate, tool, &Control::NONE).unwrap();
    assert_eq!(
        first_piece(&pieces_of(&m, &fates(&m, cut, &pc), cap)),
        "right"
    );
}

/// The finding that stops the plan at step 2 (plans/c1-m2-naming, Open
/// questions): the same plate walked from another corner swaps the
/// pieces. Arris orders them by the ids of what bounds them (its
/// ADR-0009), an extrude numbers a profile's sides in walk order, and the
/// sketcher starts a loop's walk at its oldest entity, so redrawing the
/// two oldest sides of a rectangle is enough.
#[test]
fn piece_0_of_a_split_follows_the_profile_s_walk() {
    let order: Vec<_> = (0..4)
        .map(|first| first_piece(&channel(40.0, first, nothing)))
        .collect();
    assert_eq!(order, ["right", "right", "left", "left"]);
}
