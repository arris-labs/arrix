//! The sketch overlay: a view type for what sketch mode shows, and its
//! painter (docs/UI-RENDERING.md §Sketch mode, §View types). Everything is
//! plain data in plane-local metres; `arrix-app` builds it and hands over
//! the [`PlaneToScreen`] transform it is drawn through, so this module
//! names no document or sketch type and holds no camera.

use egui::{Align2, Color32, FontId, Pos2, Rect, Shape, Stroke, Vec2};
use serde::{Deserialize, Serialize};

/// How far a sketch is determined, as the solver reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SolverState {
    /// Solved, with freedom left.
    UnderConstrained,
    FullyConstrained,
    /// The constraints contradict each other: one has to go.
    OverConstrained,
    /// The solver stopped short of a solution the constraints allow.
    DidNotConverge,
}

impl SolverState {
    pub fn label(self) -> &'static str {
        match self {
            Self::UnderConstrained => "under-constrained",
            Self::FullyConstrained => "fully constrained",
            Self::OverConstrained => "over-constrained",
            Self::DidNotConverge => "did not converge",
        }
    }
}

/// How much freedom a point or curve has left, which colours it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DofState {
    Under,
    Fully,
    /// Held by a fix or an anchor.
    Fixed,
    /// Part of what the solver found contradictory.
    Conflicting,
}

impl DofState {
    fn colour(self) -> Color32 {
        match self {
            Self::Under => Color32::from_rgb(88, 170, 235),
            Self::Fully => Color32::from_rgb(235, 235, 235),
            Self::Fixed => Color32::from_rgb(178, 128, 235),
            Self::Conflicting => Color32::from_rgb(242, 72, 72),
        }
    }
}

/// What the status line says of the open sketch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SketchStatusView {
    pub dof: i32,
    pub state: SolverState,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SketchPointView {
    pub id: String,
    pub at: [f64; 2],
    pub state: DofState,
}

/// The shape of one curve: a line, a circle, or a circular arc running
/// counter-clockwise from `start` to `end` (radians, from the plane's U
/// axis).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CurveShape {
    Line {
        from: [f64; 2],
        to: [f64; 2],
    },
    Circle {
        centre: [f64; 2],
        radius: f64,
    },
    Arc {
        centre: [f64; 2],
        radius: f64,
        start: f64,
        end: f64,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SketchCurveView {
    pub id: String,
    pub shape: CurveShape,
    /// Drawn dashed, and never part of a profile.
    pub construction: bool,
    pub state: DofState,
}

/// One constraint's mark beside what it holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConstraintGlyphView {
    /// The constraint's id.
    pub id: String,
    /// The constraint's kind, as the document names it (`horizontal`).
    pub kind: String,
    /// What is drawn: a letter or a sign.
    pub symbol: String,
    pub at: [f64; 2],
    /// The mark is of a redundant or conflicting constraint.
    pub flagged: bool,
}

/// A dimension: its leader, and its label with the value it holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DimensionView {
    pub id: String,
    pub kind: String,
    pub text: String,
    pub from: [f64; 2],
    pub to: [f64; 2],
    pub at: [f64; 2],
    /// Measured, not driving.
    pub reference: bool,
    pub flagged: bool,
}

/// Everything sketch mode draws over the viewport, in plane-local metres.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SketchView {
    pub points: Vec<SketchPointView>,
    pub curves: Vec<SketchCurveView>,
    pub glyphs: Vec<ConstraintGlyphView>,
    pub dimensions: Vec<DimensionView>,
    pub status: SketchStatusView,
}

/// The map from the sketch's plane to the screen: a homography, so it is
/// exact for both projections and inverts. The app builds it from the
/// camera and the plane's frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaneToScreen {
    to_screen: [[f64; 3]; 3],
    from_screen: [[f64; 3]; 3],
}

impl PlaneToScreen {
    /// From the clip coordinates `(x, y, w)` of the plane's U direction, V
    /// direction and origin, and the viewport they are drawn in. `None`
    /// when the plane is edge-on.
    pub fn new(
        clip_u: [f64; 3],
        clip_v: [f64; 3],
        clip_origin: [f64; 3],
        viewport: Rect,
    ) -> Option<Self> {
        let (w, h) = (f64::from(viewport.width()), f64::from(viewport.height()));
        let (x0, y0) = (f64::from(viewport.min.x), f64::from(viewport.min.y));
        let col = [clip_u, clip_v, clip_origin];
        let row = |f: &dyn Fn([f64; 3]) -> f64| [f(col[0]), f(col[1]), f(col[2])];
        let sx = row(&|c| w / 2.0 * c[0] + (x0 + w / 2.0) * c[2]);
        let sy = row(&|c| -h / 2.0 * c[1] + (y0 + h / 2.0) * c[2]);
        let sw = row(&|c| c[2]);
        let to_screen = [sx, sy, sw];
        let from_screen = invert(to_screen)?;
        Some(Self {
            to_screen,
            from_screen,
        })
    }

    /// Where the plane-local point `uv` (metres) is on the screen; `None`
    /// behind the eye.
    pub fn project(&self, uv: [f64; 2]) -> Option<Pos2> {
        let [x, y, w] = apply(self.to_screen, [uv[0], uv[1], 1.0]);
        (w > 1e-12).then(|| Pos2::new((x / w) as f32, (y / w) as f32))
    }

    /// The plane-local point under screen position `at`.
    pub fn unproject(&self, at: Pos2) -> Option<[f64; 2]> {
        let [u, v, w] = apply(self.from_screen, [f64::from(at.x), f64::from(at.y), 1.0]);
        (w.abs() > 1e-12).then(|| [u / w, v / w])
    }

    /// Screen points per metre of the plane near `uv`: what turns a
    /// tolerance in pixels into one in metres.
    pub fn points_per_metre(&self, uv: [f64; 2]) -> Option<f64> {
        const STEP: f64 = 1e-3;
        let a = self.project(uv)?;
        let b = self.project([uv[0] + STEP, uv[1]])?;
        Some(f64::from((b - a).length()) / STEP)
    }
}

fn apply(m: [[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    let dot = |r: [f64; 3]| r[0] * v[0] + r[1] * v[1] + r[2] * v[2];
    [dot(m[0]), dot(m[1]), dot(m[2])]
}

fn invert(m: [[f64; 3]; 3]) -> Option<[[f64; 3]; 3]> {
    let c = |r: usize, k: usize| {
        let (r1, r2) = ((r + 1) % 3, (r + 2) % 3);
        let (k1, k2) = ((k + 1) % 3, (k + 2) % 3);
        m[r1][k1] * m[r2][k2] - m[r1][k2] * m[r2][k1]
    };
    let det = m[0][0] * c(0, 0) + m[0][1] * c(0, 1) + m[0][2] * c(0, 2);
    // Relative to the matrix's own scale, which is points per metre cubed.
    let scale = m.iter().flatten().map(|x| x.abs()).fold(0.0, f64::max);
    if det.abs() <= 1e-12 * scale * scale * scale {
        return None;
    }
    let mut inverse = [[0.0; 3]; 3];
    for (r, row) in inverse.iter_mut().enumerate() {
        for (k, cell) in row.iter_mut().enumerate() {
            *cell = c(k, r) / det;
        }
    }
    Some(inverse)
}

/// Segments to the full circle when a curve is drawn as a polyline.
const CIRCLE_SEGMENTS: f64 = 96.0;

/// Draws `view` through `to_screen`, inside `rect`.
pub fn paint_sketch(painter: &egui::Painter, to_screen: &PlaneToScreen, view: &SketchView) {
    let project = |uv: [f64; 2]| to_screen.project(uv);
    for curve in &view.curves {
        let stroke = Stroke::new(1.75, curve.state.colour());
        let polyline: Vec<Pos2> = match &curve.shape {
            CurveShape::Line { from, to } => [project(*from), project(*to)]
                .into_iter()
                .flatten()
                .collect(),
            CurveShape::Circle { centre, radius } => {
                arc_points(*centre, *radius, 0.0, std::f64::consts::TAU, &project)
            }
            CurveShape::Arc {
                centre,
                radius,
                start,
                end,
            } => {
                let mut sweep = end - start;
                while sweep <= 0.0 {
                    sweep += std::f64::consts::TAU;
                }
                arc_points(*centre, *radius, *start, sweep, &project)
            }
        };
        if polyline.len() < 2 {
            continue;
        }
        if curve.construction {
            painter.extend(Shape::dashed_line(&polyline, stroke, 6.0, 4.0));
        } else {
            painter.add(Shape::line(polyline, stroke));
        }
    }
    for dimension in &view.dimensions {
        paint_dimension(painter, &project, dimension);
    }
    for point in &view.points {
        if let Some(at) = project(point.at) {
            let colour = point.state.colour();
            painter.rect_filled(Rect::from_center_size(at, Vec2::splat(6.0)), 1.0, colour);
        }
    }
    for glyph in &view.glyphs {
        let Some(at) = project(glyph.at) else {
            continue;
        };
        let colour = if glyph.flagged {
            DofState::Conflicting.colour()
        } else {
            Color32::from_rgb(210, 200, 120)
        };
        let centre = at + Vec2::new(10.0, -10.0);
        let rect = Rect::from_center_size(centre, Vec2::splat(14.0));
        painter.rect_filled(rect, 2.0, Color32::from_black_alpha(160));
        painter.text(
            centre,
            Align2::CENTER_CENTER,
            &glyph.symbol,
            FontId::monospace(10.0),
            colour,
        );
    }
}

/// `steps` points along the arc of `radius` about `centre` from angle
/// `start` through `sweep`, those in front of the eye.
fn arc_points(
    centre: [f64; 2],
    radius: f64,
    start: f64,
    sweep: f64,
    project: &dyn Fn([f64; 2]) -> Option<Pos2>,
) -> Vec<Pos2> {
    let steps = (sweep / std::f64::consts::TAU * CIRCLE_SEGMENTS)
        .ceil()
        .max(2.0) as usize;
    (0..=steps)
        .filter_map(|i| {
            let a = start + sweep * i as f64 / steps as f64;
            project([centre[0] + radius * a.cos(), centre[1] + radius * a.sin()])
        })
        .collect()
}

fn paint_dimension(
    painter: &egui::Painter,
    project: &dyn Fn([f64; 2]) -> Option<Pos2>,
    dimension: &DimensionView,
) {
    let colour = if dimension.flagged {
        DofState::Conflicting.colour()
    } else if dimension.reference {
        Color32::from_gray(150)
    } else {
        Color32::from_rgb(120, 205, 150)
    };
    let stroke = Stroke::new(1.0, colour);
    if let (Some(a), Some(b)) = (project(dimension.from), project(dimension.to)) {
        painter.line_segment([a, b], stroke);
        for end in [a, b] {
            painter.circle_filled(end, 2.0, colour);
        }
    }
    if let Some(at) = project(dimension.at) {
        let galley =
            painter.layout_no_wrap(dimension.text.clone(), FontId::proportional(12.0), colour);
        let rect = Align2::CENTER_CENTER
            .anchor_size(at, galley.size())
            .expand(2.0);
        painter.rect_filled(rect, 2.0, Color32::from_black_alpha(190));
        painter.galley(rect.min + Vec2::splat(2.0), galley, colour);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An orthographic view square to the plane, 1 m across 200 points.
    fn square_on() -> PlaneToScreen {
        let viewport = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(200.0, 100.0));
        // Clip x spans ±0.5 m, clip y ±0.25 m, w 1.
        PlaneToScreen::new([2.0, 0.0, 0.0], [0.0, 4.0, 0.0], [0.0, 0.0, 1.0], viewport)
            .expect("a plane facing the camera")
    }

    #[test]
    fn the_origin_is_the_viewports_centre_and_v_is_up() {
        let t = square_on();
        assert_eq!(t.project([0.0, 0.0]), Some(Pos2::new(110.0, 70.0)));
        let right = t.project([0.5, 0.0]).unwrap();
        let up = t.project([0.0, 0.25]).unwrap();
        assert_eq!(right, Pos2::new(210.0, 70.0));
        assert_eq!(up, Pos2::new(110.0, 20.0));
    }

    #[test]
    fn unprojecting_undoes_projecting_in_perspective_too() {
        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(300.0, 200.0));
        // A tilted plane: w grows with v.
        let t = PlaneToScreen::new([1.0, 0.0, 0.1], [0.0, 1.0, 0.5], [0.1, -0.2, 3.0], viewport)
            .expect("an invertible view");
        for uv in [[0.0, 0.0], [0.4, -0.3], [-1.0, 0.7]] {
            let back = t.unproject(t.project(uv).unwrap()).unwrap();
            assert!(
                // Through f32 screen points.
                (back[0] - uv[0]).abs() < 1e-3 && (back[1] - uv[1]).abs() < 1e-3
            );
        }
    }

    #[test]
    fn an_edge_on_plane_has_no_transform() {
        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(100.0, 100.0));
        assert!(
            PlaneToScreen::new([1.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 0.0, 1.0], viewport)
                .is_none()
        );
    }

    #[test]
    fn a_metre_is_this_many_points_where_the_view_is_square() {
        let scale = square_on().points_per_metre([0.0, 0.0]).unwrap();
        assert!((scale - 200.0).abs() < 0.1, "{scale}");
    }
}
