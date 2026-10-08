//! The frozen badges (docs/UI-RENDERING.md §The viewport, overlays): which
//! features show a stored result, and where on screen their faces are.
//! Session state only: nothing here issues a command.

use arrix_core::{DVec3, FeatureId};
use arrix_ui::{BadgeView, FeatureState};
use arrix_viewport::{OrbitCamera, Scene};

use crate::documents::{OpenDocument, frozen_reason, standing_of};

/// A frozen feature, before the camera says where its badge goes.
pub struct BadgeAnchor {
    pub feature: FeatureId,
    pub title: String,
    pub state: FeatureState,
    pub reason: String,
}

impl OpenDocument {
    /// Every frozen feature of the document, in history order.
    pub(crate) fn badge_anchors(&self) -> Vec<BadgeAnchor> {
        let document = self.replica.document();
        let mut out = Vec::new();
        for part in document.parts().values() {
            for (index, id) in part.history.iter().enumerate() {
                let Some(record) = part.features.get(id) else {
                    continue;
                };
                let standing = standing_of(&self.replica, part, index, record);
                let Some(info) = standing.frozen else {
                    continue;
                };
                out.push(BadgeAnchor {
                    feature: record.id,
                    title: record.name.clone(),
                    state: standing.state,
                    reason: frozen_reason(&record.type_id, &info),
                });
            }
        }
        out
    }
}

/// The point `p` (metres, world) on the screen of `rect`, in points; `None`
/// behind the camera.
pub(crate) fn project(camera: &OrbitCamera, rect: egui::Rect, p: [f64; 3]) -> Option<egui::Pos2> {
    let aspect = rect.width().max(1.0) / rect.height().max(1.0);
    let clip = camera.view_proj(aspect) * DVec3::from_array(p).as_vec3().extend(1.0);
    if clip.w <= 0.0 {
        return None;
    }
    let ndc = clip.truncate() / clip.w;
    Some(egui::pos2(
        rect.min.x + (ndc.x + 1.0) * 0.5 * rect.width(),
        rect.min.y + (1.0 - ndc.y) * 0.5 * rect.height(),
    ))
}

/// The badges to draw: one per body each frozen feature has faces on, at
/// the centre of those faces' box, so a joined body carries the badge at the
/// frozen feature's faces and not at its own centre.
pub(crate) fn place<M: arrix_viewport::BodyPayload>(
    anchors: &[BadgeAnchor],
    scene: &Scene<M>,
    camera: &OrbitCamera,
    rect: egui::Rect,
) -> Vec<BadgeView> {
    let mut out = Vec::new();
    for anchor in anchors {
        for (_, bounds) in scene.feature_bounds(anchor.feature) {
            let Some(at) = project(camera, rect, bounds.center().to_array().map(f64::from)) else {
                continue;
            };
            out.push(BadgeView {
                feature: anchor.feature.0.to_string(),
                title: anchor.title.clone(),
                state: anchor.state,
                reason: anchor.reason.clone(),
                at: [at.x, at.y].map(|x| (f64::from(x) * 1e6).round() / 1e6),
            });
        }
    }
    out
}
