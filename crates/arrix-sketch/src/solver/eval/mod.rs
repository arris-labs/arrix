//! One definition per constraint kind.
//!
//! Before this module the same formula was written down three times — once in
//! `push_constraint_residuals`, once in `push_constraint_jacobian`, and a
//! third time in `compute_reference_dimension_value` — and the three agreed
//! only because a finite-difference test said so. Here each kind has exactly
//! one function, and it writes its rows into a [`ConstraintEval`] sink:
//!
//! ```ignore
//! out.row(pa.x - pb.x, |g| {
//!     g.pt(a, 1.0, 0.0);
//!     g.pt(b, -1.0, 0.0);
//! });
//! ```
//!
//! The sink decides what the caller actually gets:
//!
//! * residuals only — the gradient closure is never called (the solver's hot
//!   path, and what [`measured_value`] reads),
//! * residuals + analytical Jacobian rows,
//! * the *sparsity* those rows imply, for [`System::partition`] — which used
//!   to assemble a throwaway Jacobian purely to learn which columns a
//!   constraint touches.
//!
//! A dimensional kind also calls [`ConstraintEval::measure`] with the quantity
//! its residual compares against `value`, so a reference (non-driving)
//! dimension displays *the residual's own measurement* rather than a parallel
//! re-derivation of it.

use std::collections::BTreeMap;

use arrix_core::LENGTH_TOLERANCE;

use crate::constraint::Constraint;
use crate::entity::Entity;
use crate::ids::{EntityId, PointId};
use crate::sketch::Sketch;

use super::system::*;
use super::*;

mod basic;
mod block;
#[cfg(feature = "conics")]
mod conics;
mod datum;
mod dimensions;

/// Blocked-entity targets, as [`System`] froze them at build time.
pub(crate) type BlockedTargets = BTreeMap<EntityId, BlockedTarget>;

/// The sink one constraint's evaluation writes its rows into.
///
/// Construct it for what you need: [`residuals_only`](Self::residuals_only),
/// [`with_jacobian`](Self::with_jacobian), or [`sparsity`](Self::sparsity).
/// One sink can absorb many constraints in a row — that is how the whole
/// system's residual vector and Jacobian are assembled.
pub(crate) struct ConstraintEval<'a> {
    /// `None` in residual-only mode: no columns to map gradients onto, so the
    /// gradient closures are skipped entirely.
    sys: Option<&'a System>,
    sparsity: bool,
    residuals: Vec<f64>,
    rows: Vec<Vec<(usize, f64)>>,
    cols: Vec<usize>,
    measured: Option<f64>,
    /// Reused row buffer for sparsity mode, which throws the values away.
    scratch: Vec<(usize, f64)>,
}

impl<'a> ConstraintEval<'a> {
    pub(crate) fn residuals_only() -> Self {
        Self::new(None, false)
    }

    pub(crate) fn with_jacobian(sys: &'a System) -> Self {
        Self::new(Some(sys), false)
    }

    pub(crate) fn sparsity(sys: &'a System) -> Self {
        Self::new(Some(sys), true)
    }

    fn new(sys: Option<&'a System>, sparsity: bool) -> Self {
        Self {
            sys,
            sparsity,
            residuals: Vec::new(),
            rows: Vec::new(),
            cols: Vec::new(),
            measured: None,
            scratch: Vec::new(),
        }
    }

    /// One residual row and, lazily, its gradient. The closure runs only when
    /// the caller asked for derivatives.
    pub(crate) fn row(&mut self, residual: f64, grad: impl FnOnce(&mut Grad<'_>)) {
        self.residuals.push(residual);
        let Some(sys) = self.sys else {
            return;
        };
        if self.sparsity {
            let mut scratch = std::mem::take(&mut self.scratch);
            scratch.clear();
            grad(&mut Grad::new(sys, &mut scratch));
            self.cols.extend(scratch.iter().map(|&(col, _)| col));
            self.scratch = scratch;
        } else {
            let mut row = Vec::new();
            grad(&mut Grad::new(sys, &mut row));
            self.rows.push(row);
        }
    }

    /// `n` rows that are satisfied and depend on nothing — what a constraint
    /// emits when the geometry it names has gone missing. The row count is
    /// still owed to [`constraint_residual_count`], so it must be paid.
    pub(crate) fn zeros(&mut self, n: usize) {
        for _ in 0..n {
            self.row(0.0, |_| {});
        }
    }

    /// The quantity this dimension's residual compares against `value`.
    pub(crate) fn measure(&mut self, value: f64) {
        self.measured = Some(value);
    }

    #[cfg(any(feature = "snells-law", feature = "conics"))]
    /// Snell's law is the one kind whose dimension is a *ratio*, and so the
    /// one whose measurement is undefined at a degenerate denominator.
    pub(crate) fn measure_ratio(&mut self, num: f64, den: f64) {
        if den.abs() > 1e-12 {
            self.measured = Some(num / den);
        }
    }

    pub(crate) fn into_rows(self) -> Vec<Vec<(usize, f64)>> {
        self.rows
    }

    /// Drains everything accumulated so far, leaving the sink reusable for the
    /// next constraint.
    pub(crate) fn take_residuals(&mut self) -> Vec<f64> {
        self.measured = None;
        std::mem::take(&mut self.residuals)
    }

    /// The columns the last constraint's gradients touched (sparsity mode).
    pub(crate) fn take_cols(&mut self) -> Vec<usize> {
        self.residuals.clear();
        self.measured = None;
        std::mem::take(&mut self.cols)
    }
}

/// Sparse gradient assembly: geometry-space derivatives in, packed columns out.
pub(crate) struct Grad<'a> {
    sys: &'a System,
    row: &'a mut Vec<(usize, f64)>,
}

impl<'a> Grad<'a> {
    pub(crate) fn new(sys: &'a System, row: &'a mut Vec<(usize, f64)>) -> Self {
        Self { sys, row }
    }

    /// Structural zeros are dropped: an `x`-only gradient must not cost the
    /// `JᵀJ` accumulator a second column (it is quadratic in row length).
    /// Emitting a duplicate column is still fine — those are summed.
    pub(crate) fn pt(&mut self, p: PointId, gx: f64, gy: f64) {
        if let Some(&col) = self.sys.point_col.get(&p) {
            if gx != 0.0 {
                self.row.push((col, gx));
            }
            if gy != 0.0 {
                self.row.push((col + 1, gy));
            }
        }
    }

    pub(crate) fn radius_var(&mut self, e: EntityId, g: f64) {
        if g == 0.0 {
            return;
        }
        if let Some(&col) = self.sys.radius_col.get(&e) {
            self.row.push((col, g));
        }
    }

    #[cfg(feature = "conics")]
    pub(crate) fn minor_radius_var(&mut self, e: EntityId, g: f64) {
        if g == 0.0 {
            return;
        }
        if let Some(&col) = self.sys.minor_radius_col.get(&e) {
            self.row.push((col, g));
        }
    }

    /// v · ∂radius(e)/∂vars — Circle → the radius var; Arc → ±u on (start, center).
    pub(crate) fn entity_radius(&mut self, sketch: &Sketch, e: EntityId, v: f64) {
        match sketch.entities.get(&e) {
            Some(Entity::Circle { .. }) => {
                self.radius_var(e, v);
            }
            Some(Entity::Arc { center, start, .. }) => {
                let c = point(sketch, *center);
                let s = point(sketch, *start);
                let dx = s.x - c.x;
                let dy = s.y - c.y;
                let r = (dx * dx + dy * dy).sqrt();
                if r >= LENGTH_TOLERANCE {
                    let ux = dx / r;
                    let uy = dy / r;
                    self.pt(*start, v * ux, v * uy);
                    self.pt(*center, -v * ux, -v * uy);
                }
            }
            _ => {}
        }
    }

    /// v · ∂(point_line_signed_distance(P, A->B))/∂vars
    pub(crate) fn signed_dist(
        &mut self,
        sketch: &Sketch,
        p: PointId,
        a: PointId,
        b: PointId,
        scale: f64,
    ) {
        let p_pt = point(sketch, p);
        let a_pt = point(sketch, a);
        let b_pt = point(sketch, b);
        let wx = p_pt.x - a_pt.x;
        let wy = p_pt.y - a_pt.y;
        let dx = b_pt.x - a_pt.x;
        let dy = b_pt.y - a_pt.y;
        let l = (dx * dx + dy * dy).sqrt();
        if l < LENGTH_TOLERANCE {
            return;
        }
        let inv_l = 1.0 / l;
        let s = (wx * dy - wy * dx) * inv_l;
        let ds_dp_x = dy * inv_l;
        let ds_dp_y = -dx * inv_l;
        let ds_db_x = (-wy - s * (dx * inv_l)) * inv_l;
        let ds_db_y = (wx - s * (dy * inv_l)) * inv_l;
        let ds_da_x = -ds_dp_x - ds_db_x;
        let ds_da_y = -ds_dp_y - ds_db_y;

        self.pt(p, scale * ds_dp_x, scale * ds_dp_y);
        self.pt(a, scale * ds_da_x, scale * ds_da_y);
        self.pt(b, scale * ds_db_x, scale * ds_db_y);
    }
}

/// H2: ∂θ/∂a = (+a.y/|a|², −a.x/|a|²), ∂θ/∂b = (−b.y/|b|², +b.x/|b|²) for θ = atan2(cross(a, b), dot(a, b)).
pub(crate) fn angle_grad_vectors(ax: f64, ay: f64, bx: f64, by: f64) -> ((f64, f64), (f64, f64)) {
    let la2 = ax * ax + ay * ay;
    let lb2 = bx * bx + by * by;
    let d_da = if la2 >= LENGTH_TOLERANCE * LENGTH_TOLERANCE {
        (ay / la2, -ax / la2)
    } else {
        (0.0, 0.0)
    };
    let d_db = if lb2 >= LENGTH_TOLERANCE * LENGTH_TOLERANCE {
        (-by / lb2, bx / lb2)
    } else {
        (0.0, 0.0)
    };
    (d_da, d_db)
}

#[cfg(feature = "snells-law")]
/// H4: Gradient of g · u(r) w.r.t. vector r = (rx, ry), where u(r) = r / |r|.
pub(crate) fn unit_vec_grad(gx: f64, gy: f64, rx: f64, ry: f64) -> (f64, f64) {
    let l = (rx * rx + ry * ry).sqrt();
    if l < LENGTH_TOLERANCE {
        return (0.0, 0.0);
    }
    let inv_l = 1.0 / l;
    let ux = rx * inv_l;
    let uy = ry * inv_l;
    let k = (gx * uy - gy * ux) * inv_l;
    (k * uy, -k * ux)
}

/// H5: Jacobians of 2D reflection R(P, A, B) = 2(A + t d) - P w.r.t. P, A, B.
#[allow(clippy::type_complexity)]
pub(crate) fn reflect_grad_matrices(
    px: f64,
    py: f64,
    ax: f64,
    ay: f64,
    bx: f64,
    by: f64,
) -> (
    ((f64, f64), (f64, f64)), // dR/dP: ((dRx/dPx, dRx/dPy), (dRy/dPx, dRy/dPy))
    ((f64, f64), (f64, f64)), // dR/dA: ((dRx/dAx, dRx/dAy), (dRy/dAx, dRy/dAy))
    ((f64, f64), (f64, f64)), // dR/dB: ((dRx/dBx, dRx/dBy), (dRy/dBx, dRy/dBy))
) {
    let dx = bx - ax;
    let dy = by - ay;
    let len2 = dx * dx + dy * dy;
    if len2 < LENGTH_TOLERANCE * LENGTH_TOLERANCE {
        return (
            ((1.0, 0.0), (0.0, 1.0)),
            ((0.0, 0.0), (0.0, 0.0)),
            ((0.0, 0.0), (0.0, 0.0)),
        );
    }
    let inv_len2 = 1.0 / len2;
    let wx = px - ax;
    let wy = py - ay;
    let t = (wx * dx + wy * dy) * inv_len2;

    // dR/dP = 2 d d^T / L^2 - I
    let dr_dp = (
        (2.0 * dx * dx * inv_len2 - 1.0, 2.0 * dx * dy * inv_len2),
        (2.0 * dy * dx * inv_len2, 2.0 * dy * dy * inv_len2 - 1.0),
    );

    // dt/dB = (w - 2t d) / L^2
    let dt_db_x = (wx - 2.0 * t * dx) * inv_len2;
    let dt_db_y = (wy - 2.0 * t * dy) * inv_len2;

    // dR/dB = 2t I + 2 d ⊗ dt/dB
    let dr_db = (
        (2.0 * t + 2.0 * dx * dt_db_x, 2.0 * dx * dt_db_y),
        (2.0 * dy * dt_db_x, 2.0 * t + 2.0 * dy * dt_db_y),
    );

    // dt/dA = (-d - w + 2t d) / L^2
    let dt_da_x = (-(dx + wx) + 2.0 * t * dx) * inv_len2;
    let dt_da_y = (-(dy + wy) + 2.0 * t * dy) * inv_len2;

    // dR/dA = (2 - 2t) I + 2 d ⊗ dt/dA
    let dr_da = (
        ((2.0 - 2.0 * t) + 2.0 * dx * dt_da_x, 2.0 * dx * dt_da_y),
        (2.0 * dy * dt_da_x, (2.0 - 2.0 * t) + 2.0 * dy * dt_da_y),
    );

    (dr_dp, dr_da, dr_db)
}

/// The signed direction vectors of two line entities, and their endpoints.
#[allow(clippy::type_complexity)]
pub(super) fn line_pair_dirs(
    sketch: &Sketch,
    a: EntityId,
    b: EntityId,
) -> Option<(
    (PointId, PointId),
    (PointId, PointId),
    (f64, f64),
    (f64, f64),
)> {
    let (a_start, a_end) = sketch.entities.get(&a)?.line_ends()?;
    let (b_start, b_end) = sketch.entities.get(&b)?.line_ends()?;
    let pas = point(sketch, a_start);
    let pae = point(sketch, a_end);
    let pbs = point(sketch, b_start);
    let pbe = point(sketch, b_end);
    Some((
        (a_start, a_end),
        (b_start, b_end),
        (pae.x - pas.x, pae.y - pas.y),
        (pbe.x - pbs.x, pbe.y - pbs.y),
    ))
}

/// Signed angle from direction `a` to direction `b`, normalised out of the
/// vectors' lengths — the quantity every two-line angular kind differs on.
pub(super) fn signed_angle(a: (f64, f64), b: (f64, f64)) -> f64 {
    let na = (a.0 * a.0 + a.1 * a.1).sqrt().max(LENGTH_TOLERANCE);
    let nb = (b.0 * b.0 + b.1 * b.1).sqrt().max(LENGTH_TOLERANCE);
    let dot = ((a.0 * b.0 + a.1 * b.1) / (na * nb)).clamp(-1.0, 1.0);
    let cross = (a.0 * b.1 - a.1 * b.0) / (na * nb);
    cross.atan2(dot)
}

/// The gradient every two-line angular kind shares: `scale · ∂θ/∂(endpoints)`.
pub(super) fn push_angle_grad(
    g: &mut Grad<'_>,
    (a_start, a_end): (PointId, PointId),
    (b_start, b_end): (PointId, PointId),
    da: (f64, f64),
    db: (f64, f64),
    scale: f64,
) {
    let (d_da, d_db) = angle_grad_vectors(da.0, da.1, db.0, db.1);
    g.pt(a_end, scale * d_da.0, scale * d_da.1);
    g.pt(a_start, -scale * d_da.0, -scale * d_da.1);
    g.pt(b_end, scale * d_db.0, scale * d_db.1);
    g.pt(b_start, -scale * d_db.0, -scale * d_db.1);
}

/// The three points of an arc.
pub(super) fn arc_points(sketch: &Sketch, arc: EntityId) -> Option<(PointId, PointId, PointId)> {
    sketch
        .entities
        .get(&arc)?
        .arc_ends()
        .and_then(|(start, end)| {
            let center = sketch.entities.get(&arc)?.circle_center()?;
            Some((center, start, end))
        })
}

/// The one dispatch site: every constraint kind resolves to exactly one
/// evaluator, and the compiler enforces that a new kind gets one.
pub(crate) fn eval_constraint(
    sketch: &Sketch,
    c: &Constraint,
    blocked_targets: Option<&BlockedTargets>,
    out: &mut ConstraintEval<'_>,
) {
    match *c {
        Constraint::Coincident { a, b } => basic::coincident(sketch, a, b, out),
        Constraint::Horizontal { line } => basic::horizontal(sketch, line, out),
        Constraint::Vertical { line } => basic::vertical(sketch, line, out),
        Constraint::HorizontalPoints { a, b } => basic::horizontal_points(sketch, a, b, out),
        Constraint::VerticalPoints { a, b } => basic::vertical_points(sketch, a, b, out),
        Constraint::Parallel { a, b } => basic::parallel(sketch, a, b, out),
        Constraint::Perpendicular { a, b } => basic::perpendicular(sketch, a, b, out),
        Constraint::Equal { a, b } => basic::equal(sketch, a, b, out),
        Constraint::Concentric { a, b } => basic::concentric(sketch, a, b, out),
        Constraint::Tangent { line, circle } => basic::tangent(sketch, line, circle, out),
        Constraint::TangentCircles { a, b } => basic::tangent_circles(sketch, a, b, out),
        Constraint::Symmetric { a, b, mirror } => basic::symmetric(sketch, a, b, mirror, out),
        Constraint::SymmetricPoints { a, b, center } => {
            basic::symmetric_points(sketch, a, b, center, out)
        }
        Constraint::PointOnPerpBisector { point: p, a, b } => {
            basic::point_on_perp_bisector(sketch, p, a, b, out)
        }
        Constraint::PointOnLine { point: p, line } => basic::point_on_line(sketch, p, line, out),
        Constraint::PointOnCircle { point: p, circle } => {
            basic::point_on_circle(sketch, p, circle, out)
        }
        Constraint::Midpoint { point: p, line } => basic::midpoint(sketch, p, line, out),
        Constraint::Fix { point: p, x, y } => basic::fix(sketch, p, x, y, out),
        #[cfg(feature = "snells-law")]
        Constraint::SnellsLaw {
            ray1_start,
            ray1_end,
            ray2_end,
            boundary,
            ratio,
        } => basic::snells_law(sketch, ray1_start, ray1_end, ray2_end, boundary, ratio, out),

        Constraint::Distance { a, b, value } => dimensions::distance(sketch, a, b, value, out),
        Constraint::HorizontalDistance { a, b, value } => {
            dimensions::horizontal_distance(sketch, a, b, value, out)
        }
        Constraint::VerticalDistance { a, b, value } => {
            dimensions::vertical_distance(sketch, a, b, value, out)
        }
        Constraint::DistancePointLine {
            point: p,
            line,
            value,
        } => dimensions::distance_point_line(sketch, p, line, value, out),
        Constraint::DistanceParallelLines { a, b, value } => {
            dimensions::distance_parallel_lines(sketch, a, b, value, out)
        }
        Constraint::Angle { a, b, value } => dimensions::angle(sketch, a, b, value, out),
        Constraint::AnglePoints {
            a,
            vertex,
            b,
            value,
        } => dimensions::angle_points(sketch, a, vertex, b, value, out),
        Constraint::Radius { target, value } => dimensions::radius(sketch, target, value, out),
        Constraint::Diameter { target, value } => dimensions::diameter(sketch, target, value, out),
        Constraint::ArcLength { arc, value } => dimensions::arc_length(sketch, arc, value, out),
        Constraint::DistanceToAxisX { point: p, value } => {
            dimensions::distance_to_axis_x(sketch, p, value, out)
        }
        Constraint::DistanceToAxisY { point: p, value } => {
            dimensions::distance_to_axis_y(sketch, p, value, out)
        }
        Constraint::DistanceCircleCircle { a, b, value } => {
            dimensions::distance_circle_circle(sketch, a, b, value, out)
        }
        Constraint::DistancePointCircle {
            point: p,
            circle,
            value,
        } => dimensions::distance_point_circle(sketch, p, circle, value, out),

        Constraint::PointOnDatum { point: p, datum }
        | Constraint::CoincidentToDatum { point: p, datum } => {
            datum::point_on_datum(sketch, p, datum, out)
        }
        Constraint::DistanceToDatum {
            point: p,
            datum,
            value,
        } => datum::distance_to_datum(sketch, p, datum, value, out),
        Constraint::AngleWithDatum { line, datum, value } => {
            datum::angle_with_datum(sketch, line, datum, value, out)
        }
        Constraint::SymmetricAcrossDatum { a, b, datum } => {
            datum::symmetric_across_datum(sketch, a, b, datum, out)
        }

        #[cfg(feature = "conics")]
        Constraint::PointOnEllipse { point: p, ellipse } => {
            conics::point_on_ellipse(sketch, p, ellipse, out)
        }
        #[cfg(feature = "conics")]
        Constraint::TangentLineEllipse { line, ellipse } => {
            conics::tangent_line_ellipse(sketch, line, ellipse, out)
        }
        #[cfg(feature = "conics")]
        Constraint::InternalAlignment { ellipse, alignment } => {
            conics::internal_alignment(sketch, ellipse, alignment, out)
        }
        #[cfg(feature = "conics")]
        Constraint::MinorRadius { ellipse, value } => {
            conics::minor_radius(sketch, ellipse, value, out)
        }
        #[cfg(feature = "conics")]
        Constraint::MajorRadius { ellipse, value } => {
            conics::major_radius(sketch, ellipse, value, out)
        }
        #[cfg(feature = "conics")]
        Constraint::PointOnBSpline {
            point: p,
            bspline,
            u,
        } => conics::point_on_bspline(sketch, p, bspline, u, out),
        #[cfg(feature = "conics")]
        Constraint::BSplineTangent { bspline, u, line } => {
            conics::bspline_tangent(sketch, bspline, u, line, out)
        }
        #[cfg(feature = "conics")]
        Constraint::BSplineCurvature { bspline, u, value } => {
            conics::bspline_curvature(sketch, bspline, u, value, out)
        }

        Constraint::Block { entity } => block::block(sketch, entity, blocked_targets, out),
    }
}

/// The live measurement behind a dimensional constraint — what a *reference*
/// (non-driving) dimension displays. It is the same number the constraint's
/// own residual subtracts `value` from, because it comes from that residual's
/// evaluation; non-dimensional kinds measure nothing and return `None`.
pub fn measured_value(sketch: &Sketch, c: &Constraint) -> Option<f64> {
    if !geometry_present(sketch, c) {
        return None;
    }
    let mut eval = ConstraintEval::residuals_only();
    eval_constraint(sketch, c, None, &mut eval);
    eval.measured
}

/// The evaluators index points directly (the solver only ever calls them on
/// geometry it just packed), so a caller holding an arbitrary constraint —
/// a dimension preview built from a live selection, say — is screened first.
fn geometry_present(sketch: &Sketch, c: &Constraint) -> bool {
    c.referenced_points()
        .iter()
        .all(|p| sketch.points.contains_key(p))
        && c.referenced_entities().iter().all(|e| {
            sketch.entities.get(e).is_none_or(|ent| {
                ent.point_ids()
                    .iter()
                    .all(|p| sketch.points.contains_key(p))
            })
        })
}
