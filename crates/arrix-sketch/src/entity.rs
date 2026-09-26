//! Sketch entities: free points and the curves built on them
//! (docs/DATA-MODEL.md §Sketches).

use serde::{Deserialize, Serialize};

use crate::ids::PointId;

fn is_false(b: &bool) -> bool {
    !*b
}

/// A 2D point in plane-local coordinates, in metres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Point {
    pub x: f64,
    pub y: f64,
    /// A fixed point is not a solver variable: it anchors the sketch in
    /// the plane.
    #[serde(default, skip_serializing_if = "is_false")]
    pub fixed: bool,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y, fixed: false }
    }

    pub fn fixed(x: f64, y: f64) -> Self {
        Self { x, y, fixed: true }
    }

    pub fn pos(&self) -> [f64; 2] {
        [self.x, self.y]
    }

    pub fn set_pos(&mut self, x: f64, y: f64) {
        self.x = x;
        self.y = y;
    }

    pub fn distance_to(&self, other: &Point) -> f64 {
        (self.x - other.x).hypot(self.y - other.y)
    }
}

/// A curve built on points. Lines and arcs own their ends as [`PointId`]s,
/// so a shared vertex is coincident by identity, not only by constraint. A
/// circle's radius is a solver variable of its own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Entity {
    /// A lone point, drawn as geometry of its own.
    Point(PointId),
    Line {
        start: PointId,
        end: PointId,
    },
    Circle {
        center: PointId,
        /// In metres; a solver variable unless constrained.
        radius: f64,
    },
    /// A circular arc counter-clockwise from `start` to `end` about
    /// `center`. `|start − center|` and `|end − center|` are held equal by
    /// an implicit residual.
    Arc {
        center: PointId,
        start: PointId,
        end: PointId,
    },
    /// An ellipse about `center`: its major radius is the distance to
    /// `major_axis_end`, its minor radius `minor_radius`.
    #[cfg(feature = "conics")]
    Ellipse {
        center: PointId,
        major_axis_end: PointId,
        minor_radius: f64,
    },
    /// An elliptic arc from `start` to `end` on the ellipse the first three
    /// fields define.
    #[cfg(feature = "conics")]
    ArcOfEllipse {
        center: PointId,
        major_axis_end: PointId,
        minor_radius: f64,
        start: PointId,
        end: PointId,
    },
    /// A non-uniform B-spline on its control points.
    #[cfg(feature = "conics")]
    BSpline {
        control_points: Vec<PointId>,
        knots: Vec<f64>,
        degree: usize,
        periodic: bool,
    },
}

impl Entity {
    /// Whether this entity can bound a profile: every curve, never a lone
    /// point.
    pub fn is_curve(&self) -> bool {
        !matches!(self, Self::Point(_))
    }

    /// The points this entity is built on, in the order its fields name
    /// them.
    pub fn point_ids(&self) -> Vec<PointId> {
        match *self {
            Self::Point(point) => vec![point],
            Self::Line { start, end } => vec![start, end],
            Self::Circle { center, .. } => vec![center],
            Self::Arc { center, start, end } => vec![center, start, end],
            #[cfg(feature = "conics")]
            Self::Ellipse {
                center,
                major_axis_end,
                ..
            } => vec![center, major_axis_end],
            #[cfg(feature = "conics")]
            Self::ArcOfEllipse {
                center,
                major_axis_end,
                start,
                end,
                ..
            } => vec![center, major_axis_end, start, end],
            #[cfg(feature = "conics")]
            Self::BSpline {
                ref control_points, ..
            } => control_points.clone(),
        }
    }

    pub fn is_line(&self) -> bool {
        matches!(self, Self::Line { .. })
    }

    pub fn is_circle_like(&self) -> bool {
        matches!(self, Self::Circle { .. } | Self::Arc { .. })
    }

    pub fn is_ellipse_like(&self) -> bool {
        #[cfg(feature = "conics")]
        return matches!(self, Self::Ellipse { .. } | Self::ArcOfEllipse { .. });
        #[cfg(not(feature = "conics"))]
        false
    }

    pub fn is_bspline(&self) -> bool {
        #[cfg(feature = "conics")]
        return matches!(self, Self::BSpline { .. });
        #[cfg(not(feature = "conics"))]
        false
    }

    pub fn line_ends(&self) -> Option<(PointId, PointId)> {
        match *self {
            Self::Line { start, end } => Some((start, end)),
            _ => None,
        }
    }

    /// The centre of a circle or arc; `None` for every other entity.
    pub fn circle_center(&self) -> Option<PointId> {
        match *self {
            Self::Circle { center, .. } | Self::Arc { center, .. } => Some(center),
            _ => None,
        }
    }

    /// The ends of a circular arc.
    pub fn arc_ends(&self) -> Option<(PointId, PointId)> {
        match *self {
            Self::Arc { start, end, .. } => Some((start, end)),
            _ => None,
        }
    }

    #[cfg(feature = "conics")]
    pub fn ellipse_center(&self) -> Option<PointId> {
        match *self {
            Self::Ellipse { center, .. } | Self::ArcOfEllipse { center, .. } => Some(center),
            _ => None,
        }
    }

    #[cfg(feature = "conics")]
    pub fn ellipse_major_axis_end(&self) -> Option<PointId> {
        match *self {
            Self::Ellipse { major_axis_end, .. } | Self::ArcOfEllipse { major_axis_end, .. } => {
                Some(major_axis_end)
            }
            _ => None,
        }
    }

    #[cfg(feature = "conics")]
    pub fn ellipse_minor_radius(&self) -> Option<f64> {
        match *self {
            Self::Ellipse { minor_radius, .. } | Self::ArcOfEllipse { minor_radius, .. } => {
                Some(minor_radius)
            }
            _ => None,
        }
    }

    #[cfg(feature = "conics")]
    pub fn bspline_control_points(&self) -> Option<&[PointId]> {
        match self {
            Self::BSpline { control_points, .. } => Some(control_points),
            _ => None,
        }
    }
}
