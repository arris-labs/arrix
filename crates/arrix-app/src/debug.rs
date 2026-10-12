//! `debug_state()`: JSON of what the app drew and why (docs/UI-RENDERING.md
//! §Visual debugging). It is half of every snapshot golden, so its fields
//! are stable, ordered, and its floats rounded to six places.

use serde::Serialize;

use crate::ArrixApp;
use arrix_config::{Binding, ConfigDiagnostic};
use arrix_core::PersistentName;
use arrix_ui::ShellView;
use arrix_viewport::{BodyEntry, GpuMesh};

/// What the last frame drew. `None` fields mean no frame has run yet.
#[derive(Debug, Serialize)]
pub struct DebugState {
    /// The window, `[width, height]` in points.
    pub window: Option<[f64; 2]>,
    pub layout: Option<LayoutDebug>,
    /// The effective theme's name: what `ui.theme` got, or `dark` when it
    /// could not be had.
    pub theme: String,
    pub shell: Option<ShellView>,
    /// The active document's evaluation, once one is open.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evaluation: Option<EvaluationDebug>,
    /// What the active tab looks at (ADR-0030 §1); present once the
    /// document has an assembly, since a part's is then the only choice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<ContextDebug>,
    /// The active document's bodies as the scene holds them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bodies: Vec<BodyDebug>,
    /// In an assembly's context, what is drawn: each placed component's
    /// body at its world pose, one upload per body however many there are.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub instances: Vec<InstanceDebug>,
    /// The create form's preview in the active document, while one is
    /// asked for (ADR-0019).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<PreviewDebug>,
    /// The joint a left drag is moving, and its trial value (ADR-0030 §3).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub joint_drag: Option<crate::joint_drag_app::JointDragDebug>,
    /// The camera the active view looks through.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera: Option<CameraDebug>,
    /// What is selected in the active view, by persistent name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection: Option<PersistentName>,
    /// The component ids from the assembly the selection is of; absent in
    /// a part's context.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selection_path: Vec<String>,
    /// As `selection_path`, for the hover.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hover_path: Vec<String>,
    /// What the open form marks in the scene, drawn as the selection is:
    /// its fields' references, and an edit form's diagnostic's references
    /// and candidates.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub marked: Vec<PersistentName>,
    /// What the pointer is over in the active view, by persistent name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hover: Option<PersistentName>,
    /// A pick is on its way back from the GPU.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub picking: bool,
    /// The sketch open in the active document, in sketch mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sketch: Option<SketchModeDebug>,
    /// What the viewport draws of it, in plane-local metres.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sketch_view: Option<arrix_ui::SketchView>,
    /// The finished sketches drawn in part mode.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub part_sketches: Vec<PartSketchDebug>,
    /// How many commands the last gesture submitted (SEED.md §8.2: one,
    /// or none where nothing changed); absent before any gesture.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gesture_commands: Option<usize>,
    /// The closed gestures, the latest last and at most the last 256: what
    /// opened each, its document and the commands it applied.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub gestures: Vec<crate::Gesture>,
    /// The commands the active document has applied, undo and redo
    /// included: what its gestures' counts add up to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commands_applied: Option<usize>,
    /// The frozen badges the viewport drew, in points.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub badges: Vec<arrix_ui::BadgeView>,
    /// The keymap in force: each key, its command and its context.
    pub keymap: Vec<Binding>,
    /// What the config files got wrong, each with its file and line.
    pub config_diagnostics: Vec<ConfigDiagnostic>,
}

/// A finished sketch drawn over the part.
#[derive(Debug, Serialize)]
pub struct PartSketchDebug {
    pub feature: String,
    pub points: usize,
    pub curves: usize,
    /// Region keys are in the view only while a field asks for one.
    pub regions: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hover_region: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub selected_regions: Vec<String>,
}

/// A create form's preview.
#[derive(Debug, Serialize)]
pub struct PreviewDebug {
    /// Its outcome has been heard; until then the form says evaluating.
    pub heard: bool,
    /// Every body it changed is drawn refined.
    pub settled: bool,
    /// The bodies it draws, tinted, in place of the document's.
    pub bodies: Vec<BodyDebug>,
    /// The document's bodies it hides, by `part/feature/slot`.
    pub replaced: Vec<String>,
}

/// The sketch the active tab is editing.
#[derive(Debug, Serialize)]
pub struct SketchModeDebug {
    pub feature: String,
    /// The camera has turned square to the sketch's plane.
    pub turned: bool,
    /// The drag under way: how many frames it has stepped and what the
    /// last one's solve cost. Present only while the button is down.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drag: Option<DragDebug>,
}

/// A drag in progress.
#[derive(Debug, Serialize)]
pub struct DragDebug {
    pub frames: usize,
    pub solve_us: u64,
    /// The last frame could not follow the cursor.
    pub blocked: bool,
}

/// Where the active document's evaluation is.
#[derive(Debug, Serialize)]
pub struct EvaluationDebug {
    pub document: String,
    pub generation: u64,
    /// The latest generation's evaluation has finished.
    pub finished: bool,
    /// Everything it will say has been heard: every body meshed refined.
    pub settled: bool,
    /// What went wrong beside the document's own diagnostics.
    pub problems: Vec<String>,
}

/// What a tab looks at.
#[derive(Debug, Serialize)]
pub struct ContextDebug {
    /// `part` or `assembly`.
    pub kind: String,
    pub id: String,
    pub name: String,
}

/// One instance of a body: where a component's part is drawn.
#[derive(Debug, Serialize)]
pub struct InstanceDebug {
    /// The component ids from the assembly down to the part.
    pub path: Vec<String>,
    pub part: String,
    pub feature: String,
    pub slot: String,
    /// Where the part's origin and axes land in the world, in metres.
    pub origin: [f64; 3],
    pub x_axis: [f64; 3],
    pub z_axis: [f64; 3],
}

/// One body in the scene.
#[derive(Debug, Serialize)]
pub struct BodyDebug {
    pub part: String,
    pub feature: String,
    pub slot: String,
    /// The chord of the mesh drawn, in micrometres: refined is finer than
    /// coarse.
    pub chord_um: f64,
    pub triangles: u32,
    /// `[min_x, min_y, min_z, max_x, max_y, max_z]` in the world, metres.
    pub bounds: Option<[f64; 6]>,
    /// The colour its material gives it, red, green and blue in 0 to 1;
    /// absent for the viewport's own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub colour: Option<[f32; 3]>,
}

#[derive(Debug, Serialize)]
pub struct CameraDebug {
    pub target: [f64; 3],
    pub distance: f64,
    pub yaw: f64,
    pub pitch: f64,
    pub projection: String,
    /// The view-projection matrix at the viewport's aspect, column by
    /// column: what the shaders draw with.
    pub view_proj: [f64; 16],
}

/// Each shell region as `[min_x, min_y, max_x, max_y]` in points; a card
/// that is not showing is `null`.
#[derive(Debug, Serialize)]
pub struct LayoutDebug {
    pub ribbon: [f64; 4],
    pub tabs: [f64; 4],
    pub viewport: [f64; 4],
    pub status: [f64; 4],
    pub tree: Option<[f64; 4]>,
    pub properties: Option<[f64; 4]>,
}

fn body_debug(e: &BodyEntry<GpuMesh>) -> BodyDebug {
    BodyDebug {
        part: e.key.part.0.to_string(),
        feature: e.key.feature.0.to_string(),
        slot: e.key.slot.to_string(),
        chord_um: round6((e.chord * 1e6) as f32),
        triangles: e.mesh.index_count / 3,
        bounds: e
            .world_bounds()
            .map(|b| [b.min.x, b.min.y, b.min.z, b.max.x, b.max.y, b.max.z].map(round6)),
        colour: e.colour,
    }
}

/// Poses are compared to a nanometre: six places would hide a drift.
fn round9(x: f64) -> f64 {
    (x * 1e9).round() / 1e9
}

fn context_debug(d: &crate::documents::OpenDocument) -> Option<ContextDebug> {
    let doc = d.replica.document();
    if doc.assemblies().is_empty() {
        return None;
    }
    let context = d.context()?;
    let (name, id) = context.names(doc)?;
    let kind = match context {
        crate::context::Context::Part(_) => "part",
        crate::context::Context::Assembly(_) => "assembly",
    };
    Some(ContextDebug {
        kind: kind.to_owned(),
        id,
        name,
    })
}

fn round6(x: f32) -> f64 {
    (f64::from(x) * 1e6).round() / 1e6
}

fn view_proj(camera: &arrix_viewport::OrbitCamera, viewport: egui::Rect) -> [f64; 16] {
    let aspect = viewport.width().max(1.0) / viewport.height().max(1.0);
    camera.view_proj(aspect).to_cols_array().map(round6)
}

fn rect(r: egui::Rect) -> [f64; 4] {
    [
        round6(r.min.x),
        round6(r.min.y),
        round6(r.max.x),
        round6(r.max.y),
    ]
}

impl ArrixApp {
    pub fn debug_state(&self) -> DebugState {
        let Some(drawn) = &self.drawn else {
            return DebugState {
                window: None,
                layout: None,
                theme: self.theme.name.clone(),
                shell: None,
                evaluation: None,
                context: None,
                bodies: Vec::new(),
                instances: Vec::new(),
                preview: None,
                joint_drag: None,
                camera: None,
                selection: None,
                selection_path: Vec::new(),
                hover_path: Vec::new(),
                marked: Vec::new(),
                hover: None,
                picking: false,
                sketch: None,
                sketch_view: None,
                part_sketches: Vec::new(),
                badges: Vec::new(),
                gesture_commands: self.session.gestures.last(),
                gestures: self.session.gestures.log().cloned().collect(),
                commands_applied: self.active_document().map(|d| d.applied()),
                keymap: self.keymap.bindings().to_vec(),
                config_diagnostics: self.config_diagnostics.clone(),
            };
        };
        let l = &drawn.layout;
        DebugState {
            theme: self.theme.name.clone(),
            window: Some([round6(drawn.window.width()), round6(drawn.window.height())]),
            layout: Some(LayoutDebug {
                ribbon: rect(l.ribbon),
                tabs: rect(l.tabs),
                viewport: rect(l.viewport),
                status: rect(l.status),
                tree: l.tree.map(rect),
                properties: l.properties.map(rect),
            }),
            shell: Some(drawn.view.clone()),
            evaluation: self.active_document().map(|d| EvaluationDebug {
                document: d.title.clone(),
                generation: d.replica.generation().0,
                finished: d.finished(),
                settled: d.settled(),
                problems: d.problems.clone(),
            }),
            context: self.active_document().and_then(context_debug),
            bodies: self.active_document().map_or_else(Vec::new, |d| {
                d.view.scene.entries().map(body_debug).collect()
            }),
            joint_drag: self
                .active_document()
                .and_then(crate::OpenDocument::joint_drag_debug),
            instances: self.active_document().map_or_else(Vec::new, |d| {
                let round = |v: arrix_core::DVec3| [v.x, v.y, v.z].map(round9);
                d.view
                    .scene
                    .instances()
                    .unwrap_or_default()
                    .iter()
                    .map(|i| InstanceDebug {
                        path: i.path.iter().map(|c| c.0.to_string()).collect(),
                        part: i.key.part.0.to_string(),
                        feature: i.key.feature.0.to_string(),
                        slot: i.key.slot.to_string(),
                        origin: round(i.pose.origin()),
                        x_axis: round(i.pose.x_axis()),
                        z_axis: round(i.pose.z_axis()),
                    })
                    .collect()
            }),
            preview: self
                .active_document()
                .filter(|d| d.preview.active())
                .map(|d| {
                    let scene = &d.view.scene;
                    PreviewDebug {
                        heard: d.preview.heard().is_some(),
                        settled: d.preview.heard().is_some_and(|h| h.settled()),
                        bodies: scene.preview().map(body_debug).collect(),
                        replaced: scene
                            .keys()
                            .filter(|k| scene.replaced(k))
                            .map(|k| format!("{}/{}/{}", k.part.0, k.feature.0, k.slot))
                            .collect(),
                    }
                }),
            camera: self.active_document().map(|d| {
                let c = &d.view.camera;
                CameraDebug {
                    target: [c.target.x, c.target.y, c.target.z].map(round6),
                    distance: round6(c.distance),
                    yaw: round6(c.yaw),
                    pitch: round6(c.pitch),
                    projection: c.projection.label().to_owned(),
                    view_proj: view_proj(c, drawn.layout.viewport),
                }
            }),
            selection: self
                .active_document()
                .and_then(|d| d.view.selection.as_ref().map(|h| h.name.clone())),
            marked: self.form_marks(),
            hover: self
                .active_document()
                .and_then(|d| d.view.hover.as_ref().map(|h| h.name.clone())),
            selection_path: self
                .active_document()
                .and_then(|d| d.view.selection.as_ref())
                .map_or_else(Vec::new, |h| path_ids(&h.path)),
            hover_path: self
                .active_document()
                .and_then(|d| d.view.hover.as_ref())
                .map_or_else(Vec::new, |h| path_ids(&h.path)),
            picking: self
                .active_document()
                .is_some_and(|d| d.view.pick.is_some()),
            sketch: self
                .active_document()
                .and_then(|d| d.sketch.as_ref())
                .map(|mode| SketchModeDebug {
                    feature: mode.feature.to_string(),
                    turned: mode.turned(),
                    drag: mode.drag.as_ref().map(|d| DragDebug {
                        frames: d.frames,
                        solve_us: d.solve_us,
                        blocked: d.blocked,
                    }),
                }),
            sketch_view: self.sketch_overlay().map(|o| o.view),
            part_sketches: self
                .part_overlays()
                .into_iter()
                .map(|o| PartSketchDebug {
                    feature: o.feature.to_string(),
                    points: o.view.points.len(),
                    curves: o.view.curves.len(),
                    regions: o.view.regions.len(),
                    hover_region: o.view.hover_region,
                    selected_regions: o.view.selected_regions,
                })
                .collect(),
            badges: drawn.badges.clone(),
            gesture_commands: self.session.gestures.last(),
            gestures: self.session.gestures.log().cloned().collect(),
            commands_applied: self.active_document().map(|d| d.applied()),
            keymap: self.keymap.bindings().to_vec(),
            config_diagnostics: self.config_diagnostics.clone(),
        }
    }

    /// [`Self::debug_state`] as pretty JSON: what goldens and the scratch
    /// render hold.
    pub fn debug_state_json(&self) -> String {
        serde_json::to_string_pretty(&self.debug_state())
            .unwrap_or_else(|err| format!("{{\"error\": \"{err}\"}}"))
    }
}

fn path_ids(path: &[arrix_core::ComponentId]) -> Vec<String> {
    path.iter().map(|c| c.0.to_string()).collect()
}
