//! Sketch validation: open vertices, the gaps between them, and degenerate
//! geometry (a collapsed curve, or a duplicate of another), with a purge
//! of the degenerate ones.
//!
//! Every check reads connectivity the same way: two endpoints are joined
//! when they are one point or tied by an active, driving `Coincident`.

use std::collections::BTreeMap;

use crate::constraint::Constraint;
use crate::dsu::Dsu;
use crate::entity::Entity;
use crate::ids::{EntityId, PointId};
use crate::sketch::Sketch;

/// A curve endpoint joined to no other curve's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenVertex {
    pub point_id: PointId,
    pub entity_id: EntityId,
    pub position: [f64; 2],
}

/// Two open vertices within a tolerance of each other: a gap a
/// `Coincident` would close.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenGap {
    pub a: OpenVertex,
    pub b: OpenVertex,
    pub distance: f64,
}

/// What is wrong with a degenerate entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DegenerateKind {
    ZeroLengthLine,
    ZeroRadiusCircle,
    ZeroRadiusArc,
    DuplicateLine,
    DuplicateCircle,
    DuplicateArc,
}

/// A degenerate entity. A duplicate names the entity it duplicates.
#[derive(Debug, Clone, PartialEq)]
pub struct DegenerateEntity {
    pub entity_id: EntityId,
    pub kind: DegenerateKind,
    pub duplicate_of: Option<EntityId>,
}

/// What [`SketchValidation::validate`] checks, and at what tolerance.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationOptions {
    /// Metres, for gaps and degenerate geometry. Default 0.1 mm.
    pub linear_tol: f64,
    pub check_degenerate: bool,
    pub check_open_vertices: bool,
}

impl Default for ValidationOptions {
    fn default() -> Self {
        Self {
            linear_tol: 1e-4,
            check_degenerate: true,
            check_open_vertices: true,
        }
    }
}

/// Whether a sketch closes, and what stops it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ValidationReport {
    pub open_vertices: Vec<OpenVertex>,
    pub open_gaps: Vec<OpenGap>,
    pub degenerate_entities: Vec<DegenerateEntity>,
    /// Regions of the arrangement.
    pub closed_profiles_count: usize,
    /// Boundary loops, the regions' holes included.
    pub total_loops_count: usize,
    /// No open vertex, and at least one region.
    pub is_fully_closed: bool,
    /// Fully closed, and no degenerate entity.
    pub is_valid_for_extrusion: bool,
}

/// The validation checks over one sketch.
pub struct SketchValidation<'a> {
    sketch: &'a Sketch,
    joined: Dsu<PointId>,
}

type Pos = [f64; 2];

fn dist(a: Pos, b: Pos) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// The live candidates for the duplicate passes: every curve that did not
/// collapse, with the positions it is compared by.
#[derive(Default)]
struct Candidates {
    lines: Vec<(EntityId, [PointId; 2], [Pos; 2])>,
    circles: Vec<(EntityId, PointId, f64, Pos)>,
    arcs: Vec<(EntityId, [PointId; 3], [Pos; 3])>,
}

impl<'a> SketchValidation<'a> {
    pub fn new(sketch: &'a Sketch) -> Self {
        let mut joined = Dsu::new();
        for record in sketch.constraints.values() {
            if record.is_active
                && record.is_driving
                && let Constraint::Coincident { a, b } = record.constraint
            {
                joined.union(a, b);
            }
        }
        Self { sketch, joined }
    }

    fn pos(&self, p: PointId) -> Pos {
        self.sketch.points[&p].pos()
    }

    /// Every curve endpoint of a profile curve (construction geometry has
    /// no gaps) joined to no other endpoint, in id order.
    pub fn detect_open_vertices(&mut self) -> Vec<OpenVertex> {
        let mut clusters: BTreeMap<PointId, Vec<(PointId, EntityId)>> = BTreeMap::new();
        for (&id, entity) in self.sketch.entities.iter() {
            if self.sketch.construction.contains(&id) {
                continue;
            }
            for p in open_ends(entity) {
                clusters
                    .entry(self.joined.find(p))
                    .or_default()
                    .push((p, id));
            }
        }
        let mut open: Vec<OpenVertex> = clusters
            .into_values()
            .filter(|c| c.len() == 1)
            .map(|c| OpenVertex {
                point_id: c[0].0,
                entity_id: c[0].1,
                position: self.pos(c[0].0),
            })
            .collect();
        open.sort_by_key(|v| v.point_id);
        open
    }

    /// Pairs of open vertices within `tol` metres, nearest first.
    pub fn detect_open_gaps(&mut self, tol: f64) -> Vec<OpenGap> {
        let open = self.detect_open_vertices();
        let mut gaps = Vec::new();
        for (i, &a) in open.iter().enumerate() {
            for &b in &open[i + 1..] {
                let distance = dist(a.position, b.position);
                if distance <= tol && self.joined.find(a.point_id) != self.joined.find(b.point_id) {
                    gaps.push(OpenGap { a, b, distance });
                }
            }
        }
        gaps.sort_by(|a, b| a.distance.total_cmp(&b.distance));
        gaps
    }

    /// Collapsed curves (a line shorter than `tol`, a circle or arc with a
    /// radius under it, an arc whose ends meet) and duplicates (a curve on
    /// the same points, or within `tol` of another's). Of two duplicates the
    /// one more constraints name survives, then the one with the smaller
    /// id; the other is reported.
    pub fn detect_degenerate_geometries(&mut self, tol: f64) -> Vec<DegenerateEntity> {
        let (mut out, live) = self.collapsed(tol);
        let weight: BTreeMap<EntityId, usize> = live_ids(&live)
            .map(|id| (id, self.constraints_on(id)))
            .collect();
        let survivor = |a: EntityId, b: EntityId| {
            let key = |e: EntityId| (std::cmp::Reverse(weight[&e]), e);
            if key(a) <= key(b) { (a, b) } else { (b, a) }
        };
        let mut gone = std::collections::BTreeSet::new();
        let mut report = |a, b, kind, gone: &mut std::collections::BTreeSet<EntityId>| {
            let (keep, drop) = survivor(a, b);
            if gone.insert(drop) {
                out.push(DegenerateEntity {
                    entity_id: drop,
                    kind,
                    duplicate_of: Some(keep),
                });
            }
        };
        for (i, &(a, pa, xa)) in live.lines.iter().enumerate() {
            for &(b, pb, xb) in &live.lines[i + 1..] {
                if !gone.contains(&a) && !gone.contains(&b) && self.same_line(pa, xa, pb, xb, tol) {
                    report(a, b, DegenerateKind::DuplicateLine, &mut gone);
                }
            }
        }
        for (i, &(a, ca, ra, xa)) in live.circles.iter().enumerate() {
            for &(b, cb, rb, xb) in &live.circles[i + 1..] {
                let same_centre =
                    self.joined.find(ca) == self.joined.find(cb) || dist(xa, xb) <= tol;
                if !gone.contains(&a) && !gone.contains(&b) && same_centre && (ra - rb).abs() <= tol
                {
                    report(a, b, DegenerateKind::DuplicateCircle, &mut gone);
                }
            }
        }
        for (i, &(a, pa, xa)) in live.arcs.iter().enumerate() {
            for &(b, pb, xb) in &live.arcs[i + 1..] {
                if !gone.contains(&a) && !gone.contains(&b) && self.same_arc(pa, xa, pb, xb, tol) {
                    report(a, b, DegenerateKind::DuplicateArc, &mut gone);
                }
            }
        }
        out
    }

    /// The collapsed curves, and the rest as duplicate candidates.
    fn collapsed(&self, tol: f64) -> (Vec<DegenerateEntity>, Candidates) {
        let mut out = Vec::new();
        let mut live = Candidates::default();
        let mut collapsed = |entity_id, kind| {
            out.push(DegenerateEntity {
                entity_id,
                kind,
                duplicate_of: None,
            })
        };
        for (&id, entity) in &self.sketch.entities {
            match *entity {
                Entity::Line { start, end } => {
                    let (s, e) = (self.pos(start), self.pos(end));
                    if start == end || dist(s, e) < tol {
                        collapsed(id, DegenerateKind::ZeroLengthLine);
                    } else {
                        live.lines.push((id, [start, end], [s, e]));
                    }
                }
                Entity::Circle { center, radius } => {
                    if radius.abs() < tol {
                        collapsed(id, DegenerateKind::ZeroRadiusCircle);
                    } else {
                        live.circles
                            .push((id, center, radius.abs(), self.pos(center)));
                    }
                }
                Entity::Arc { center, start, end } => {
                    let (c, s, e) = (self.pos(center), self.pos(start), self.pos(end));
                    if start == end || dist(c, s) < tol || dist(s, e) < tol {
                        collapsed(id, DegenerateKind::ZeroRadiusArc);
                    } else {
                        live.arcs.push((id, [center, start, end], [c, s, e]));
                    }
                }
                _ => {}
            }
        }
        (out, live)
    }

    fn constraints_on(&self, id: EntityId) -> usize {
        self.sketch
            .constraints
            .values()
            .filter(|r| r.constraint.uses_entity(id))
            .count()
    }

    /// Lines on the same two points, either way round, or within `tol` of
    /// each other end for end.
    fn same_line(
        &mut self,
        pa: [PointId; 2],
        xa: [Pos; 2],
        pb: [PointId; 2],
        xb: [Pos; 2],
        tol: f64,
    ) -> bool {
        let [s1, e1] = pa.map(|p| self.joined.find(p));
        let [s2, e2] = pb.map(|p| self.joined.find(p));
        (s1 == s2 && e1 == e2)
            || (s1 == e2 && e1 == s2)
            || dist(xa[0], xb[0]) + dist(xa[1], xb[1]) <= 2.0 * tol
            || dist(xa[0], xb[1]) + dist(xa[1], xb[0]) <= 2.0 * tol
    }

    /// Arcs on the same centre from the same start to the same end. Only
    /// that way round: an arc from `e` to `s` is the complement of the one
    /// from `s` to `e`, not a copy.
    fn same_arc(
        &mut self,
        pa: [PointId; 3],
        xa: [Pos; 3],
        pb: [PointId; 3],
        xb: [Pos; 3],
        tol: f64,
    ) -> bool {
        let [c1, s1, e1] = pa.map(|p| self.joined.find(p));
        let [c2, s2, e2] = pb.map(|p| self.joined.find(p));
        let same_centre = c1 == c2 || dist(xa[0], xb[0]) <= tol;
        let same_ends =
            (s1 == s2 && e1 == e2) || dist(xa[1], xb[1]) + dist(xa[2], xb[2]) <= 2.0 * tol;
        same_centre && same_ends
    }

    /// Every check, and the regions the sketch closes.
    pub fn validate(&mut self, options: &ValidationOptions) -> ValidationReport {
        let (open_vertices, open_gaps) = if options.check_open_vertices {
            (
                self.detect_open_vertices(),
                self.detect_open_gaps(options.linear_tol),
            )
        } else {
            (Vec::new(), Vec::new())
        };
        let degenerate_entities = if options.check_degenerate {
            self.detect_degenerate_geometries(options.linear_tol)
        } else {
            Vec::new()
        };
        let profiles = crate::region::find_profiles(self.sketch);
        let is_fully_closed = open_vertices.is_empty() && !profiles.is_empty();
        ValidationReport {
            closed_profiles_count: profiles.len(),
            total_loops_count: profiles.iter().map(|p| 1 + p.holes.len()).sum(),
            is_fully_closed,
            is_valid_for_extrusion: is_fully_closed && degenerate_entities.is_empty(),
            open_vertices,
            open_gaps,
            degenerate_entities,
        }
    }

    /// Removes the degenerate entities, the constraints that named them and
    /// the points only they used, and returns what went. Positions are left
    /// as they are: the next solve re-solves them.
    pub fn purge_degenerate_geometries(sketch: &mut Sketch, tol: f64) -> Vec<EntityId> {
        let removed: Vec<EntityId> = SketchValidation::new(sketch)
            .detect_degenerate_geometries(tol)
            .into_iter()
            .map(|d| d.entity_id)
            .collect();
        let mut their_points = Vec::new();
        for &id in &removed {
            if let Some(e) = sketch.entities.get(&id) {
                their_points.extend(e.point_ids());
            }
            sketch.remove_entity(id);
        }
        for p in their_points {
            let used = sketch.entities.values().any(|e| e.point_ids().contains(&p))
                || sketch
                    .constraints
                    .values()
                    .any(|r| r.constraint.uses_point(p));
            if !used {
                sketch.remove_point(p);
            }
        }
        removed
    }
}

fn live_ids(live: &Candidates) -> impl Iterator<Item = EntityId> + '_ {
    let lines = live.lines.iter().map(|l| l.0);
    let circles = live.circles.iter().map(|c| c.0);
    let arcs = live.arcs.iter().map(|a| a.0);
    lines.chain(circles).chain(arcs)
}

/// The endpoints of an open curve; none for a closed one or a point.
fn open_ends(entity: &Entity) -> Vec<PointId> {
    match entity {
        Entity::Line { start, end } | Entity::Arc { start, end, .. } => vec![*start, *end],
        #[cfg(feature = "conics")]
        Entity::ArcOfEllipse { start, end, .. } => vec![*start, *end],
        #[cfg(feature = "conics")]
        Entity::BSpline {
            control_points,
            periodic: false,
            ..
        } if control_points.len() >= 2 => {
            vec![control_points[0], control_points[control_points.len() - 1]]
        }
        _ => Vec::new(),
    }
}
