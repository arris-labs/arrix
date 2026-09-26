//! Offset a chain of lines and arcs, or a circle, by a signed distance.
//!
//! The geometry is the mitre. A line moves along its normal; an arc keeps
//! its centre and takes radius `r ∓ d`; and at each corner the new end is
//! where the two offset curves cross, the crossing nearest the corner's own
//! offset image. Where the original corner is a smooth joint (a tangency),
//! the copy's is too: its image is the corner pushed out along the shared
//! normal. An open chain's two free ends are pushed along the perpendicular
//! to the end curve.
//!
//! Every copy is a fresh entity on fresh points, sharing one point per corner
//! where the original shares one, and the original is never touched. What
//! ties the copy to it is ordinary constraints (the row set is what
//! `tests/offset_probe.rs` measured), written in this order, each
//! kept only where [`Sketch::check_candidate`] says `Ok`:
//!
//! 1. every arc's and circle's `Concentric` and `DistanceCircleCircle`;
//! 2. `Tangent` at each smooth line–arc joint of the copy. The original need
//!    not hold one: the slot template is smooth by endpoint-over-centre, and
//!    its copy is only pinned at the joint by a first-order row;
//! 3. every line's `Parallel` and `DistanceParallelLines`. A line beside a
//!    tangent arc has no distance row left, because the arc's and the
//!    tangency's imply it;
//! 4. at an open chain's free ends, a construction segment from the
//!    original end to the copy end: `Perpendicular` to the end line, or the
//!    original centre `PointOnLine` of it at an arc end.
//!
//! A typed expression goes on every distance row that stands, so one
//! parameter drives the whole chain; a plain number is stored on each row.
//!
//! The sign. For an open chain a positive distance is to the left of the
//! chain's direction (the picked curve's own, [`super::find_chain`]). For a
//! circle or a closed loop it is outward, whichever way the loop runs.

use std::f64::consts::{PI, TAU};

use arrix_core::{IdMinter, LENGTH_TOLERANCE};

use super::ModifyError;
use super::chain::{Chain, Link, find_chain};
use crate::arrangement::carrier_crossings;
use crate::arrangement::curve::{Curve, along, angle_at, cross, dist, dot, sub, unit, wrap_tau};
use crate::constraint::Constraint;
use crate::draft::Draft;
use crate::entity::{Entity, Point};
use crate::ids::{ConstraintId, EntityId, PointId};
use crate::sketch::Sketch;

/// `|sin|` of the turn at a corner below which the joint is smooth: two
/// tangent curves that the solver left a few nanoradians apart.
const SMOOTH_SIN: f64 = 1.0e-7;

/// What an offset wrote, by id.
#[derive(Debug, Clone, PartialEq)]
pub struct OffsetEdit {
    /// The chain that was copied, in its direction.
    pub chain: Chain,
    /// `(original, copy)` per link, in chain order.
    pub pairs: Vec<(EntityId, EntityId)>,
    /// The copy's corner points, in chain order. A closed chain has one per
    /// link, `corners[i]` where link `i` starts. An open chain has one more
    /// than it has links: the free start, each corner, the free end. A
    /// circle has none.
    pub corners: Vec<PointId>,
    /// The copy's centre of each arc or circle, by the same link.
    pub centers: Vec<(EntityId, PointId)>,
    /// The constraints that tie the copy to the original, in the order they
    /// were written.
    pub constraints: Vec<ConstraintId>,
    /// The distance rows among them: one per arc or circle, and one per line
    /// that has no tangent arc beside it. These carry the typed expression.
    pub distances: Vec<ConstraintId>,
    /// The construction segments that anchor an open chain's free ends.
    pub anchors: Vec<EntityId>,
    /// Rows the design writes that the sketch already implied: not added, and
    /// reported so a refusal is never silent.
    pub dropped: Vec<Constraint>,
}

/// Copies the chain `start` belongs to, `distance` meters to one side, and
/// ties the copy to it. `expr`, when given, is the expression `distance` was
/// typed as: it goes on every distance row, whose value is `|distance|`.
pub fn offset(
    sketch: &mut Draft,
    start: EntityId,
    distance: f64,
    expr: Option<&str>,
) -> Result<OffsetEdit, ModifyError> {
    if !(distance.is_finite() && distance != 0.0) {
        return Err(ModifyError::NotPositive);
    }
    let chain = find_chain(sketch, start)?;
    let shape = Shape::compute(sketch, &chain, distance)?;
    let smooth = shape.smooth.clone();
    let mut edit = shape.write(sketch, chain);
    tie(sketch, &mut edit, &smooth, distance.abs(), expr);
    Ok(edit)
}

/// The copy's curves as polylines, for the tool's hover: the geometry
/// [`offset`] writes, without the rows that tie it (a solve of those rows
/// moves nothing, so the picture is the same) and so cheap enough to redo
/// every frame.
pub fn offset_preview(
    sketch: &Sketch,
    start: EntityId,
    distance: f64,
) -> Result<Vec<Vec<[f64; 2]>>, ModifyError> {
    if !(distance.is_finite() && distance != 0.0) {
        return Err(ModifyError::NotPositive);
    }
    let chain = find_chain(sketch, start)?;
    let shape = Shape::compute(sketch, &chain, distance)?;
    // The copy's ids are the trial's own and never leave it.
    let mut trial = Draft::with_sketch(sketch.clone(), IdMinter::new(0));
    let edit = shape.write(&mut trial, chain);
    Ok(edit
        .pairs
        .iter()
        .filter_map(|(_, copy)| super::curve_samples(&trial, *copy))
        .collect())
}

/// The signed distance `at` lies from the chain `start` belongs to, in
/// [`offset`]'s convention: to the left of an open chain's direction is
/// positive, and so is outward from a circle or a closed loop. Measured from
/// the chain's curve nearest `at`, on its carrier.
pub fn offset_distance_at(sketch: &Sketch, start: EntityId, at: [f64; 2]) -> Option<f64> {
    let chain = find_chain(sketch, start).ok()?;
    if let [link] = chain.links[..]
        && link.from.is_none()
    {
        let Some(Curve::Circle { center, radius }) = Curve::from_entity(sketch, link.entity) else {
            return None;
        };
        return Some(dist(center, at) - radius);
    }
    let reach = |l: &Link| {
        Curve::from_entity(sketch, l.entity).map_or(f64::INFINITY, |c| c.distance_to(at))
    };
    let nearest = chain
        .links
        .iter()
        .min_by(|a, b| reach(a).total_cmp(&reach(b)))?;
    let left = match Prim::read(sketch, nearest).ok()? {
        Prim::Line { a, b } => cross(unit(sub(b, a)), sub(at, a)),
        Prim::Arc {
            center,
            radius,
            ccw,
            ..
        } => {
            let r = dist(center, at);
            if ccw { radius - r } else { r - radius }
        }
    };
    Some(if chain.closed && loop_area(sketch, &chain) > 0.0 {
        -left
    } else {
        left
    })
}

/// A closed chain's signed area, positive when it runs counter-clockwise.
fn loop_area(sketch: &Sketch, chain: &Chain) -> f64 {
    chain
        .links
        .iter()
        .filter_map(|l| {
            Curve::from_entity(sketch, l.entity).map(|c| c.signed_area_term(!l.reversed))
        })
        .sum()
}

/// The copy's geometry, computed before anything is written so an error
/// leaves the sketch alone.
struct Shape {
    /// Where each corner goes; see [`OffsetEdit::corners`].
    corners: Vec<[f64; 2]>,
    /// Each link's offset radius, for the arcs and circles.
    radii: Vec<Option<f64>>,
    /// Whether each corner is a smooth joint. A free end is not.
    smooth: Vec<bool>,
}

impl Shape {
    fn compute(sketch: &Sketch, chain: &Chain, distance: f64) -> Result<Self, ModifyError> {
        if let [link] = chain.links[..]
            && link.from.is_none()
        {
            return Self::circle(sketch, link.entity, distance);
        }
        let prims: Vec<Prim> = chain
            .links
            .iter()
            .map(|l| Prim::read(sketch, l))
            .collect::<Result<_, _>>()?;
        let left = if chain.closed && loop_area(sketch, chain) > 0.0 {
            -distance
        } else {
            distance
        };

        let carriers: Vec<Curve> = prims
            .iter()
            .map(|p| p.carrier(left))
            .collect::<Result<_, _>>()?;
        let n = prims.len();
        let turn = |prev: usize, next: usize| {
            corner(
                &prims[prev],
                &prims[next],
                &carriers[prev],
                &carriers[next],
                left,
            )
        };
        let mut turns = Vec::with_capacity(n + 1);
        if chain.closed {
            for i in 0..n {
                turns.push(turn((i + n - 1) % n, i)?);
            }
        } else {
            turns.push((prims[0].push_start(left), false));
            for i in 1..n {
                turns.push(turn(i - 1, i)?);
            }
            turns.push((prims[n - 1].push_end(left), false));
        }
        let (corners, smooth): (Vec<_>, Vec<_>) = turns.into_iter().unzip();

        for (i, prim) in prims.iter().enumerate() {
            let a = corners[i];
            let b = corners[(i + 1) % corners.len()];
            prim.check(a, b, left)?;
        }
        let radii = carriers
            .iter()
            .map(|c| c.circle().map(|(_, r)| r))
            .collect();
        Ok(Self {
            corners,
            radii,
            smooth,
        })
    }

    fn circle(sketch: &Sketch, id: EntityId, distance: f64) -> Result<Self, ModifyError> {
        let Some(Curve::Circle { radius, .. }) = Curve::from_entity(sketch, id) else {
            return Err(ModifyError::NotAChain);
        };
        let r = radius + distance;
        if r <= LENGTH_TOLERANCE {
            return Err(ModifyError::TooLarge);
        }
        Ok(Self {
            corners: Vec::new(),
            radii: vec![Some(r)],
            smooth: Vec::new(),
        })
    }

    fn write(self, sketch: &mut Draft, chain: Chain) -> OffsetEdit {
        let corners: Vec<PointId> = self
            .corners
            .iter()
            .map(|c| sketch.add_point(Point::new(c[0], c[1])))
            .collect();
        let n = chain.links.len();
        let mut pairs = Vec::with_capacity(n);
        let mut centers = Vec::new();
        for (i, link) in chain.links.iter().enumerate() {
            let copy = match sketch.entities[&link.entity].clone() {
                Entity::Circle { center, .. } => {
                    let c = sketch.points[&center].pos();
                    let cp = sketch.add_point(Point::new(c[0], c[1]));
                    let copy = sketch.add_entity(Entity::Circle {
                        center: cp,
                        radius: self.radii[i].unwrap(),
                    });
                    centers.push((copy, cp));
                    copy
                }
                other => {
                    // The copy runs the way its original does: a link met
                    // end-first gets its corners the other way round.
                    let (a, b) = (corners[i], corners[(i + 1) % corners.len()]);
                    let (start, end) = if link.reversed { (b, a) } else { (a, b) };
                    match other {
                        Entity::Arc { center, .. } => {
                            let c = sketch.points[&center].pos();
                            let cp = sketch.add_point(Point::new(c[0], c[1]));
                            let copy = sketch.add_entity(Entity::Arc {
                                center: cp,
                                start,
                                end,
                            });
                            centers.push((copy, cp));
                            copy
                        }
                        _ => sketch.add_entity(Entity::Line { start, end }),
                    }
                }
            };
            let construction = sketch.is_construction(link.entity);
            sketch.set_construction(copy, construction);
            pairs.push((link.entity, copy));
        }
        OffsetEdit {
            chain,
            pairs,
            corners,
            centers,
            constraints: Vec::new(),
            distances: Vec::new(),
            anchors: Vec::new(),
            dropped: Vec::new(),
        }
    }
}

/// A link read in the chain's direction.
#[derive(Debug, Clone, Copy)]
enum Prim {
    Line {
        a: [f64; 2],
        b: [f64; 2],
    },
    Arc {
        center: [f64; 2],
        radius: f64,
        /// The chain runs counter-clockwise about the centre.
        ccw: bool,
        a: [f64; 2],
        b: [f64; 2],
    },
}

impl Prim {
    fn read(sketch: &Sketch, link: &Link) -> Result<Self, ModifyError> {
        let pos = |p: Option<PointId>| {
            p.and_then(|p| sketch.points.get(&p))
                .map(Point::pos)
                .ok_or(ModifyError::NotAChain)
        };
        let (a, b) = (pos(link.from)?, pos(link.to)?);
        match Curve::from_entity(sketch, link.entity) {
            Some(Curve::Line { .. }) => Ok(Self::Line { a, b }),
            Some(Curve::Arc { center, radius, .. }) => Ok(Self::Arc {
                center,
                radius,
                ccw: !link.reversed,
                a,
                b,
            }),
            _ => Err(ModifyError::NotAChain),
        }
    }

    /// The chain's unit tangent where it enters and where it leaves.
    fn start_tangent(&self) -> [f64; 2] {
        match *self {
            Self::Line { a, b } => unit(sub(b, a)),
            Self::Arc { center, ccw, a, .. } => arc_tangent(center, a, ccw),
        }
    }

    fn end_tangent(&self) -> [f64; 2] {
        match *self {
            Self::Line { a, b } => unit(sub(b, a)),
            Self::Arc { center, ccw, b, .. } => arc_tangent(center, b, ccw),
        }
    }

    /// The whole line or circle this curve lies on, `left` to its left.
    fn carrier(&self, left: f64) -> Result<Curve, ModifyError> {
        match *self {
            Self::Line { a, b } => {
                let n = left_of(unit(sub(b, a)));
                Ok(Curve::Line {
                    a: along(a, n, left),
                    b: along(b, n, left),
                })
            }
            Self::Arc {
                center,
                radius,
                ccw,
                ..
            } => {
                // Counter-clockwise, the left is toward the centre.
                let r = if ccw { radius - left } else { radius + left };
                if r <= LENGTH_TOLERANCE {
                    return Err(ModifyError::TooLarge);
                }
                Ok(Curve::Circle { center, radius: r })
            }
        }
    }

    /// The free start pushed to the offset curve, along the perpendicular.
    fn push_start(&self, left: f64) -> [f64; 2] {
        match *self {
            Self::Line { a, .. } => along(a, left_of(self.start_tangent()), left),
            Self::Arc { center, a, .. } => radial_push(self, center, a, left),
        }
    }

    fn push_end(&self, left: f64) -> [f64; 2] {
        match *self {
            Self::Line { b, .. } => along(b, left_of(self.end_tangent()), left),
            Self::Arc { center, b, .. } => radial_push(self, center, b, left),
        }
    }

    /// Would the copy from `a` to `b` still be this curve, not one the
    /// distance has collapsed or turned inside out?
    fn check(&self, a2: [f64; 2], b2: [f64; 2], left: f64) -> Result<(), ModifyError> {
        match *self {
            Self::Line { a, b } => {
                let d = sub(b2, a2);
                if dot(d, sub(b, a)) > 0.0 && dist(a2, b2) > LENGTH_TOLERANCE {
                    Ok(())
                } else {
                    Err(ModifyError::TooLarge)
                }
            }
            Self::Arc {
                center,
                radius,
                ccw,
                a,
                b,
            } => {
                let r2 = if ccw { radius - left } else { radius + left };
                let sign = if ccw { 1.0 } else { -1.0 };
                let sweep = wrap_tau(sign * (angle_at(center, b) - angle_at(center, a)));
                // Each end moves round the centre by the wrapped angle; the
                // copy's sweep is the original's plus the difference.
                let moved = |from: [f64; 2], to: [f64; 2]| {
                    let d = wrap_tau(angle_at(center, to) - angle_at(center, from));
                    if d > PI { d - TAU } else { d }
                };
                let sweep2 = sweep + sign * (moved(b, b2) - moved(a, a2));
                let direct = wrap_tau(sign * (angle_at(center, b2) - angle_at(center, a2)));
                if sweep2 > LENGTH_TOLERANCE / r2 && (sweep2 - direct).abs() <= 1.0e-6 {
                    Ok(())
                } else {
                    Err(ModifyError::TooLarge)
                }
            }
        }
    }
}

fn arc_tangent(center: [f64; 2], at: [f64; 2], ccw: bool) -> [f64; 2] {
    let u = unit(sub(at, center));
    let t = [-u[1], u[0]];
    if ccw { t } else { [-t[0], -t[1]] }
}

/// The unit normal to the left of a unit tangent.
fn left_of(t: [f64; 2]) -> [f64; 2] {
    [-t[1], t[0]]
}

fn radial_push(prim: &Prim, center: [f64; 2], at: [f64; 2], left: f64) -> [f64; 2] {
    let Prim::Arc { radius, ccw, .. } = *prim else {
        unreachable!("radial_push is for arcs")
    };
    let r = if ccw { radius - left } else { radius + left };
    along(center, unit(sub(at, center)), r)
}

/// Where the chain's copy turns the corner between `prev` and `next`, at the
/// point `prev` leaves by and `next` enters at.
fn corner(
    prev: &Prim,
    next: &Prim,
    prev_carrier: &Curve,
    next_carrier: &Curve,
    left: f64,
) -> Result<([f64; 2], bool), ModifyError> {
    let x = match *prev {
        Prim::Line { b, .. } | Prim::Arc { b, .. } => b,
    };
    let (t_in, t_out) = (prev.end_tangent(), next.start_tangent());
    let (n_in, n_out) = (left_of(t_in), left_of(t_out));
    if cross(t_in, t_out).abs() <= SMOOTH_SIN {
        // Smooth: the copy stays smooth, at the corner pushed along the
        // shared normal. A fold back has no offset.
        return if dot(t_in, t_out) > 0.0 {
            Ok((along(x, n_in, left), true))
        } else {
            Err(ModifyError::NotAChain)
        };
    }
    let image = |n: [f64; 2]| along(x, n, left);
    let hint = {
        let (p, q) = (image(n_in), image(n_out));
        [(p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0]
    };
    carrier_crossings(prev_carrier, next_carrier)
        .into_iter()
        .min_by(|a, b| dist(*a, hint).total_cmp(&dist(*b, hint)))
        .map(|at| (at, false))
        .ok_or(ModifyError::TooLarge)
}

/// Writes the rows that tie the copy to the original, in the order the
/// module documents.
fn tie(
    sketch: &mut Draft,
    edit: &mut OffsetEdit,
    smooth: &[bool],
    distance: f64,
    expr: Option<&str>,
) {
    let n = edit.pairs.len();
    let is_line = |sketch: &Sketch, e: EntityId| matches!(sketch.entities[&e], Entity::Line { .. });
    let keep = |sketch: &mut Draft, edit: &mut OffsetEdit, row: Constraint, dist: bool| {
        if !sketch.check_candidate(&row).is_ok() {
            edit.dropped.push(row);
            return None;
        }
        let id = sketch.add_constraint(row);
        edit.constraints.push(id);
        if dist {
            edit.distances.push(id);
            if let Some(expr) = expr {
                sketch.set_constraint_expr(id, Some(expr.to_owned()));
            }
        }
        Some(id)
    };

    for (orig, copy) in edit.pairs.clone() {
        if !is_line(sketch, orig) {
            keep(
                sketch,
                edit,
                Constraint::Concentric { a: orig, b: copy },
                false,
            );
            let row = Constraint::DistanceCircleCircle {
                a: orig,
                b: copy,
                value: distance,
            };
            keep(sketch, edit, row, true);
        }
    }

    // Smooth line–arc joints. A closed chain has a corner at every link's
    // start; an open one at every link but the first.
    let joints = match (edit.chain.closed, smooth.is_empty()) {
        (_, true) => 0..0, // a circle has no corners
        (true, _) => 0..n,
        (false, _) => 1..n,
    };
    for i in joints {
        if !smooth[i] {
            continue;
        }
        let (a, b) = (edit.pairs[(i + n - 1) % n].1, edit.pairs[i].1);
        let (line, circle) = match (is_line(sketch, a), is_line(sketch, b)) {
            (true, false) => (a, b),
            (false, true) => (b, a),
            _ => continue,
        };
        keep(sketch, edit, Constraint::Tangent { line, circle }, false);
    }

    for (orig, copy) in edit.pairs.clone() {
        if is_line(sketch, orig) {
            keep(
                sketch,
                edit,
                Constraint::Parallel { a: orig, b: copy },
                false,
            );
            let row = Constraint::DistanceParallelLines {
                a: orig,
                b: copy,
                value: distance,
            };
            keep(sketch, edit, row, true);
        }
    }

    if !edit.chain.closed {
        let (first, last) = (edit.chain.links[0], edit.chain.links[n - 1]);
        let ends = [
            (first.from, edit.corners[0], edit.pairs[0].0),
            (last.to, edit.corners[n], edit.pairs[n - 1].0),
        ];
        for (from, to, orig) in ends {
            let (Some(from), to) = (from, to) else {
                continue;
            };
            let anchor = sketch.add_entity(Entity::Line {
                start: from,
                end: to,
            });
            sketch.set_construction(anchor, true);
            let row = match sketch.entities[&orig] {
                Entity::Arc { center, .. } => Constraint::PointOnLine {
                    point: center,
                    line: anchor,
                },
                _ => Constraint::Perpendicular { a: anchor, b: orig },
            };
            if keep(sketch, edit, row, false).is_some() {
                edit.anchors.push(anchor);
            } else {
                sketch.remove_entity(anchor);
            }
        }
    }
}
