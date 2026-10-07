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
use crate::sketch_view::{SketchOverlay, Solved, sketch_view};

/// How wide the view is, in metres from the centre, when there is no body
/// to frame a sketch against.
const EMPTY_RADIUS: f64 = 0.1;

/// The least a sketch's view is fitted to, in metres, so a lone point is
/// not zoomed into.
const MIN_RADIUS: f64 = 0.01;

/// The centre and half-diagonal, with a margin, of a sketch's points, in
/// plane-local metres; `None` for a sketch with none.
fn extent(sketch: &arrix_sketch::Sketch) -> Option<([f64; 2], f64)> {
    let mut points = sketch.points().values().map(arrix_sketch::Point::pos);
    let first = points.next()?;
    let (lo, hi) = points.fold((first, first), |(lo, hi), p| {
        (
            [lo[0].min(p[0]), lo[1].min(p[1])],
            [hi[0].max(p[0]), hi[1].max(p[1])],
        )
    });
    let centre = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
    Some((centre, 0.65 * (hi[0] - lo[0]).hypot(hi[1] - lo[1])))
}

/// The sketch being edited in a tab.
#[derive(Clone, Debug)]
pub struct SketchMode {
    pub feature: FeatureId,
    /// The camera as it was before the sketch turned it, set once the
    /// view has turned; finishing flies back to it.
    saved: Option<OrbitCamera>,
    /// The sketch as last evaluated and solved, which the overlay draws;
    /// made again when a newer evaluation of it arrives.
    pub solved: Option<Solved>,
    /// The plane it stands on, as last evaluated.
    pub plane: Option<arrix_core::Frame>,
}

impl SketchMode {
    pub fn new(feature: FeatureId) -> Self {
        Self {
            feature,
            saved: None,
            solved: None,
            plane: None,
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

    /// Keeps each tab's open sketch in step with its evaluation: the
    /// solved draft is made again when a newer evaluation of the sketch
    /// arrives, and the camera turns square to its plane, in orthographic
    /// projection, the first time one has (the evaluation of a new sketch
    /// arrives a frame or so after its command). A sketch an undo took
    /// away ends sketch mode.
    pub(crate) fn follow_sketches(&mut self) {
        for document in &mut self.documents {
            let Some(mode) = &mut document.sketch else {
                continue;
            };
            if mode.turned() {
                let held = document
                    .replica
                    .document()
                    .parts()
                    .values()
                    .any(|p| p.features.contains_key(&mode.feature));
                if !held {
                    document.leave_sketch();
                    continue;
                }
            }
            let Some((generation, FeatureOutcome::Ok { slots, .. })) =
                document.replica.outcome(mode.feature)
            else {
                continue;
            };
            let Some(SlotView::Sketch(view)) =
                slots.values().find(|s| matches!(s, SlotView::Sketch(_)))
            else {
                continue;
            };
            if mode
                .solved
                .as_ref()
                .is_none_or(|s| s.generation != generation)
            {
                mode.solved = Some(Solved::new(generation, view.sketch.clone()));
                mode.plane = Some(view.plane);
            }
            if mode.turned() {
                continue;
            }
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
            // Fits the sketch's own extent; an empty one gets a default view
            // from the plane's origin.
            let (centre, radius) = mode
                .solved
                .as_ref()
                .and_then(|s| extent(&s.sketch))
                .map_or(([0.0, 0.0], EMPTY_RADIUS), |(c, r)| (c, r.max(MIN_RADIUS)));
            let at = plane.point(arrix_core::DVec2::new(centre[0], centre[1]));
            camera.projection = Projection::Orthographic;
            camera.animate_home_with_orientation(at.as_vec3(), radius as f32, yaw, pitch);
        }
    }

    /// The degrees of freedom and solver state of the shown document's open
    /// sketch, for the status line.
    pub(crate) fn sketch_status(&self) -> Option<arrix_ui::SketchStatusView> {
        let solved = self.active_document()?.sketch.as_ref()?.solved.as_ref()?;
        Some(arrix_ui::SketchStatusView {
            dof: solved.diagnostics.dof,
            state: crate::sketch_view::solver_state(&solved.diagnostics),
        })
    }

    /// What the viewport draws of the shown document's open sketch.
    pub(crate) fn sketch_overlay(&self) -> Option<SketchOverlay> {
        let mode = self.active_document()?.sketch.as_ref()?;
        Some(SketchOverlay {
            view: sketch_view(mode.solved.as_ref()?),
            plane: mode.plane?,
        })
    }
}
