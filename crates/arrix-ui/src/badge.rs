//! The frozen badge (docs/UI-RENDERING.md §The viewport, overlays): a chip
//! over the viewport at a frozen feature's faces, saying the feature's name
//! and that its result is the stored one. The app projects where it goes;
//! this paints it and says which one the pointer is over.

use egui::{Align2, FontId, Id, Rect, Sense, Stroke, StrokeKind, vec2};
use serde::{Deserialize, Serialize};

use crate::FeatureState;

/// One badge, as the app placed it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BadgeView {
    /// The frozen feature's id.
    pub feature: String,
    /// The feature's name in the tree.
    pub title: String,
    /// `Frozen` or `FrozenStale`.
    pub state: FeatureState,
    /// Why it is frozen, in the form's words: what hovering says.
    pub reason: String,
    /// The chip's centre, in points.
    pub at: [f64; 2],
}

impl BadgeView {
    /// The chip's text: the name, then the state.
    pub fn label(&self) -> String {
        match self.state.tag() {
            Some(tag) => format!("{} · {tag}", self.title),
            None => self.title.clone(),
        }
    }
}

/// Draws `badges` over `ui`'s rect, and returns the index of the one the
/// pointer is over. A chip takes the clicks on it: a click on a badge picks
/// nothing in the scene behind it.
pub fn paint_badges(ui: &mut egui::Ui, badges: &[BadgeView]) -> Option<usize> {
    let mut hovered = None;
    for (i, badge) in badges.iter().enumerate() {
        let visuals = ui.visuals().clone();
        let font = FontId::proportional(12.0);
        let galley =
            ui.painter()
                .layout_no_wrap(badge.label(), font.clone(), visuals.warn_fg_color);
        let size = galley.size() + vec2(14.0, 6.0);
        let centre = egui::pos2(badge.at[0] as f32, badge.at[1] as f32);
        let rect = Rect::from_center_size(centre, size);
        let id = Id::new(("arrix.badge", &badge.feature, i));
        if ui.interact(rect, id, Sense::click()).hovered() {
            hovered = Some(i);
        }
        let radius = rect.height() / 2.0;
        ui.painter().rect_filled(rect, radius, visuals.window_fill);
        ui.painter().rect_stroke(
            rect,
            radius,
            Stroke::new(1.0, visuals.warn_fg_color),
            StrokeKind::Inside,
        );
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            badge.label(),
            font,
            visuals.warn_fg_color,
        );
    }
    hovered
}
