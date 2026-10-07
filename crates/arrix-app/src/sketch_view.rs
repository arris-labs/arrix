//! The sketch overlay as the app builds it: a solved draft of the open
//! sketch turned into `arrix-ui`'s `SketchView` (plane-local metres), and
//! the transform from its plane to the viewport (docs/UI-RENDERING.md
//! §Sketch mode, §View types).

use arrix_core::{Frame, QuantityKind};
use arrix_sketch::{
    Constraint, ConstraintRecord, DatumEntity, Diagnostics, Entity, EntityConstraintState, Region,
    Sketch, SketchEntityId, SketchStatus, find_region_outlines,
};
use arrix_ui::{
    ConstraintGlyphView, CurveShape, DimensionView, DofState, PlaneToScreen, SketchCurveView,
    SketchPointView, SketchRegionView, SketchStatusView, SketchView, SolverState,
};
use arrix_viewport::OrbitCamera;

type Uv = [f64; 2];

/// How far a dimension's leader stands off what it measures, in metres,
/// when its record gives it no place.
const STAND_OFF: f64 = 0.006;

/// Coordinates are shown to a nanometre: a golden then does not move with
/// the last bits of a solve.
fn round(x: f64) -> f64 {
    (x * 1e9).round() / 1e9
}

fn uv(p: Uv) -> Uv {
    [round(p[0]), round(p[1])]
}

/// A sketch, solved, with the solver's verdict on it.
#[derive(Clone, Debug)]
pub struct Solved {
    /// The evaluation this was made from.
    pub generation: arrix_doc::Generation,
    /// Made by this client's own edit, submitted against this generation:
    /// only an evaluation newer than it replaces the draft, so the next
    /// click does not draw on a sketch that lacks the last one's work.
    pub awaiting: Option<arrix_doc::Generation>,
    pub sketch: Sketch,
    pub diagnostics: Diagnostics,
    /// The faces of the sketch's arrangement, largest first, with the
    /// boundaries they are shaded and tested by: what the evaluator's
    /// regions are for the same sketch. Kept by [`Self::refresh_regions`].
    pub regions: Vec<(Region, arrix_sketch::RegionOutline)>,
}

impl Solved {
    pub fn new(generation: arrix_doc::Generation, mut sketch: Sketch) -> Self {
        let (_, diagnostics) = Diagnostics::evaluate(&mut sketch);
        let regions = find_region_outlines(&sketch);
        Self {
            generation,
            awaiting: None,
            sketch,
            diagnostics,
            regions,
        }
    }

    /// The regions again, after the draft was moved in place (a drag frame).
    pub fn refresh_regions(&mut self) {
        self.regions = find_region_outlines(&self.sketch);
    }

    /// The innermost region `at` (plane-local metres) is in: the smallest
    /// that holds it, since a region inside a hole is a region of its own.
    pub fn region_at(&self, at: Uv) -> Option<&Region> {
        self.regions
            .iter()
            .filter(|(_, outline)| outline.contains(at))
            .min_by(|(a, _), (b, _)| {
                a.profile
                    .outer
                    .signed_area
                    .abs()
                    .total_cmp(&b.profile.outer.signed_area.abs())
            })
            .map(|(region, _)| region)
    }

    /// The draft an edit submitted against `base` left, until the
    /// evaluation of it arrives.
    pub fn local(base: arrix_doc::Generation, sketch: Sketch) -> Self {
        Self {
            awaiting: Some(base),
            ..Self::new(base, sketch)
        }
    }

    /// Whether the evaluation of `generation` replaces this.
    pub fn stale(&self, generation: arrix_doc::Generation) -> bool {
        match self.awaiting {
            Some(base) => generation > base,
            None => generation != self.generation,
        }
    }
}

/// What the viewport draws of the open sketch: the view and the plane it
/// stands on.
pub struct SketchOverlay {
    pub view: SketchView,
    pub plane: Frame,
    /// The select tool is in use: the pointer hovers and picks items. A
    /// drawing tool's pointer snaps instead.
    pub picks: bool,
}

/// The transform from `plane` to the viewport `rect` through `camera`.
pub fn plane_transform(
    camera: &OrbitCamera,
    rect: egui::Rect,
    plane: &Frame,
) -> Option<PlaneToScreen> {
    let aspect = rect.width().max(1.0) / rect.height().max(1.0);
    let view_proj = camera.view_proj(aspect);
    let clip = |p: arrix_core::DVec3, w: f32| {
        let c = view_proj * p.as_vec3().extend(w);
        [f64::from(c.x), f64::from(c.y), f64::from(c.w)]
    };
    PlaneToScreen::new(
        clip(plane.x_axis(), 0.0),
        clip(plane.y_axis(), 0.0),
        clip(plane.origin(), 1.0),
        rect,
    )
}

fn dof(state: EntityConstraintState) -> DofState {
    match state {
        EntityConstraintState::UnderConstrained => DofState::Under,
        EntityConstraintState::FullyConstrained => DofState::Fully,
        EntityConstraintState::Fixed => DofState::Fixed,
        EntityConstraintState::Conflicting => DofState::Conflicting,
    }
}

pub fn solver_state(diagnostics: &Diagnostics) -> SolverState {
    match diagnostics.status {
        SketchStatus::Ok => SolverState::UnderConstrained,
        SketchStatus::FullyConstrained => SolverState::FullyConstrained,
        SketchStatus::OverConstrained => SolverState::OverConstrained,
        SketchStatus::DidNotConverge => SolverState::DidNotConverge,
    }
}

/// The view of a solved sketch, with values shown in `length_unit` (`mm`)
/// and `angle_unit` (`deg`).
pub fn sketch_view(solved: &Solved) -> SketchView {
    let (sketch, diagnostics) = (&solved.sketch, &solved.diagnostics);
    let points = sketch
        .points()
        .iter()
        .map(|(id, p)| SketchPointView {
            id: id.to_string(),
            at: uv(p.pos()),
            state: dof(diagnostics.point_state(*id)),
        })
        .collect();
    let curves = sketch
        .entities()
        .iter()
        .filter_map(|(id, entity)| {
            Some(SketchCurveView {
                id: id.to_string(),
                shape: shape(sketch, entity)?,
                construction: sketch.is_construction(*id),
                state: dof(diagnostics.entity_state(*id)),
            })
        })
        .collect();
    let mut glyphs = Vec::new();
    let mut dimensions = Vec::new();
    // Leaders stand off on the side away from the middle of the points.
    let centre = middle(sketch);
    for (id, record) in sketch.constraints() {
        let flagged = diagnostics.is_redundant(*id) || diagnostics.is_conflicting(*id);
        if record.constraint.is_dimensional() {
            dimensions.extend(dimension(sketch, record, flagged, centre));
        } else {
            glyphs.extend(glyph(sketch, record, flagged));
        }
    }
    SketchView {
        points,
        curves,
        glyphs,
        dimensions,
        status: SketchStatusView {
            dof: diagnostics.dof,
            state: solver_state(diagnostics),
        },
        hover: None,
        selected: Vec::new(),
        preview: None,
        field: None,
        regions: solved
            .regions
            .iter()
            .map(|(region, outline)| SketchRegionView {
                key: region_key(&region.key),
                holes: outline.holes.len(),
                outer: outline.outer.iter().copied().map(uv).collect(),
                hole_rings: outline
                    .holes
                    .iter()
                    .map(|ring| ring.iter().copied().map(uv).collect())
                    .collect(),
            })
            .collect(),
        hover_region: None,
    }
}

/// A region's key as the document stores it: its JSON.
pub fn region_key(key: &arrix_core::RegionKey) -> String {
    serde_json::to_string(key).unwrap_or_default()
}

fn pos(sketch: &Sketch, id: SketchEntityId) -> Option<Uv> {
    sketch.point(id).map(arrix_sketch::Point::pos)
}

#[allow(
    unreachable_patterns,
    reason = "the conic entities exist with `conics`"
)]
fn shape(sketch: &Sketch, entity: &Entity) -> Option<CurveShape> {
    match *entity {
        Entity::Line { start, end } => Some(CurveShape::Line {
            from: uv(pos(sketch, start)?),
            to: uv(pos(sketch, end)?),
        }),
        Entity::Circle { center, radius } => Some(CurveShape::Circle {
            centre: uv(pos(sketch, center)?),
            radius: round(radius),
        }),
        Entity::Arc { center, start, end } => {
            let (c, s, e) = (pos(sketch, center)?, pos(sketch, start)?, pos(sketch, end)?);
            Some(CurveShape::Arc {
                centre: uv(c),
                radius: round((s[0] - c[0]).hypot(s[1] - c[1])),
                start: round((s[1] - c[1]).atan2(s[0] - c[0])),
                end: round((e[1] - c[1]).atan2(e[0] - c[0])),
            })
        }
        // Lone points are drawn as points; conics are gated.
        _ => None,
    }
}

/// A representative place on an entity: a line's middle, a circle's rim at
/// 45°, an arc's middle, a lone point.
#[allow(
    unreachable_patterns,
    reason = "the conic entities exist with `conics`"
)]
fn anchor(sketch: &Sketch, id: SketchEntityId) -> Option<Uv> {
    match *sketch.entity(id)? {
        Entity::Line { start, end } => {
            let (a, b) = (pos(sketch, start)?, pos(sketch, end)?);
            Some(mid(a, b))
        }
        Entity::Circle { center, radius } => {
            let c = pos(sketch, center)?;
            let d = radius * std::f64::consts::FRAC_1_SQRT_2;
            Some([c[0] + d, c[1] + d])
        }
        Entity::Arc { center, start, end } => {
            let (c, s, e) = (pos(sketch, center)?, pos(sketch, start)?, pos(sketch, end)?);
            let r = (s[0] - c[0]).hypot(s[1] - c[1]);
            let a0 = (s[1] - c[1]).atan2(s[0] - c[0]);
            let mut sweep = (e[1] - c[1]).atan2(e[0] - c[0]) - a0;
            while sweep <= 0.0 {
                sweep += std::f64::consts::TAU;
            }
            let a = a0 + sweep / 2.0;
            Some([c[0] + r * a.cos(), c[1] + r * a.sin()])
        }
        Entity::Point(p) => pos(sketch, p),
        _ => None,
    }
}

fn mid(a: Uv, b: Uv) -> Uv {
    [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
}

/// The constraint's kind as the document names it, `horizontal` or
/// `point_on_line`.
fn kind(constraint: &Constraint) -> String {
    serde_json::to_value(constraint)
        .ok()
        .and_then(|v| v.as_object().and_then(|o| o.keys().next().cloned()))
        .unwrap_or_default()
}

/// What a constraint's mark says: a letter or a sign, ASCII for the font.
fn symbol(constraint: &Constraint) -> &'static str {
    match constraint {
        Constraint::Coincident { .. } => "C",
        Constraint::Horizontal { .. } | Constraint::HorizontalPoints { .. } => "H",
        Constraint::Vertical { .. } | Constraint::VerticalPoints { .. } => "V",
        Constraint::Parallel { .. } => "||",
        Constraint::Perpendicular { .. } => "_|",
        Constraint::Equal { .. } => "=",
        Constraint::Concentric { .. } => "co",
        Constraint::Tangent { .. } | Constraint::TangentCircles { .. } => "T",
        Constraint::Symmetric { .. } | Constraint::SymmetricPoints { .. } => "S",
        Constraint::PointOnLine { .. } | Constraint::PointOnCircle { .. } => "on",
        Constraint::Midpoint { .. } => "M",
        Constraint::Fix { .. } => "F",
        Constraint::Block { .. } => "B",
        Constraint::PointOnPerpBisector { .. } => "pb",
        Constraint::PointOnDatum { .. } | Constraint::CoincidentToDatum { .. } => "O",
        Constraint::SymmetricAcrossDatum { .. } => "S",
        _ => "?",
    }
}

/// A mark beside each entity a geometric constraint holds, or, when it
/// holds points only, beside the first (the middle of two aligned ones).
fn glyph(sketch: &Sketch, record: &ConstraintRecord, flagged: bool) -> Vec<ConstraintGlyphView> {
    let refs = record.constraint.refs();
    let mut places: Vec<Uv> = refs
        .entities
        .iter()
        .filter_map(|e| anchor(sketch, *e))
        .collect();
    if places.is_empty() {
        let points: Vec<Uv> = refs.points.iter().filter_map(|p| pos(sketch, *p)).collect();
        places.extend(match points.as_slice() {
            [a, b, ..]
                if matches!(
                    record.constraint,
                    Constraint::HorizontalPoints { .. } | Constraint::VerticalPoints { .. }
                ) =>
            {
                Some(mid(*a, *b))
            }
            [a, ..] => Some(*a),
            [] => None,
        });
    }
    places
        .into_iter()
        .map(|at| ConstraintGlyphView {
            id: record.id.to_string(),
            kind: kind(&record.constraint),
            symbol: symbol(&record.constraint).to_owned(),
            at: uv(at),
            flagged,
        })
        .collect()
}

/// A value as shown: millimetres, or degrees.
pub fn format_quantity(kind: QuantityKind, value: f64) -> String {
    let (shown, unit) = match kind {
        QuantityKind::Angle => (value.to_degrees(), "deg"),
        _ => (value * 1e3, "mm"),
    };
    let text = format!("{shown:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    format!("{text} {unit}")
}

/// A dimension's leader and label place.
fn dimension(
    sketch: &Sketch,
    record: &ConstraintRecord,
    flagged: bool,
    centre: Uv,
) -> Option<DimensionView> {
    let refs = record.constraint.refs();
    let (value_kind, value) = refs.value?;
    let p = |i: usize| refs.points.get(i).and_then(|id| pos(sketch, *id));
    let a = |i: usize| refs.entities.get(i).and_then(|id| anchor(sketch, *id));
    let centre_of = |i: usize| {
        let id = *refs.entities.get(i)?;
        match *sketch.entity(id)? {
            Entity::Circle { center, .. } | Entity::Arc { center, .. } => pos(sketch, center),
            _ => None,
        }
    };
    let line_ends = |i: usize| {
        let id = *refs.entities.get(i)?;
        let (s, e) = sketch.entity(id)?.line_ends()?;
        Some((pos(sketch, s)?, pos(sketch, e)?))
    };
    // Where the leader runs, as its two ends.
    let (from, to): (Uv, Uv) = match record.constraint {
        Constraint::Distance { .. } => {
            let (s, e) = (p(0)?, p(1)?);
            offset_segment(s, e, centre)
        }
        Constraint::HorizontalDistance { .. } => {
            let (s, e) = (p(0)?, p(1)?);
            let outward = mid(s, e)[1] >= centre[1];
            let y = if outward {
                s[1].max(e[1]) + STAND_OFF
            } else {
                s[1].min(e[1]) - STAND_OFF
            };
            ([s[0], y], [e[0], y])
        }
        Constraint::VerticalDistance { .. } => {
            let (s, e) = (p(0)?, p(1)?);
            let outward = mid(s, e)[0] >= centre[0];
            let x = if outward {
                s[0].max(e[0]) + STAND_OFF
            } else {
                s[0].min(e[0]) - STAND_OFF
            };
            ([x, s[1]], [x, e[1]])
        }
        Constraint::DistancePointLine { .. } => {
            let point = p(0)?;
            let (s, e) = line_ends(0)?;
            (point, foot(point, s, e))
        }
        Constraint::DistanceParallelLines { .. } => {
            let (s0, e0) = line_ends(0)?;
            let (s1, e1) = line_ends(1)?;
            let m = mid(s0, e0);
            (m, foot(m, s1, e1))
        }
        Constraint::Radius { .. } | Constraint::Diameter { .. } => {
            let c = centre_of(0)?;
            let rim = a(0)?;
            if matches!(record.constraint, Constraint::Diameter { .. }) {
                ([2.0 * c[0] - rim[0], 2.0 * c[1] - rim[1]], rim)
            } else {
                (c, rim)
            }
        }
        Constraint::Angle { .. } => (a(0)?, a(1)?),
        Constraint::AnglePoints { .. } => (p(0)?, p(2)?),
        Constraint::ArcLength { .. } => {
            let id = *refs.entities.first()?;
            let (s, e) = sketch.entity(id)?.arc_ends()?;
            (pos(sketch, s)?, pos(sketch, e)?)
        }
        Constraint::DistanceToAxisX { .. } => {
            let q = p(0)?;
            (q, [0.0, q[1]])
        }
        Constraint::DistanceToAxisY { .. } => {
            let q = p(0)?;
            (q, [q[0], 0.0])
        }
        Constraint::DistanceToDatum { datum, .. } => {
            let q = p(0)?;
            (
                q,
                match datum {
                    DatumEntity::Origin => [0.0, 0.0],
                    DatumEntity::AxisX => [q[0], 0.0],
                    DatumEntity::AxisY => [0.0, q[1]],
                },
            )
        }
        Constraint::AngleWithDatum { .. } => (a(0)?, [0.0, 0.0]),
        Constraint::DistanceCircleCircle { .. } => (centre_of(0)?, centre_of(1)?),
        Constraint::DistancePointCircle { .. } => (p(0)?, centre_of(0)?),
        _ => return None,
    };
    let at = record
        .display_pos
        .map_or_else(|| mid(from, to), |(u, v)| [u, v]);
    let measured = format_quantity(value_kind, value);
    let text = match &record.expr {
        Some(expr) => format!("{expr} = {measured}"),
        None => measured,
    };
    Some(DimensionView {
        id: record.id.to_string(),
        kind: kind(&record.constraint),
        text,
        from: uv(from),
        to: uv(to),
        at: uv(at),
        reference: record.is_reference(),
        flagged,
    })
}

/// The segment `a`-`b` moved off by the stand-off, to the side away from
/// `centre`.
fn offset_segment(a: Uv, b: Uv, centre: Uv) -> (Uv, Uv) {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = dx.hypot(dy).max(1e-12);
    let mut n = [-dy / len * STAND_OFF, dx / len * STAND_OFF];
    let m = mid(a, b);
    if n[0] * (m[0] - centre[0]) + n[1] * (m[1] - centre[1]) < 0.0 {
        n = [-n[0], -n[1]];
    }
    ([a[0] + n[0], a[1] + n[1]], [b[0] + n[0], b[1] + n[1]])
}

/// What a hit is, for the status line: `Line`, `Point`, `Horizontal
/// constraint`, `Distance dimension (6 mm)`.
pub fn describe(solved: &Solved, hit: &arrix_ui::SketchHit) -> String {
    use arrix_ui::HitKind;
    let sketch = &solved.sketch;
    let Ok(id) = hit.id.parse::<SketchEntityId>() else {
        return "Item".into();
    };
    let words = |name: &str| {
        let mut chars = name.replace('_', " ");
        if let Some(first) = chars.get_mut(..1) {
            first.make_ascii_uppercase();
        }
        chars
    };
    match hit.kind {
        HitKind::Point => "Point".into(),
        HitKind::Curve => match sketch.entity(id) {
            Some(Entity::Line { .. }) if sketch.is_construction(id) => "Construction line".into(),
            Some(Entity::Line { .. }) => "Line".into(),
            Some(Entity::Circle { .. }) => "Circle".into(),
            Some(Entity::Arc { .. }) => "Arc".into(),
            _ => "Curve".into(),
        },
        HitKind::Glyph => {
            let kind = sketch.get_constraint(id).map(kind).unwrap_or_default();
            format!("{} constraint", words(&kind))
        }
        HitKind::Dimension => {
            let (kind, text) = sketch
                .get_constraint_record(id)
                .map(|r| (kind(&r.constraint), r.constraint.refs().value))
                .unzip();
            let value = text
                .flatten()
                .map(|(k, v)| format!(" ({})", format_quantity(k, v)))
                .unwrap_or_default();
            format!("{} dimension{value}", words(&kind.unwrap_or_default()))
        }
    }
}

/// The foot of the perpendicular from `p` on the infinite line `a`-`b`.
fn foot(p: Uv, a: Uv, b: Uv) -> Uv {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = (dx * dx + dy * dy).max(1e-24);
    let t = ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2;
    [a[0] + t * dx, a[1] + t * dy]
}

/// The middle of the sketch's points' bounding box, the origin for none.
fn middle(sketch: &Sketch) -> Uv {
    let mut points = sketch.points().values().map(arrix_sketch::Point::pos);
    let Some(first) = points.next() else {
        return [0.0, 0.0];
    };
    let (lo, hi) = points.fold((first, first), |(lo, hi), p| {
        (
            [lo[0].min(p[0]), lo[1].min(p[1])],
            [hi[0].max(p[0]), hi[1].max(p[1])],
        )
    });
    mid(lo, hi)
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrix_core::Id;
    use arrix_sketch::Point;

    fn id(n: u64) -> SketchEntityId {
        SketchEntityId(Id(n))
    }

    fn view(sketch: Sketch) -> SketchView {
        sketch_view(&Solved::new(arrix_doc::Generation(0), sketch))
    }

    /// A line from the origin along X, with its ends.
    fn line() -> Sketch {
        let mut sketch = Sketch::new();
        sketch.insert_point(id(1), Point::fixed(0.0, 0.0)).unwrap();
        sketch.insert_point(id(2), Point::new(0.02, 0.0)).unwrap();
        sketch
            .insert_entity(
                id(3),
                Entity::Line {
                    start: id(1),
                    end: id(2),
                },
            )
            .unwrap();
        sketch
    }

    fn record(n: u64, constraint: Constraint) -> ConstraintRecord {
        ConstraintRecord::new(id(n), constraint)
    }

    #[test]
    fn a_free_line_is_under_constrained_with_its_dof_in_the_status() {
        let v = view(line());
        assert_eq!(v.status.state, SolverState::UnderConstrained);
        assert_eq!(v.status.dof, 2, "the free end has two");
        assert_eq!(v.points.len(), 2);
        assert_eq!(v.points[0].state, DofState::Fixed);
        assert_eq!(v.points[1].state, DofState::Under);
        assert!(v.glyphs.is_empty() && v.dimensions.is_empty());
    }

    #[test]
    fn constraints_are_glyphs_and_dimensions_are_labels_with_leaders() {
        let mut sketch = line();
        sketch
            .insert_constraint(record(4, Constraint::Horizontal { line: id(3) }))
            .unwrap();
        let mut length = record(
            5,
            Constraint::Distance {
                a: id(1),
                b: id(2),
                value: 0.02,
            },
        );
        length.expr = Some("width".into());
        sketch.insert_constraint(length).unwrap();
        let v = view(sketch);
        assert_eq!(v.status.state, SolverState::FullyConstrained);
        assert_eq!(v.glyphs.len(), 1);
        assert_eq!(
            (v.glyphs[0].kind.as_str(), v.glyphs[0].symbol.as_str()),
            ("horizontal", "H")
        );
        assert_eq!(v.glyphs[0].at, [0.01, 0.0], "the middle of the line");
        let d = &v.dimensions[0];
        assert_eq!(
            (d.kind.as_str(), d.text.as_str()),
            ("distance", "width = 20 mm")
        );
        assert_eq!(d.from[0], 0.0);
        assert_eq!(d.to[0], 0.02);
        assert_eq!(d.from[1], d.to[1], "the leader runs parallel to the line");
        assert!(d.from[1].abs() > 1e-9, "and stands off it");
    }

    #[test]
    fn a_circle_an_arc_and_a_construction_line_keep_their_shapes() {
        let mut sketch = Sketch::new();
        sketch.insert_point(id(1), Point::new(0.0, 0.0)).unwrap();
        sketch.insert_point(id(2), Point::new(0.01, 0.0)).unwrap();
        sketch.insert_point(id(3), Point::new(0.0, 0.01)).unwrap();
        sketch
            .insert_entity(
                id(4),
                Entity::Circle {
                    center: id(1),
                    radius: 0.005,
                },
            )
            .unwrap();
        sketch
            .insert_entity(
                id(5),
                Entity::Arc {
                    center: id(1),
                    start: id(2),
                    end: id(3),
                },
            )
            .unwrap();
        sketch
            .insert_entity(
                id(6),
                Entity::Line {
                    start: id(2),
                    end: id(3),
                },
            )
            .unwrap();
        sketch.set_construction(id(6), true);
        sketch
            .insert_constraint(record(
                7,
                Constraint::Radius {
                    target: id(4),
                    value: 0.005,
                },
            ))
            .unwrap();
        sketch
            .insert_constraint(record(
                8,
                Constraint::Angle {
                    a: id(6),
                    b: id(6),
                    value: std::f64::consts::FRAC_PI_4,
                },
            ))
            .unwrap();
        let v = view(sketch);
        let shapes: Vec<_> = v
            .curves
            .iter()
            .map(|c| (&c.shape, c.construction))
            .collect();
        assert!(
            matches!(shapes[0], (CurveShape::Circle { radius, .. }, false) if *radius == 0.005)
        );
        assert!(matches!(
            shapes[1],
            (CurveShape::Arc { start, end, radius, .. }, false)
                if *start == 0.0 && (end - std::f64::consts::FRAC_PI_2).abs() < 1e-9 && *radius == 0.01
        ));
        assert!(matches!(shapes[2], (CurveShape::Line { .. }, true)));
        let texts: Vec<_> = v.dimensions.iter().map(|d| d.text.as_str()).collect();
        assert!(
            texts.contains(&"5 mm") && texts.contains(&"45 deg"),
            "{texts:?}"
        );
    }

    #[test]
    fn contradictory_dimensions_are_over_constrained_and_flagged() {
        let mut sketch = line();
        for (n, value) in [(4, 0.02), (5, 0.03)] {
            sketch
                .insert_constraint(record(
                    n,
                    Constraint::Distance {
                        a: id(1),
                        b: id(2),
                        value,
                    },
                ))
                .unwrap();
        }
        let v = view(sketch);
        assert_eq!(v.status.state, SolverState::OverConstrained);
        assert!(v.dimensions.iter().any(|d| d.flagged));
        assert!(v.points.iter().any(|p| p.state == DofState::Conflicting));
    }
}
