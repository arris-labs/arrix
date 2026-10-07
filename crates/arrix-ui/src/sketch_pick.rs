//! What a form's field picks in a finished sketch, drawn in part mode
//! (docs/UI-RENDERING.md §The viewport): a region, a point, or a line. The
//! field decides which kind the pointer finds; the other kinds are not
//! there to hit.

use egui::Pos2;

use crate::sketch::{distance_to_polyline, polyline};
use crate::{CurveShape, PlaneToScreen, SketchRegionView, SketchView};

/// What the active field of a form takes from a sketch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SketchAsk {
    Region,
    Point,
    /// A straight line.
    Axis,
}

/// An item of a sketch a field asked for, by the id or key its record has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PartSketchItem {
    Region { key: String },
    Point { id: String },
    Line { id: String },
}

/// Whether `uv` is inside the region: even-odd over the outer boundary and
/// the holes together, as the fill is.
fn contains(region: &SketchRegionView, uv: [f64; 2]) -> bool {
    let mut inside = false;
    for ring in std::iter::once(&region.outer).chain(&region.hole_rings) {
        for (a, b) in ring.iter().zip(ring.iter().cycle().skip(1)) {
            if (a[1] > uv[1]) != (b[1] > uv[1])
                && uv[0] < a[0] + (b[0] - a[0]) * (uv[1] - a[1]) / (b[1] - a[1])
            {
                inside = !inside;
            }
        }
    }
    inside
}

/// The item `ask` wants that is under `at` (screen points) within
/// `tolerance`: the nearest point or line, or the innermost region.
pub fn pick_sketch_item(
    view: &SketchView,
    to_screen: &PlaneToScreen,
    at: Pos2,
    tolerance: f32,
    ask: SketchAsk,
) -> Option<PartSketchItem> {
    let project = |uv: [f64; 2]| to_screen.project(uv);
    let nearest = |candidates: &mut dyn Iterator<Item = (&str, f32)>| {
        let mut best: Option<(&str, f32)> = None;
        for (id, distance) in candidates {
            if distance <= tolerance && best.is_none_or(|(_, d)| distance < d) {
                best = Some((id, distance));
            }
        }
        best.map(|(id, _)| id.to_owned())
    };
    match ask {
        SketchAsk::Point => nearest(
            &mut view
                .points
                .iter()
                .filter_map(|p| Some((p.id.as_str(), (project(p.at)? - at).length()))),
        )
        .map(|id| PartSketchItem::Point { id }),
        SketchAsk::Axis => nearest(&mut view.curves.iter().filter_map(|c| {
            if !matches!(c.shape, CurveShape::Line { .. }) {
                return None;
            }
            Some((
                c.id.as_str(),
                distance_to_polyline(&polyline(&c.shape, &project), at)?,
            ))
        }))
        .map(|id| PartSketchItem::Line { id }),
        // Largest first, so the last that holds it is the smallest.
        SketchAsk::Region => {
            let uv = to_screen.unproject(at)?;
            view.regions
                .iter()
                .rfind(|r| contains(r, uv))
                .map(|r| PartSketchItem::Region { key: r.key.clone() })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DofState, SketchCurveView, SketchPointView, SketchStatusView, SolverState};

    fn square(min: f64, max: f64) -> Vec<[f64; 2]> {
        vec![[min, min], [max, min], [max, max], [min, max]]
    }

    fn view() -> SketchView {
        SketchView {
            points: vec![SketchPointView {
                id: "p".into(),
                at: [0.0, 0.0],
                state: DofState::Under,
            }],
            curves: vec![
                SketchCurveView {
                    id: "line".into(),
                    shape: CurveShape::Line {
                        from: [0.0, 0.0],
                        to: [100.0, 0.0],
                    },
                    construction: false,
                    state: DofState::Under,
                },
                SketchCurveView {
                    id: "circle".into(),
                    shape: CurveShape::Circle {
                        centre: [50.0, 50.0],
                        radius: 10.0,
                    },
                    construction: false,
                    state: DofState::Under,
                },
            ],
            glyphs: vec![],
            dimensions: vec![],
            status: SketchStatusView {
                dof: 0,
                state: SolverState::UnderConstrained,
            },
            hover: None,
            selected: vec![],
            preview: None,
            field: None,
            regions: vec![
                SketchRegionView {
                    key: "plate".into(),
                    holes: 1,
                    outer: square(0.0, 100.0),
                    hole_rings: vec![square(40.0, 60.0)],
                },
                SketchRegionView {
                    key: "island".into(),
                    holes: 0,
                    outer: square(45.0, 55.0),
                    hole_rings: vec![],
                },
            ],
            hover_region: None,
            selected_regions: vec![],
        }
    }

    /// The identity plane: one unit of the sketch is one point.
    fn plane() -> PlaneToScreen {
        let rect = egui::Rect::from_min_max(Pos2::new(-1.0, -1.0), Pos2::new(1.0, 1.0));
        PlaneToScreen::new([1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0], rect)
            .expect("a plane")
    }

    fn at(ask: SketchAsk, x: f32, y: f32) -> Option<PartSketchItem> {
        let to_screen = plane();
        let p = to_screen.project([f64::from(x), f64::from(y)]).unwrap();
        pick_sketch_item(&view(), &to_screen, p, 3.0, ask)
    }

    #[test]
    fn the_field_decides_which_kind_is_there_to_hit() {
        let at_point = at(SketchAsk::Point, 1.0, 1.0);
        assert_eq!(at_point, Some(PartSketchItem::Point { id: "p".into() }));
        assert!(at(SketchAsk::Region, 1.0, 1.0).is_some());
        assert_eq!(
            at(SketchAsk::Axis, 50.0, 1.0),
            Some(PartSketchItem::Line { id: "line".into() })
        );
        assert_eq!(at(SketchAsk::Point, 50.0, 1.0), None, "a line is no point");
        assert_eq!(at(SketchAsk::Axis, 60.0, 50.0), None, "a circle is no axis");
    }

    #[test]
    fn a_region_is_the_innermost_that_holds_the_point_and_a_bore_is_not_in_it() {
        let key = |x, y| match at(SketchAsk::Region, x, y) {
            Some(PartSketchItem::Region { key }) => Some(key),
            _ => None,
        };
        assert_eq!(key(20.0, 20.0).as_deref(), Some("plate"));
        assert_eq!(key(50.0, 50.0).as_deref(), Some("island"));
        assert_eq!(key(42.0, 50.0), None, "inside the bore, outside the island");
        assert_eq!(key(150.0, 50.0), None);
    }
}
