//! Entering and leaving sketch mode (docs/UI-RENDERING.md §Sketch mode).
//! Sketch mode is session state per document tab: which sketch is open,
//! and the camera it took over. Creating a sketch is a command; opening
//! one and finishing it are not.

use std::collections::BTreeMap;

use arrix_core::{FeatureId, Input, NameRoot, Ref, SlotName, TopoKind};
use arrix_doc::{Command, FeatureOutcome, FeatureRecord, FeatureTypeId, Part, SlotView};
use arrix_viewport::{CameraAnimation, CameraSample, OrbitCamera, Projection};
use web_time::Instant;

use crate::ArrixApp;
use crate::documents::OpenDocument;

/// How wide the view is, in metres from the centre, when there is no body
/// to frame a sketch against.
const EMPTY_RADIUS: f32 = 0.1;

/// The sketch being edited in a tab.
#[derive(Clone, Debug)]
pub struct SketchMode {
    pub feature: FeatureId,
    /// The camera as it was before the sketch turned it, set once the
    /// view has turned; finishing flies back to it.
    saved: Option<OrbitCamera>,
}

impl SketchMode {
    pub fn new(feature: FeatureId) -> Self {
        Self {
            feature,
            saved: None,
        }
    }

    /// Whether the camera has turned to the sketch's plane yet.
    pub fn turned(&self) -> bool {
        self.saved.is_some()
    }
}

impl OpenDocument {
    /// Leaves sketch mode, if in it, and flies the camera back to where it
    /// was before the sketch turned it.
    pub fn leave_sketch(&mut self) {
        let Some(mode) = self.sketch.take() else {
            return;
        };
        if let Some(saved) = mode.saved {
            let camera = &mut self.view.camera;
            camera.animation = Some(CameraAnimation::from_samples(
                CameraSample::new(camera.target, camera.distance, camera.yaw, camera.pitch),
                CameraSample::new(saved.target, saved.distance, saved.yaw, saved.pitch),
                Instant::now(),
                CameraAnimation::DEFAULT_DURATION,
            ));
            camera.projection = saved.projection;
            camera.near = saved.near;
            camera.far = saved.far;
        }
    }
}

/// The feature a topological name is rooted at.
fn root_feature(name: &arrix_core::PersistentName) -> FeatureId {
    match &name.root {
        NameRoot::Sweep { feature, .. }
        | NameRoot::Revolve { feature, .. }
        | NameRoot::Plugin { feature, .. }
        | NameRoot::Frozen { feature, .. } => *feature,
    }
}

impl ArrixApp {
    /// `core.sketch.create`: a sketch on the selected planar face or datum
    /// plane, else on the world XY plane; in a document with no part, in a
    /// new one. One command.
    pub(crate) fn create_sketch(&mut self) {
        let Some(document) = self.documents.get(self.active) else {
            return;
        };
        let doc = document.replica.document();
        let face = document
            .view
            .selection
            .as_ref()
            .filter(|n| n.kind == TopoKind::Face);
        let datum = document.view.feature_selection.filter(|id| {
            doc.parts()
                .values()
                .filter_map(|p| p.features.get(id))
                .any(|r| r.type_id.as_str() == "core.datum-plane")
        });
        let (input, near) = match (face, datum) {
            (Some(name), _) => (
                Some(Input::One(Ref::Topo(name.clone()))),
                Some(root_feature(name)),
            ),
            (None, Some(feature)) => (
                Some(Input::One(Ref::Slot {
                    feature,
                    slot: SlotName::new("plane").expect("a literal slot name"),
                })),
                Some(feature),
            ),
            (None, None) => (None, None),
        };
        let part = near
            .and_then(|f| doc.parts().values().find(|p| p.features.contains_key(&f)))
            .or_else(|| doc.parts().values().next());
        let name = {
            let used = |n: &str| part.is_some_and(|p| p.features.values().any(|r| r.name == n));
            (1..)
                .map(|i| format!("Sketch {i}"))
                .find(|n| !used(n))
                .expect("an unbounded range has a free name")
        };
        let version = self
            .feature_types
            .iter()
            .find(|t| t.id == "core.sketch")
            .map_or(1, |t| t.version);
        let feature = FeatureId(self.ids.next_id());
        let record = FeatureRecord {
            id: feature,
            type_id: FeatureTypeId::new("core.sketch").expect("a literal type id"),
            type_version: version,
            name,
            params: BTreeMap::new(),
            choices: BTreeMap::new(),
            inputs: input.into_iter().map(|i| ("plane".to_owned(), i)).collect(),
            suppressed: false,
            sketch: Some(Box::new(arrix_sketch::Sketch::new())),
            frozen: None,
        };
        let command = match part {
            Some(part) => Command::AddFeature {
                part: part.id,
                at: part.history.len(),
                record,
            },
            None => {
                let id = arrix_core::PartId(self.ids.next_id());
                Command::Group {
                    label: "Create sketch".into(),
                    commands: vec![
                        Command::AddPart {
                            part: Part {
                                id,
                                name: "Part 1".into(),
                                history: Vec::new(),
                                features: BTreeMap::new(),
                                rollback: None,
                            },
                        },
                        Command::AddFeature {
                            part: id,
                            at: 0,
                            record,
                        },
                    ],
                }
            }
        };
        let Some(document) = self.documents.get_mut(self.active) else {
            return;
        };
        match document.submit(command) {
            Ok(_) => {
                document.sketch = Some(SketchMode::new(feature));
                document.view.feature_selection = Some(feature);
            }
            Err(reason) => self.session.notice = Some(reason),
        }
    }

    /// `core.sketch.edit`: opens the tree's selected sketch. No command.
    pub(crate) fn edit_sketch(&mut self) {
        if let Some(feature) = self.selected_sketch() {
            self.open_sketch(feature);
        }
    }

    /// The tree's selection if it is a sketch with a sketch in it.
    pub(crate) fn selected_sketch(&self) -> Option<FeatureId> {
        let document = self.active_document()?;
        let feature = document.view.feature_selection?;
        document
            .replica
            .document()
            .parts()
            .values()
            .filter_map(|p| p.features.get(&feature))
            .any(|r| r.type_id.as_str() == "core.sketch")
            .then_some(feature)
    }

    fn open_sketch(&mut self, feature: FeatureId) {
        if let Some(document) = self.documents.get_mut(self.active) {
            document.sketch = Some(SketchMode::new(feature));
        }
    }

    /// A double-click on a tree row: a sketch opens in sketch mode, a datum
    /// plane gets a sketch (the selection is the row).
    pub(crate) fn activate_feature(&mut self, feature: FeatureId) {
        let Some(document) = self.documents.get_mut(self.active) else {
            return;
        };
        if document.sketch.is_some() {
            self.session.notice = Some("Finish the sketch first.".into());
            return;
        }
        document.view.feature_selection = Some(feature);
        let kind = document
            .replica
            .document()
            .parts()
            .values()
            .find_map(|p| p.features.get(&feature))
            .map(|r| r.type_id.as_str().to_owned());
        match kind.as_deref() {
            Some("core.sketch") => self.open_sketch(feature),
            Some("core.datum-plane") => self.create_sketch(),
            _ => {}
        }
    }

    /// `core.sketch.finish`: leaves sketch mode and flies the camera back.
    /// Issues no command.
    pub(crate) fn finish_sketch(&mut self) {
        if let Some(document) = self.documents.get_mut(self.active) {
            document.leave_sketch();
        }
    }

    /// Turns each tab's camera square to its sketch's plane, in
    /// orthographic projection, once the plane is known: the evaluation of
    /// a new sketch arrives a frame or so after its command.
    pub(crate) fn turn_to_sketches(&mut self) {
        for document in &mut self.documents {
            let Some(mode) = &mut document.sketch else {
                continue;
            };
            if mode.turned() {
                // An undo took the sketch away: there is nothing to edit.
                let held = document
                    .replica
                    .document()
                    .parts()
                    .values()
                    .any(|p| p.features.contains_key(&mode.feature));
                if !held {
                    document.leave_sketch();
                }
                continue;
            }
            let Some((_, FeatureOutcome::Ok { slots, .. })) =
                document.replica.outcome(mode.feature)
            else {
                continue;
            };
            let Some(SlotView::Sketch(view)) =
                slots.values().find(|s| matches!(s, SlotView::Sketch(_)))
            else {
                continue;
            };
            let plane = view.plane;
            let camera = &mut document.view.camera;
            mode.saved = Some(camera.clone());
            let normal = plane.z_axis().as_vec3();
            let pitch = normal.z.clamp(-1.0, 1.0).asin();
            // Looking along Z leaves the yaw free: keep the current one.
            let yaw = if normal.x.hypot(normal.y) < 1e-6 {
                camera.yaw
            } else {
                normal.y.atan2(normal.x)
            };
            let origin = plane.origin().as_vec3();
            let radius = match document.view.scene.bounding_sphere() {
                Some((centre, radius)) => (centre - origin).length() + radius,
                None => EMPTY_RADIUS,
            };
            camera.projection = Projection::Orthographic;
            camera.animate_home_with_orientation(origin, radius.max(EMPTY_RADIUS), yaw, pitch);
        }
    }
}
