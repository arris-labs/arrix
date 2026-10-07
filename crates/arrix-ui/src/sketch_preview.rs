//! A drawing tool's preview over the sketch: what the next click would
//! draw, where the cursor snapped, the snap's glyphs and alignment guides,
//! and the points acquired as alignment sources (docs/UI-RENDERING.md
//! §Sketch mode). Plain data in plane-local metres, built by the app from
//! the snap search; what a glyph commits is named so a scenario can hold
//! the preview against the edit the click makes.

use egui::{Align2, Color32, FontId, Pos2, Rect, Shape, Stroke, Vec2};
use serde::{Deserialize, Serialize};

use crate::sketch::{CurveShape, polyline};

/// One glyph of the snap: where, what it marks, and the constraint kinds
/// the click commits for it, as the document names them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SnapGlyphView {
    pub at: [f64; 2],
    /// `point`, `origin`, `horizontal`, `on-curve`, …
    pub kind: String,
    /// What is drawn: a letter or a sign.
    pub symbol: String,
    /// `coincident`, `horizontal_points`, …
    pub commits: Vec<String>,
}

/// What a drawing tool shows between clicks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SketchPreviewView {
    /// The tool's command id: `core.sketch.line`.
    pub tool: String,
    /// Where the cursor snapped: the point the next click places.
    pub cursor: [f64; 2],
    /// What the next click would draw: the pending segment, the
    /// rectangle's four sides; none before the first click.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub curves: Vec<CurveShape>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub glyphs: Vec<SnapGlyphView>,
    /// Alignment guides, each from the point it tracks to the cursor.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guides: Vec<[[f64; 2]; 2]>,
    /// The points acquired as alignment sources, by a dwell or by the draw.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub acquired: Vec<[f64; 2]>,
}

const PREVIEW: Color32 = Color32::from_rgb(120, 190, 255);
const GUIDE: Color32 = Color32::from_rgb(200, 160, 255);
const SNAP: Color32 = Color32::from_rgb(255, 210, 90);

/// Draws `preview` through `project`, over the sketch.
pub(crate) fn paint_preview(
    painter: &egui::Painter,
    project: &dyn Fn([f64; 2]) -> Option<Pos2>,
    preview: &SketchPreviewView,
) {
    for guide in &preview.guides {
        if let (Some(a), Some(b)) = (project(guide[0]), project(guide[1])) {
            painter.extend(Shape::dashed_line(
                &[a, b],
                Stroke::new(1.0, GUIDE),
                3.0,
                3.0,
            ));
        }
    }
    for curve in &preview.curves {
        let line = polyline(curve, project);
        if line.len() >= 2 {
            painter.add(Shape::line(line, Stroke::new(1.5, PREVIEW)));
        }
    }
    for at in preview.acquired.iter().filter_map(|p| project(*p)) {
        painter.circle_stroke(at, 5.0, Stroke::new(1.0, GUIDE));
    }
    if let Some(at) = project(preview.cursor) {
        let stroke = Stroke::new(1.0, SNAP);
        for d in [Vec2::new(6.0, 0.0), Vec2::new(0.0, 6.0)] {
            painter.line_segment([at - d, at + d], stroke);
        }
    }
    // A glyph's box below and to the right of its place, the next one of
    // the same place beside it.
    let mut placed: Vec<Pos2> = Vec::new();
    for glyph in &preview.glyphs {
        let Some(at) = project(glyph.at) else {
            continue;
        };
        let shift = placed.iter().filter(|p| (**p - at).length() < 1.0).count() as f32;
        placed.push(at);
        let rect =
            Rect::from_center_size(at + Vec2::new(12.0 + 16.0 * shift, 12.0), Vec2::splat(14.0));
        painter.rect_filled(rect, 2.0, Color32::from_black_alpha(170));
        painter.rect_stroke(rect, 2.0, Stroke::new(1.0, SNAP), egui::StrokeKind::Outside);
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            &glyph.symbol,
            FontId::monospace(10.0),
            SNAP,
        );
    }
}
