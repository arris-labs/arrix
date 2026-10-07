//! The snap search: one pass per candidate class, in priority order, in
//! plane-local metres. The overlay draws its guides and glyphs, and the
//! draw click commits its [`Inferred`]s — the same result, so a glyph never
//! promises what the click does not commit (docs/UI-RENDERING.md §Sketch
//! mode).

use arrix_core::LENGTH_TOLERANCE;

use super::{ANGLE_TOL, Inference};
use crate::constraint::DatumEntity;
use crate::entity::Entity;
use crate::ids::{EntityId, PointId};
use crate::sketch::Sketch;

/// What one frame's cursor asks of the search.
#[derive(Debug, Clone, Copy)]
pub struct SnapQuery<'a> {
    /// The raw cursor.
    pub cursor: (f64, f64),
    /// The snap radius: the UI's pixel radius at the current zoom, in metres.
    pub tol: f64,
    /// Where the pending segment starts, when the tool's band has a
    /// direction worth locking to horizontal or vertical. `None` for a
    /// rectangle's diagonal and a circle's radius.
    pub direction_from: Option<(f64, f64)>,
    /// A point being dragged: never a candidate — not itself, not the
    /// midpoint of a line it ends, not an alignment source. The cursor is on
    /// top of it every frame, so any of those would snap the drag back to
    /// where it already is.
    pub excluded: Option<PointId>,
    /// Points the user has acquired (dwelt on, or the draw's own): with
    /// the origin, the only alignment sources — a point never hovered
    /// makes no guide.
    pub acquired_points: &'a [PointId],
    /// Lines the user has acquired: only these offer their extension, and
    /// a direction parallel or perpendicular to them.
    pub acquired_lines: &'a [EntityId],
    /// The pending segment from `direction_from` is a line the click draws
    /// (the line tool's segment), so a relation to its
    /// direction binds to it. Off for a radius, whose relations would
    /// commit nothing.
    pub relations: bool,
}

/// An [`Inference`] before the point it binds exists: what a snap commits
/// once the click has placed its point, and, for an orientation, drawn its
/// line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inferred {
    /// Onto an existing point: `Coincident`.
    OnPoint(PointId),
    /// Onto the sketch origin: `CoincidentToDatum`.
    Origin,
    /// Onto a line's midpoint: `Midpoint`.
    Midpoint(EntityId),
    /// The pending segment is horizontal: `Horizontal` on the drawn line.
    Horizontal,
    /// The pending segment is vertical: `Vertical` on the drawn line.
    Vertical,
    /// Level with a point, as an alignment guide showed: `HorizontalPoints`.
    AlignedH(PointId),
    /// Plumb with a point: `VerticalPoints`.
    AlignedV(PointId),
    /// On a sketch axis (`AxisX` or `AxisY`) — an axis snap, or a guide
    /// from the origin: `PointOnDatum`.
    OnAxis(DatumEntity),
    /// On a line (or its extension), circle or arc: `PointOnLine` or
    /// `PointOnCircle`.
    OnCurve(EntityId),
    /// The drawn line parallel to a line: `Parallel`.
    Parallel(EntityId),
    /// The drawn line perpendicular to a line: `Perpendicular`.
    Perpendicular(EntityId),
    /// The drawn line tangent to a circle or arc: `Tangent`.
    Tangent(EntityId),
}

impl Inferred {
    /// The [`Inference`] this is once `point` is placed and, for an
    /// orientation, `line` is drawn from the pending start to it. `None`
    /// when what it names is gone, or an orientation has no line to bind.
    pub fn bind(
        self,
        sketch: &Sketch,
        point: PointId,
        line: Option<EntityId>,
    ) -> Option<Inference> {
        let at = |id: PointId| sketch.point(id).map(|p| [p.x, p.y]);
        match self {
            Self::OnPoint(target) if target != point => Some(Inference::Coincident {
                moving: point,
                target,
                at: at(target)?,
            }),
            Self::OnPoint(_) => None,
            Self::AlignedH(source) if source != point => {
                sketch.point(source)?;
                Some(Inference::AlignedH {
                    moving: point,
                    source,
                })
            }
            Self::AlignedV(source) if source != point => {
                sketch.point(source)?;
                Some(Inference::AlignedV {
                    moving: point,
                    source,
                })
            }
            Self::AlignedH(_) | Self::AlignedV(_) => None,
            Self::OnAxis(datum) => Some(Inference::OnAxis {
                moving: point,
                datum,
            }),
            Self::Origin => Some(Inference::Origin { moving: point }),
            Self::OnCurve(curve) => {
                let at = at(point)?;
                match sketch.entity(curve)? {
                    Entity::Line { .. } => Some(Inference::PointOnLine {
                        moving: point,
                        line: curve,
                        at,
                    }),
                    Entity::Circle { .. } | Entity::Arc { .. } => Some(Inference::PointOnCircle {
                        moving: point,
                        circle: curve,
                        at,
                    }),
                    _ => None,
                }
            }
            Self::Midpoint(line) => {
                let Entity::Line { start, end } = *sketch.entity(line)? else {
                    return None;
                };
                let (a, b) = (at(start)?, at(end)?);
                Some(Inference::Midpoint {
                    moving: point,
                    line,
                    at: [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5],
                })
            }
            Self::Parallel(to) | Self::Perpendicular(to) => {
                let line = line.filter(|l| *l != to)?;
                sketch.entity(to)?.is_line().then_some(())?;
                Some(if matches!(self, Self::Parallel(_)) {
                    Inference::Parallel { line, to }
                } else {
                    Inference::Perpendicular { line, to }
                })
            }
            Self::Tangent(circle) => {
                let line = line?;
                matches!(
                    sketch.entity(circle)?,
                    Entity::Circle { .. } | Entity::Arc { .. }
                )
                .then_some(Inference::Tangent { line, circle })
            }
            Self::Horizontal | Self::Vertical => {
                let line = line?;
                let Entity::Line { start, end } = *sketch.entity(line)? else {
                    return None;
                };
                let (a, b) = (at(start)?, at(end)?);
                Some(if self == Self::Horizontal {
                    Inference::Horizontal {
                        line,
                        start: a,
                        end: [b[0], a[1]],
                    }
                } else {
                    Inference::Vertical {
                        line,
                        start: a,
                        end: [a[0], b[1]],
                    }
                })
            }
        }
    }
}

/// What a glyph marks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlyphKind {
    Point,
    Center,
    Origin,
    Midpoint,
    Horizontal,
    Vertical,
    AxisU,
    AxisV,
    /// On a line, circle or arc — or an acquired line's extension, with a
    /// guide from its nearer end.
    OnCurve,
    /// Where two curves cross.
    Intersection,
    /// A circle's or arc's quadrant point.
    Quadrant,
    /// The pending segment parallel to a line.
    Parallel,
    /// The pending segment perpendicular to a line.
    Perpendicular,
    /// The pending segment ending tangent on a circle or arc.
    Tangent,
}

/// A glyph at `uv`, and the indices into [`Snap::inferences`] of what it
/// commits: one, or two for an intersection or quadrant.
#[derive(Debug, Clone, PartialEq)]
pub struct SnapGlyph {
    pub uv: (f64, f64),
    pub kind: GlyphKind,
    pub inferences: Vec<usize>,
}

/// An alignment guide, `[tracked point, snapped cursor]`, and the index
/// into [`Snap::inferences`] of what it commits.
#[derive(Debug, Clone, PartialEq)]
pub struct SnapGuide {
    pub line: [(f64, f64); 2],
    pub inference: usize,
}

/// One search's result: where the cursor resolved to, the alignment
/// guides, the glyphs, and what a click here commits, in priority order.
#[derive(Debug, Clone, PartialEq)]
pub struct Snap {
    pub uv: (f64, f64),
    pub guides: Vec<SnapGuide>,
    pub glyphs: Vec<SnapGlyph>,
    pub inferences: Vec<Inferred>,
    /// An object snap (point, origin, midpoint) placed the cursor outright.
    pub object: bool,
    /// Which coordinate `(u, v)` a guide, lock or axis already placed:
    /// what a direction relation must leave alone.
    pub locked: (bool, bool),
}

impl Snap {
    /// The cursor where it is: nothing snapped, nothing committed.
    pub fn raw(cursor: (f64, f64)) -> Self {
        Self {
            uv: cursor,
            guides: Vec::new(),
            glyphs: Vec::new(),
            inferences: Vec::new(),
            object: false,
            locked: (false, false),
        }
    }

    /// An object-class snap places the cursor outright.
    pub(super) fn place(&mut self, p: [f64; 2]) {
        self.uv = (p[0], p[1]);
        self.object = true;
    }

    /// A guide that commits `inference`.
    pub(super) fn guide(&mut self, line: [(f64, f64); 2], inference: usize) {
        self.guides.push(SnapGuide { line, inference });
    }

    /// Puts [`Self::inferences`] in commit priority order — an object
    /// snap, the line's own orientation, a relation to another curve,
    /// alignment with a point, an axis —
    /// keeping pass order within a class and every glyph and guide on its
    /// inference. So of two equivalent candidates (a chain's segment level
    /// with its own start is both `Horizontal` and `HorizontalPoints`) the
    /// line's own is the one kept.
    fn prioritise(&mut self) {
        let class = |i: &Inferred| match i {
            Inferred::OnPoint(_)
            | Inferred::Origin
            | Inferred::Midpoint(_)
            | Inferred::OnCurve(_) => 0,
            Inferred::Horizontal | Inferred::Vertical => 1,
            Inferred::Parallel(_) | Inferred::Perpendicular(_) | Inferred::Tangent(_) => 2,
            Inferred::AlignedH(_) | Inferred::AlignedV(_) => 3,
            Inferred::OnAxis(_) => 4,
        };
        let mut order: Vec<usize> = (0..self.inferences.len()).collect();
        order.sort_by_key(|&i| class(&self.inferences[i]));
        let mut new_index = vec![0; order.len()];
        for (new, &old) in order.iter().enumerate() {
            new_index[old] = new;
        }
        self.inferences = order.iter().map(|&i| self.inferences[i]).collect();
        for glyph in &mut self.glyphs {
            for i in &mut glyph.inferences {
                *i = new_index[*i];
            }
        }
        for guide in &mut self.guides {
            guide.inference = new_index[guide.inference];
        }
    }

    /// A glyph and the inference it stands for; the index of that
    /// inference, said once however many glyphs show it (a guide from the
    /// origin and the axis snap are the same `OnAxis`).
    fn infer(&mut self, kind: GlyphKind, uv: (f64, f64), inferred: Inferred) -> usize {
        self.infer_all(kind, uv, &[inferred])[0]
    }

    /// A glyph standing for several inferences at once, each said once.
    pub(super) fn infer_all(
        &mut self,
        kind: GlyphKind,
        uv: (f64, f64),
        inferred: &[Inferred],
    ) -> Vec<usize> {
        let indices: Vec<usize> = inferred
            .iter()
            .map(
                |inferred| match self.inferences.iter().position(|i| i == inferred) {
                    Some(index) => index,
                    None => {
                        self.inferences.push(*inferred);
                        self.inferences.len() - 1
                    }
                },
            )
            .collect();
        self.glyphs.push(SnapGlyph {
            uv,
            kind,
            inferences: indices.clone(),
        });
        indices
    }
}

/// The object passes of the snap search, in priority order: a point (circle
/// centres included), the origin or a line's midpoint wins outright;
/// otherwise alignment guides, the pending segment's H/V lock and the
/// sketch axes each adjust one coordinate. Later passes refine a snap, they
/// never undo one: the H/V lock never pulls an object snap off its target,
/// and joins it only where it already holds.
pub fn snap_search(sketch: &Sketch, query: &SnapQuery) -> Snap {
    let (raw_u, raw_v) = query.cursor;
    let tol = query.tol;
    let mut out = Snap::raw(query.cursor);
    let candidate = |id: PointId| Some(id) != query.excluded;

    // 1. Existing points and circle centres.
    let mut best_pt: Option<(PointId, f64, f64, f64)> = None;
    for (id, p) in sketch.points() {
        let d = (p.x - raw_u).hypot(p.y - raw_v);
        if candidate(*id) && d <= tol && best_pt.as_ref().is_none_or(|(_, bd, _, _)| d < *bd) {
            best_pt = Some((*id, d, p.x, p.y));
        }
    }
    if let Some((id, _, px, py)) = best_pt {
        out.uv = (px, py);
        out.object = true;
        let is_center = sketch
            .entities()
            .values()
            .any(|e| e.circle_center() == Some(id));
        let kind = if is_center {
            GlyphKind::Center
        } else {
            GlyphKind::Point
        };
        out.infer(kind, (px, py), Inferred::OnPoint(id));
    }

    // 2. The origin.
    if !out.object && raw_u.hypot(raw_v) <= tol {
        out.uv = (0.0, 0.0);
        out.object = true;
        out.infer(GlyphKind::Origin, (0.0, 0.0), Inferred::Origin);
    }

    // 3. Line midpoints.
    if !out.object {
        let mut best_mid: Option<(EntityId, f64, f64, f64)> = None;
        for (id, entity) in sketch.entities() {
            let Entity::Line { start, end } = entity else {
                continue;
            };
            if !(candidate(*start) && candidate(*end)) {
                continue;
            }
            if let (Some(a), Some(b)) = (sketch.point(*start), sketch.point(*end)) {
                let (mx, my) = ((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
                let d = (mx - raw_u).hypot(my - raw_v);
                if d <= tol && best_mid.as_ref().is_none_or(|(_, bd, _, _)| d < *bd) {
                    best_mid = Some((*id, d, mx, my));
                }
            }
        }
        if let Some((line, _, mx, my)) = best_mid {
            out.uv = (mx, my);
            out.object = true;
            out.infer(GlyphKind::Midpoint, (mx, my), Inferred::Midpoint(line));
        }
    }
    // 4. Curves: intersection, quadrant, on-curve, extension.
    if !out.object {
        super::curves::snap_to_curves(sketch, query, &mut out);
    }
    let oriented = out
        .inferences
        .iter()
        .any(|i| matches!(i, Inferred::Horizontal | Inferred::Vertical));
    if out.object {
        out.prioritise();
        if oriented {
            return out;
        }
        // An object snap that already lies level or plumb with the pending
        // start is that too: the orientation moves nothing, so it is shown
        // and committed with the snap — a chain closed straight down onto
        // its first point is vertical.
        if let Some((u1, v1)) = query.direction_from {
            let (dx, dy) = ((out.uv.0 - u1).abs(), (out.uv.1 - v1).abs());
            if dx.hypot(dy) > tol && dy <= LENGTH_TOLERANCE {
                out.infer(GlyphKind::Horizontal, out.uv, Inferred::Horizontal);
            } else if dx.hypot(dy) > tol && dx <= LENGTH_TOLERANCE {
                out.infer(GlyphKind::Vertical, out.uv, Inferred::Vertical);
            }
        }
        return out;
    }

    // 5. Horizontal / vertical alignment guides with acquired points and
    // the origin.
    // A guide commits what it shows: level (plumb) with a point is
    // `HorizontalPoints` (`VerticalPoints`), with the origin it is the axis.
    let align_points = query
        .acquired_points
        .iter()
        .filter(|id| candidate(**id))
        .filter_map(|id| sketch.point(*id).map(|p| (Some(*id), p.x, p.y)))
        .chain(std::iter::once((None, 0.0, 0.0)));
    let mut aligned_h: Option<(Option<PointId>, f64, f64)> = None;
    let mut aligned_v: Option<(Option<PointId>, f64, f64)> = None;
    for (id, px, py) in align_points {
        if (py - raw_v).abs() <= tol && (px - raw_u).abs() > tol {
            aligned_h = Some((id, px, py));
        }
        if (px - raw_u).abs() <= tol && (py - raw_v).abs() > tol {
            aligned_v = Some((id, px, py));
        }
    }
    if let Some((id, px, py)) = aligned_h {
        out.uv.1 = py;
        out.locked.1 = true;
        let inferred = id.map_or(Inferred::OnAxis(DatumEntity::AxisX), Inferred::AlignedH);
        let inference = out.infer(GlyphKind::Horizontal, (out.uv.0, py), inferred);
        out.guides.push(SnapGuide {
            line: [(px, py), (out.uv.0, py)],
            inference,
        });
    }
    if let Some((id, px, py)) = aligned_v {
        out.uv.0 = px;
        out.locked.0 = true;
        let inferred = id.map_or(Inferred::OnAxis(DatumEntity::AxisY), Inferred::AlignedV);
        let inference = out.infer(GlyphKind::Vertical, (px, out.uv.1), inferred);
        out.guides.push(SnapGuide {
            line: [(px, py), (px, out.uv.1)],
            inference,
        });
    }

    // 6. The pending segment's H/V lock, at the commit's own angle
    // tolerance, so the preview never promises an H/V the line won't get;
    // where none applies, a relation to another line's direction.
    let mut ray = None;
    if let Some((u1, v1)) = query.direction_from {
        match lock_at((u1, v1), out.uv, tol) {
            Some(Inferred::Horizontal) => {
                out.uv.1 = v1;
                out.locked.1 = true;
                out.infer(GlyphKind::Horizontal, out.uv, Inferred::Horizontal);
            }
            Some(_) => {
                out.uv.0 = u1;
                out.locked.0 = true;
                out.infer(GlyphKind::Vertical, out.uv, Inferred::Vertical);
            }
            None => ray = super::relations::direction(sketch, query, &mut out),
        }
    }

    // 7. The sketch axes (U: v = 0, V: u = 0) — along a relation's ray, if
    // one placed the end, so the axis never bends it.
    if let Some(ray) = ray {
        ray.onto_axes(query, &mut out);
        out.prioritise();
        return out;
    }
    if out.uv.1.abs() <= tol {
        out.uv.1 = 0.0;
        out.locked.1 = true;
        out.infer(
            GlyphKind::AxisU,
            out.uv,
            Inferred::OnAxis(DatumEntity::AxisX),
        );
    }
    if out.uv.0.abs() <= tol {
        out.uv.0 = 0.0;
        out.locked.0 = true;
        out.infer(
            GlyphKind::AxisV,
            out.uv,
            Inferred::OnAxis(DatumEntity::AxisY),
        );
    }
    out.prioritise();
    out
}

/// The H/V lock a pending segment from `start` takes when it ends at `at`:
/// within the commit's own angle tolerance of level or plumb, and longer
/// than the snap radius. So the preview never promises an H/V the line
/// won't get.
pub(super) fn lock_at(start: (f64, f64), at: (f64, f64), tol: f64) -> Option<Inferred> {
    let dx = (at.0 - start.0).abs();
    let dy = (at.1 - start.1).abs();
    let dist = dx.hypot(dy);
    if dist <= tol {
        return None;
    }
    let sin = ANGLE_TOL.sin();
    if dy / dist < sin {
        Some(Inferred::Horizontal)
    } else if dx / dist < sin {
        Some(Inferred::Vertical)
    } else {
        None
    }
}
