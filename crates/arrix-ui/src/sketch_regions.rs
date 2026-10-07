//! Region shading: the faces of the sketch's planar arrangement, painted
//! under everything else the overlay draws (docs/UI-RENDERING.md §Sketch
//! mode).
//!
//! A region is concave as often as not (a D, an L, a plate with a bore),
//! and egui fills a closed path with a triangle fan, which is right only for
//! a convex one and knows no hole. So the fill is built here as a mesh: the
//! face is cut into horizontal bands at every vertex ordinate, and each band
//! contributes one trapezoid per stretch of it that is inside the face. The
//! inside test is even-odd over the outer boundary and the holes together,
//! so a bore costs nothing extra. Between two adjacent ordinates no edge
//! starts, ends or overtakes another, so the tiling is exact: every
//! trapezoid's sloping sides are boundary edges themselves.

use egui::{Color32, Pos2, Shape, Stroke};
use serde::{Deserialize, Serialize};

use crate::PlaneToScreen;

/// Ordinates closer than this are one band: under a tenth of a pixel, where
/// a trapezoid has no area to contribute.
const BAND_EPSILON: f32 = 0.05;

const IDLE: Color32 = Color32::from_rgba_premultiplied(14, 28, 44, 44);
const HOVER: Color32 = Color32::from_rgba_premultiplied(30, 56, 90, 90);
const OUTLINE: Color32 = Color32::from_rgb(120, 190, 255);

/// One region of the open sketch: the key a feature stores to come back to
/// it, and its boundary in plane-local metres.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SketchRegionView {
    /// The region's key as the document stores it (`Ref::Region`), in JSON.
    pub key: String,
    /// How many holes the region has.
    pub holes: usize,
    /// The outer boundary and the holes, closed implicitly. Not in the
    /// debug state: the keys name the regions there.
    #[serde(skip)]
    pub outer: Vec<[f64; 2]>,
    #[serde(skip)]
    pub hole_rings: Vec<Vec<[f64; 2]>>,
}

/// Paints every region, the hovered one (by key) brightened and outlined.
/// Drawn before the curves, so one is never dimmed by the face it bounds.
pub(crate) fn paint_regions(
    painter: &egui::Painter,
    to_screen: &PlaneToScreen,
    regions: &[SketchRegionView],
    hover: Option<&str>,
) {
    for region in regions {
        let hovered = hover == Some(region.key.as_str());
        let Some(rings) = project_rings(region, to_screen) else {
            continue;
        };
        let mesh = region_mesh(&rings, if hovered { HOVER } else { IDLE });
        if !mesh.is_empty() {
            painter.add(Shape::mesh(mesh));
        }
        if hovered {
            for ring in rings {
                painter.add(Shape::closed_line(ring, Stroke::new(1.5, OUTLINE)));
            }
        }
    }
}

/// The region's rings on the screen; `None` when one has a point the plane
/// does not show.
fn project_rings(region: &SketchRegionView, to_screen: &PlaneToScreen) -> Option<Vec<Vec<Pos2>>> {
    std::iter::once(&region.outer)
        .chain(&region.hole_rings)
        .map(|ring| ring.iter().map(|p| to_screen.project(*p)).collect())
        .collect()
}

/// The face as filled triangles. See the module doc for the decomposition.
fn region_mesh(rings: &[Vec<Pos2>], color: Color32) -> egui::Mesh {
    let mut mesh = egui::Mesh::default();
    let edges: Vec<(Pos2, Pos2)> = rings
        .iter()
        .flat_map(|ring| {
            ring.iter()
                .zip(ring.iter().cycle().skip(1))
                .take(ring.len())
                .map(|(a, b)| (*a, *b))
        })
        .collect();
    if edges.len() < 3 {
        return mesh;
    }
    let mut ys: Vec<f32> = rings.iter().flatten().map(|p| p.y).collect();
    ys.sort_by(f32::total_cmp);
    ys.dedup_by(|a, b| (*a - *b).abs() <= BAND_EPSILON);

    let mut crossings: Vec<[f32; 3]> = Vec::new();
    for band in ys.windows(2) {
        let (y0, y1) = (band[0], band[1]);
        let middle = 0.5 * (y0 + y1);
        crossings.clear();
        for (a, b) in &edges {
            // An edge straddling the band's midpoint spans the whole band,
            // since no vertex ordinate falls strictly inside it.
            if (a.y > middle) != (b.y > middle) {
                let at = |y: f32| a.x + (b.x - a.x) * (y - a.y) / (b.y - a.y);
                crossings.push([at(middle), at(y0), at(y1)]);
            }
        }
        crossings.sort_by(|p, q| p[0].total_cmp(&q[0]));
        for span in crossings.as_chunks::<2>().0 {
            let base = mesh.vertices.len() as u32;
            mesh.colored_vertex(Pos2::new(span[0][1], y0), color);
            mesh.colored_vertex(Pos2::new(span[1][1], y0), color);
            mesh.colored_vertex(Pos2::new(span[1][2], y1), color);
            mesh.colored_vertex(Pos2::new(span[0][2], y1), color);
            mesh.add_triangle(base, base + 1, base + 2);
            mesh.add_triangle(base, base + 2, base + 3);
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(min: f32, max: f32) -> Vec<Pos2> {
        vec![
            Pos2::new(min, min),
            Pos2::new(max, min),
            Pos2::new(max, max),
            Pos2::new(min, max),
        ]
    }

    fn area(mesh: &egui::Mesh) -> f32 {
        mesh.indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| {
                let [a, b, c] = [t[0], t[1], t[2]].map(|i| mesh.vertices[i as usize].pos);
                0.5 * ((b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y)).abs()
            })
            .sum()
    }

    #[test]
    fn a_square_with_a_bore_tiles_to_its_own_area() {
        let mesh = region_mesh(&[square(0.0, 100.0), square(40.0, 60.0)], Color32::WHITE);
        assert!((area(&mesh) - 9600.0).abs() < 0.5, "{}", area(&mesh));
    }

    /// A triangle fan over these vertices would cover the notch.
    #[test]
    fn a_concave_face_is_not_filled_across_its_notch() {
        let u = vec![
            Pos2::new(0.0, 0.0),
            Pos2::new(100.0, 0.0),
            Pos2::new(100.0, 100.0),
            Pos2::new(60.0, 100.0),
            Pos2::new(60.0, 40.0),
            Pos2::new(40.0, 40.0),
            Pos2::new(40.0, 100.0),
            Pos2::new(0.0, 100.0),
        ];
        let mesh = region_mesh(&[u], Color32::WHITE);
        assert!((area(&mesh) - 8800.0).abs() < 0.5, "{}", area(&mesh));
    }
}
