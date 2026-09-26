//! Geometric and dimensional constraints (docs/DATA-MODEL.md §Sketches).
//! Values are SI, metres and radians; a unit is only how a value is typed
//! or shown. The enabled kinds are the ones M3's sketch mode reaches; the
//! conic kinds sit behind `conics` and `SnellsLaw` behind `snells-law`,
//! and a default build does not read them.

use serde::{Deserialize, Serialize};

use arrix_core::QuantityKind;

use crate::ids::{ConstraintId, EntityId, PointId};

/// One constraint in the residual system. Dimensional values are SI
/// (meters / radians) — the UI converts from mm/deg on the way in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Constraint {
    /// `a` and `b` share coordinates (2 residuals).
    Coincident { a: PointId, b: PointId },
    /// Line is horizontal: dy == 0 (1 residual).
    Horizontal { line: EntityId },
    /// Line is vertical: dx == 0 (1 residual).
    Vertical { line: EntityId },
    /// Point-to-point horizontal alignment: pa.y == pb.y (1 residual).
    HorizontalPoints { a: PointId, b: PointId },
    /// Point-to-point vertical alignment: pa.x == pb.x (1 residual).
    VerticalPoints { a: PointId, b: PointId },
    /// Two lines are parallel: cross(dir_a, dir_b) == 0 (1 residual).
    Parallel { a: EntityId, b: EntityId },
    /// Two lines are perpendicular: dot(dir_a, dir_b) == 0 (1 residual).
    Perpendicular { a: EntityId, b: EntityId },
    /// Equal length between two lines, or equal radius between two circles/arcs (1 residual).
    Equal { a: EntityId, b: EntityId },
    /// Two circles/arcs share center: center_a == center_b (2 residuals).
    Concentric { a: EntityId, b: EntityId },
    /// Line tangent to circle/arc: distance(center, line) == radius (1 residual).
    Tangent { line: EntityId, circle: EntityId },
    /// Two circles/arcs are tangent: dist(c_a, c_b) == r_a + r_b or |r_a - r_b| (1 residual).
    TangentCircles { a: EntityId, b: EntityId },
    /// `a` and `b` are mirror images across `mirror` (a line entity) (2 residuals).
    Symmetric {
        a: PointId,
        b: PointId,
        mirror: EntityId,
    },
    /// Distance between two points (1 residual).
    Distance { a: PointId, b: PointId, value: f64 },
    /// Horizontal distance between two points: |pa.x - pb.x| == value (1 residual).
    HorizontalDistance { a: PointId, b: PointId, value: f64 },
    /// Vertical distance between two points: |pa.y - pb.y| == value (1 residual).
    VerticalDistance { a: PointId, b: PointId, value: f64 },
    /// Perpendicular distance from a point to a line (1 residual).
    DistancePointLine {
        point: PointId,
        line: EntityId,
        value: f64,
    },
    /// Perpendicular distance between two parallel lines (1 residual).
    DistanceParallelLines {
        a: EntityId,
        b: EntityId,
        value: f64,
    },
    /// Angle between two lines, in radians (1 residual).
    Angle {
        a: EntityId,
        b: EntityId,
        value: f64,
    },
    /// Circle (or arc) radius in meters (1 residual).
    Radius { target: EntityId, value: f64 },
    /// Circle (or arc) diameter in meters: 2 * radius == value (1 residual).
    Diameter { target: EntityId, value: f64 },
    /// Point lies on the infinite line through a line entity (1 residual).
    PointOnLine { point: PointId, line: EntityId },
    /// Point lies on a circle/arc: distance(center, point) == radius (1 residual).
    PointOnCircle { point: PointId, circle: EntityId },
    /// Point is the midpoint of a line (2 residuals).
    Midpoint { point: PointId, line: EntityId },
    /// Soft/hard pin used during drag and for construction anchors that
    /// aren't marked `Point::fixed`. Two residuals.
    Fix { point: PointId, x: f64, y: f64 },
    /// Central symmetry: `a` and `b` symmetric about `center` (2 residuals: pa + pb - 2*pc == 0).
    SymmetricPoints {
        a: PointId,
        b: PointId,
        center: PointId,
    },
    /// Point lies on the perpendicular bisector of segment `a`-`b` (1 residual: dist(p,a)^2 - dist(p,b)^2 == 0).
    PointOnPerpBisector {
        point: PointId,
        a: PointId,
        b: PointId,
    },
    /// Arc length dimension: s = R * theta (1 residual).
    ArcLength { arc: EntityId, value: f64 },
    /// Block/lock all degrees of freedom of an entity relative to sketch coordinates (2, 3, 4, or 6 residuals).
    Block { entity: EntityId },
    /// Angle between 3 points with vertex at `vertex` (1 residual).
    AnglePoints {
        a: PointId,
        vertex: PointId,
        b: PointId,
        value: f64,
    },
    /// Distance from point to datum Vertical Axis X=0 (1 residual: |p.x| == value).
    DistanceToAxisX { point: PointId, value: f64 },
    /// Distance from point to datum Horizontal Axis Y=0 (1 residual: |p.y| == value).
    DistanceToAxisY { point: PointId, value: f64 },
    /// Perimeter clearance distance between two circles/arcs (1 residual).
    DistanceCircleCircle {
        a: EntityId,
        b: EntityId,
        value: f64,
    },
    /// Clearance distance from a point to a circle/arc perimeter (1 residual).
    DistancePointCircle {
        point: PointId,
        circle: EntityId,
        value: f64,
    },
    /// Snell's law of refraction across boundary: sin(theta1) - ratio * sin(theta2) == 0 (1 residual).
    #[cfg(feature = "snells-law")]
    SnellsLaw {
        ray1_start: PointId,
        ray1_end: PointId,
        ray2_end: PointId,
        boundary: EntityId,
        ratio: f64,
    },
    /// Point lies on datum (Origin: (0,0), AxisX: y=0, AxisY: x=0).
    PointOnDatum { point: PointId, datum: DatumEntity },
    /// Perpendicular distance from a point to a datum axis (or Euclidean distance to origin).
    DistanceToDatum {
        point: PointId,
        datum: DatumEntity,
        value: f64,
    },
    /// Angle between a line entity and a datum axis (AxisX: horizontal, AxisY: vertical), in radians.
    AngleWithDatum {
        line: EntityId,
        datum: DatumEntity,
        value: f64,
    },
    /// Points `a` and `b` are symmetric across a datum axis (or point-symmetric about origin).
    SymmetricAcrossDatum {
        a: PointId,
        b: PointId,
        datum: DatumEntity,
    },
    /// Point is coincident to datum (for Origin: 2 residuals (x=0, y=0); for AxisX: y=0; for AxisY: x=0).
    CoincidentToDatum { point: PointId, datum: DatumEntity },
    /// Point lies on an ellipse or elliptic arc (1 residual).
    #[cfg(feature = "conics")]
    PointOnEllipse { point: PointId, ellipse: EntityId },
    /// Line is tangent to an ellipse (1 residual).
    #[cfg(feature = "conics")]
    TangentLineEllipse { line: EntityId, ellipse: EntityId },
    /// Point aligns with an internal geometric feature of an ellipse (focus or axis endpoint) (2 residuals).
    #[cfg(feature = "conics")]
    InternalAlignment {
        ellipse: EntityId,
        alignment: AlignmentKind,
    },
    /// Minor radius of an ellipse in meters (1 residual).
    #[cfg(feature = "conics")]
    MinorRadius { ellipse: EntityId, value: f64 },
    /// Major radius of an ellipse in meters (1 residual).
    #[cfg(feature = "conics")]
    MajorRadius { ellipse: EntityId, value: f64 },
    /// Point lies on a B-spline curve at knot parameter u in [0, 1] (2 residuals).
    #[cfg(feature = "conics")]
    PointOnBSpline {
        point: PointId,
        bspline: EntityId,
        u: f64,
    },
    /// B-spline tangent at parameter u in [0, 1] is parallel to a line (1 residual).
    #[cfg(feature = "conics")]
    BSplineTangent {
        bspline: EntityId,
        u: f64,
        line: EntityId,
    },
    /// Signed/unsigned curvature of a B-spline curve at parameter u in [0, 1] (1 residual).
    #[cfg(feature = "conics")]
    BSplineCurvature {
        bspline: EntityId,
        u: f64,
        value: f64,
    },
}

/// Internal geometric feature alignment target for ellipses (matching FreeCAD `ConstraintInternalAlignment`).
#[cfg(feature = "conics")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlignmentKind {
    /// First focus point $F_1 = C + \sqrt{a^2 - b^2} \hat{u}$.
    Focus1(PointId),
    /// Second focus point $F_2 = C - \sqrt{a^2 - b^2} \hat{u}$.
    Focus2(PointId),
    /// Minor axis endpoint $C + b \hat{v}$.
    MinorRadiusEnd(PointId),
    /// Major axis endpoint $M = C + a \hat{u}$.
    MajorRadiusEnd(PointId),
}

#[cfg(feature = "conics")]
impl AlignmentKind {
    /// The sketch point this alignment pins to the ellipse's internal
    /// feature — every variant carries exactly one.
    pub fn point(&self) -> PointId {
        match *self {
            Self::Focus1(p)
            | Self::Focus2(p)
            | Self::MinorRadiusEnd(p)
            | Self::MajorRadiusEnd(p) => p,
        }
    }
}

/// Built-in coordinate datum entity in the sketch's local 2D UV coordinate system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatumEntity {
    /// Origin point at (0, 0).
    Origin,
    /// Horizontal axis along Y = 0 (U axis, direction (1, 0)).
    AxisX,
    /// Vertical axis along X = 0 (V axis, direction (0, 1)).
    AxisY,
}

impl Constraint {
    pub fn point_on_datum(point: PointId, datum: DatumEntity) -> Self {
        Self::PointOnDatum { point, datum }
    }

    pub fn distance_to_datum(point: PointId, datum: DatumEntity, value: f64) -> Self {
        Self::DistanceToDatum {
            point,
            datum,
            value,
        }
    }

    pub fn angle_with_datum(line: EntityId, datum: DatumEntity, value: f64) -> Self {
        Self::AngleWithDatum { line, datum, value }
    }

    pub fn symmetric_across_datum(a: PointId, b: PointId, datum: DatumEntity) -> Self {
        Self::SymmetricAcrossDatum { a, b, datum }
    }

    pub fn coincident_to_datum(point: PointId, datum: DatumEntity) -> Self {
        Self::CoincidentToDatum { point, datum }
    }

    pub fn symmetric_points(a: PointId, b: PointId, center: PointId) -> Self {
        Self::SymmetricPoints { a, b, center }
    }

    pub fn point_on_perp_bisector(point: PointId, a: PointId, b: PointId) -> Self {
        Self::PointOnPerpBisector { point, a, b }
    }

    pub fn arc_length(arc: EntityId, value: f64) -> Self {
        Self::ArcLength { arc, value }
    }

    pub fn block(entity: EntityId) -> Self {
        Self::Block { entity }
    }

    pub fn angle_points(a: PointId, vertex: PointId, b: PointId, value: f64) -> Self {
        Self::AnglePoints {
            a,
            vertex,
            b,
            value,
        }
    }

    pub fn distance_to_axis_x(point: PointId, value: f64) -> Self {
        Self::DistanceToAxisX { point, value }
    }

    pub fn distance_to_axis_y(point: PointId, value: f64) -> Self {
        Self::DistanceToAxisY { point, value }
    }

    pub fn distance_circle_circle(a: EntityId, b: EntityId, value: f64) -> Self {
        Self::DistanceCircleCircle { a, b, value }
    }

    pub fn distance_point_circle(point: PointId, circle: EntityId, value: f64) -> Self {
        Self::DistancePointCircle {
            point,
            circle,
            value,
        }
    }

    #[cfg(feature = "snells-law")]
    pub fn snells_law(
        ray1_start: PointId,
        ray1_end: PointId,
        ray2_end: PointId,
        boundary: EntityId,
        ratio: f64,
    ) -> Self {
        Self::SnellsLaw {
            ray1_start,
            ray1_end,
            ray2_end,
            boundary,
            ratio,
        }
    }

    #[cfg(feature = "conics")]
    pub fn point_on_ellipse(point: PointId, ellipse: EntityId) -> Self {
        Self::PointOnEllipse { point, ellipse }
    }

    #[cfg(feature = "conics")]
    pub fn tangent_line_ellipse(line: EntityId, ellipse: EntityId) -> Self {
        Self::TangentLineEllipse { line, ellipse }
    }

    #[cfg(feature = "conics")]
    pub fn internal_alignment(ellipse: EntityId, alignment: AlignmentKind) -> Self {
        Self::InternalAlignment { ellipse, alignment }
    }

    #[cfg(feature = "conics")]
    pub fn internal_alignment_focus1(ellipse: EntityId, point: PointId) -> Self {
        Self::InternalAlignment {
            ellipse,
            alignment: AlignmentKind::Focus1(point),
        }
    }

    #[cfg(feature = "conics")]
    pub fn internal_alignment_focus2(ellipse: EntityId, point: PointId) -> Self {
        Self::InternalAlignment {
            ellipse,
            alignment: AlignmentKind::Focus2(point),
        }
    }

    #[cfg(feature = "conics")]
    pub fn internal_alignment_minor_axis(ellipse: EntityId, point: PointId) -> Self {
        Self::InternalAlignment {
            ellipse,
            alignment: AlignmentKind::MinorRadiusEnd(point),
        }
    }

    #[cfg(feature = "conics")]
    pub fn internal_alignment_major_axis(ellipse: EntityId, point: PointId) -> Self {
        Self::InternalAlignment {
            ellipse,
            alignment: AlignmentKind::MajorRadiusEnd(point),
        }
    }

    #[cfg(feature = "conics")]
    pub fn minor_radius(ellipse: EntityId, value: f64) -> Self {
        Self::MinorRadius { ellipse, value }
    }

    #[cfg(feature = "conics")]
    pub fn major_radius(ellipse: EntityId, value: f64) -> Self {
        Self::MajorRadius { ellipse, value }
    }

    #[cfg(feature = "conics")]
    pub fn point_on_bspline(point: PointId, bspline: EntityId, u: f64) -> Self {
        Self::PointOnBSpline { point, bspline, u }
    }

    #[cfg(feature = "conics")]
    pub fn bspline_tangent(bspline: EntityId, u: f64, line: EntityId) -> Self {
        Self::BSplineTangent { bspline, u, line }
    }

    #[cfg(feature = "conics")]
    pub fn bspline_curvature(bspline: EntityId, u: f64, value: f64) -> Self {
        Self::BSplineCurvature { bspline, u, value }
    }

    /// How many scalar residuals this constraint contributes.
    pub fn residual_count(&self) -> usize {
        match self {
            Self::Coincident { .. }
            | Self::Concentric { .. }
            | Self::Fix { .. }
            | Self::Midpoint { .. }
            | Self::Symmetric { .. }
            | Self::SymmetricPoints { .. }
            | Self::SymmetricAcrossDatum { .. } => 2,
            #[cfg(feature = "conics")]
            Self::InternalAlignment { .. } | Self::PointOnBSpline { .. } => 2,
            Self::PointOnDatum { datum, .. } | Self::CoincidentToDatum { datum, .. } => match datum
            {
                DatumEntity::Origin => 2,
                DatumEntity::AxisX | DatumEntity::AxisY => 1,
            },
            Self::Block { .. } => 4,
            Self::Horizontal { .. }
            | Self::Vertical { .. }
            | Self::HorizontalPoints { .. }
            | Self::VerticalPoints { .. }
            | Self::Parallel { .. }
            | Self::Perpendicular { .. }
            | Self::Equal { .. }
            | Self::Tangent { .. }
            | Self::TangentCircles { .. }
            | Self::Distance { .. }
            | Self::HorizontalDistance { .. }
            | Self::VerticalDistance { .. }
            | Self::DistancePointLine { .. }
            | Self::DistanceParallelLines { .. }
            | Self::Angle { .. }
            | Self::Radius { .. }
            | Self::Diameter { .. }
            | Self::PointOnLine { .. }
            | Self::PointOnCircle { .. }
            | Self::PointOnPerpBisector { .. }
            | Self::ArcLength { .. }
            | Self::AnglePoints { .. }
            | Self::DistanceToAxisX { .. }
            | Self::DistanceToAxisY { .. }
            | Self::DistanceCircleCircle { .. }
            | Self::DistancePointCircle { .. }
            | Self::DistanceToDatum { .. }
            | Self::AngleWithDatum { .. } => 1,
            #[cfg(feature = "snells-law")]
            Self::SnellsLaw { .. } => 1,
            #[cfg(feature = "conics")]
            Self::PointOnEllipse { .. }
            | Self::TangentLineEllipse { .. }
            | Self::MinorRadius { .. }
            | Self::MajorRadius { .. }
            | Self::BSplineTangent { .. }
            | Self::BSplineCurvature { .. } => 1,
        }
    }

    /// All point ids directly referenced by this constraint, in the order
    /// the constraint names them.
    pub fn referenced_points(&self) -> Vec<PointId> {
        self.refs().points
    }

    /// All entity ids directly referenced by this constraint, in the order
    /// the constraint names them.
    pub fn referenced_entities(&self) -> Vec<EntityId> {
        self.refs().entities
    }

    pub fn uses_point(&self, point: PointId) -> bool {
        self.referenced_points().contains(&point)
    }

    pub fn uses_entity(&self, entity: EntityId) -> bool {
        self.referenced_entities().contains(&entity)
    }

    /// Whether this constraint represents a driving scalar dimension.
    pub fn is_dimensional(&self) -> bool {
        self.dimensional_value().is_some()
    }

    /// Returns the driving dimension value in SI units (meters or radians), if applicable.
    pub fn dimensional_value(&self) -> Option<f64> {
        self.refs().value.map(|(_, v)| v)
    }

    /// What a driving dimension measures: a length, an angle, or (Snell's
    /// ratio) a plain ratio. The document checks an expression against it
    /// before it resolves one into [`Constraint::set_dimensional_value`].
    pub fn dimension_kind(&self) -> Option<QuantityKind> {
        self.refs().value.map(|(k, _)| k)
    }

    /// Sets the driving dimension value in SI units (meters or radians). Returns true if modified.
    pub fn set_dimensional_value(&mut self, new_val: f64) -> bool {
        match self {
            Self::Distance { value, .. }
            | Self::HorizontalDistance { value, .. }
            | Self::VerticalDistance { value, .. }
            | Self::DistancePointLine { value, .. }
            | Self::DistanceParallelLines { value, .. }
            | Self::Angle { value, .. }
            | Self::Radius { value, .. }
            | Self::Diameter { value, .. }
            | Self::ArcLength { value, .. }
            | Self::AnglePoints { value, .. }
            | Self::DistanceToAxisX { value, .. }
            | Self::DistanceToAxisY { value, .. }
            | Self::DistanceCircleCircle { value, .. }
            | Self::DistancePointCircle { value, .. }
            | Self::DistanceToDatum { value, .. }
            | Self::AngleWithDatum { value, .. } => {
                *value = new_val;
                true
            }
            #[cfg(feature = "conics")]
            Self::MinorRadius { value, .. }
            | Self::MajorRadius { value, .. }
            | Self::BSplineCurvature { value, .. } => {
                *value = new_val;
                true
            }
            #[cfg(feature = "snells-law")]
            Self::SnellsLaw { ratio, .. } => {
                *ratio = new_val;
                true
            }
            _ => false,
        }
    }
}

/// What a constraint names, in the order it names them, and the value it
/// drives, if any, with the kind of quantity that value is.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConstraintRefs {
    pub points: Vec<PointId>,
    pub entities: Vec<EntityId>,
    pub datum: Option<DatumEntity>,
    pub value: Option<(QuantityKind, f64)>,
}

impl ConstraintRefs {
    fn p(mut self, p: PointId) -> Self {
        self.points.push(p);
        self
    }

    fn e(mut self, e: EntityId) -> Self {
        self.entities.push(e);
        self
    }

    fn d(mut self, d: DatumEntity) -> Self {
        self.datum = Some(d);
        self
    }

    fn len(mut self, v: f64) -> Self {
        self.value = Some((QuantityKind::Length, v));
        self
    }

    fn ang(mut self, v: f64) -> Self {
        self.value = Some((QuantityKind::Angle, v));
        self
    }
}

impl Constraint {
    /// The one place a constraint is taken apart for what it references.
    pub fn refs(&self) -> ConstraintRefs {
        let r = ConstraintRefs::default;
        match *self {
            Self::Coincident { a, b }
            | Self::HorizontalPoints { a, b }
            | Self::VerticalPoints { a, b } => r().p(a).p(b),
            Self::Horizontal { line } | Self::Vertical { line } => r().e(line),
            Self::Parallel { a, b }
            | Self::Perpendicular { a, b }
            | Self::Equal { a, b }
            | Self::Concentric { a, b }
            | Self::TangentCircles { a, b } => r().e(a).e(b),
            Self::Tangent { line, circle } => r().e(line).e(circle),
            Self::Symmetric { a, b, mirror } => r().p(a).p(b).e(mirror),
            Self::Distance { a, b, value }
            | Self::HorizontalDistance { a, b, value }
            | Self::VerticalDistance { a, b, value } => r().p(a).p(b).len(value),
            Self::DistancePointLine { point, line, value } => r().p(point).e(line).len(value),
            Self::DistanceParallelLines { a, b, value }
            | Self::DistanceCircleCircle { a, b, value } => r().e(a).e(b).len(value),
            Self::Angle { a, b, value } => r().e(a).e(b).ang(value),
            Self::Radius { target, value } | Self::Diameter { target, value } => {
                r().e(target).len(value)
            }
            Self::PointOnLine { point, line } | Self::Midpoint { point, line } => {
                r().p(point).e(line)
            }
            Self::PointOnCircle { point, circle } => r().p(point).e(circle),
            Self::Fix { point, .. } => r().p(point),
            Self::SymmetricPoints { a, b, center } => r().p(a).p(b).p(center),
            Self::PointOnPerpBisector { point, a, b } => r().p(point).p(a).p(b),
            Self::ArcLength { arc, value } => r().e(arc).len(value),
            Self::Block { entity } => r().e(entity),
            Self::AnglePoints {
                a,
                vertex,
                b,
                value,
            } => r().p(a).p(vertex).p(b).ang(value),
            Self::DistanceToAxisX { point, value } | Self::DistanceToAxisY { point, value } => {
                r().p(point).len(value)
            }
            Self::DistancePointCircle {
                point,
                circle,
                value,
            } => r().p(point).e(circle).len(value),
            #[cfg(feature = "snells-law")]
            Self::SnellsLaw {
                ray1_start,
                ray1_end,
                ray2_end,
                boundary,
                ratio,
            } => {
                let mut out = r().p(ray1_start).p(ray1_end).p(ray2_end).e(boundary);
                out.value = Some((QuantityKind::Ratio, ratio));
                out
            }
            Self::PointOnDatum { point, datum } | Self::CoincidentToDatum { point, datum } => {
                r().p(point).d(datum)
            }
            Self::DistanceToDatum {
                point,
                datum,
                value,
            } => r().p(point).d(datum).len(value),
            Self::AngleWithDatum { line, datum, value } => r().e(line).d(datum).ang(value),
            Self::SymmetricAcrossDatum { a, b, datum } => r().p(a).p(b).d(datum),
            #[cfg(feature = "conics")]
            Self::PointOnEllipse { point, ellipse } => r().p(point).e(ellipse),
            #[cfg(feature = "conics")]
            Self::TangentLineEllipse { line, ellipse } => r().e(line).e(ellipse),
            #[cfg(feature = "conics")]
            Self::InternalAlignment { ellipse, alignment } => r().p(alignment.point()).e(ellipse),
            #[cfg(feature = "conics")]
            Self::MinorRadius { ellipse, value } | Self::MajorRadius { ellipse, value } => {
                r().e(ellipse).len(value)
            }
            #[cfg(feature = "conics")]
            Self::PointOnBSpline { point, bspline, .. } => r().p(point).e(bspline),
            #[cfg(feature = "conics")]
            Self::BSplineTangent { bspline, line, .. } => r().e(bspline).e(line),
            #[cfg(feature = "conics")]
            Self::BSplineCurvature { bspline, value, .. } => {
                let mut out = r().e(bspline);
                out.value = Some((QuantityKind::Ratio, value));
                out
            }
        }
    }
}

fn yes() -> bool {
    true
}

fn is_true(b: &bool) -> bool {
    *b
}

fn is_primary(p: &ConstraintPriority) -> bool {
    *p == ConstraintPriority::Primary
}

/// Priority in the solve. Primary constraints are enforced; drag
/// constraints guide the free degrees of freedom during a drag.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintPriority {
    #[default]
    Primary = 0,
    Drag = 1,
}

/// A stored constraint: its id, the constraint, and its flags. The fields
/// at their defaults are not written.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstraintRecord {
    pub id: ConstraintId,
    pub constraint: Constraint,
    /// Driving constraints are solved; a reference one is a measurement.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub is_driving: bool,
    /// A suppressed (inactive) constraint is ignored by the solver.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub is_active: bool,
    /// A name the user gave it (`Width_Slot_1`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "is_primary")]
    pub priority: ConstraintPriority,
    /// Where a dimension's label sits, plane-local `(u, v)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_pos: Option<(f64, f64)>,
    /// The expression source that drives a dimension's value
    /// (`"plate_w / 2"`). Opaque here: this crate never parses it; the
    /// document resolves it before every solve. It lives on the record so
    /// undo and removal carry it along.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expr: Option<String>,
}

impl ConstraintRecord {
    pub fn new(id: ConstraintId, constraint: Constraint) -> Self {
        Self {
            id,
            constraint,
            is_driving: true,
            is_active: true,
            name: None,
            priority: ConstraintPriority::Primary,
            display_pos: None,
            expr: None,
        }
    }

    pub fn is_reference(&self) -> bool {
        !self.is_driving
    }

    pub fn is_suppressed(&self) -> bool {
        !self.is_active
    }

    pub fn is_drag(&self) -> bool {
        self.priority == ConstraintPriority::Drag
    }
}
