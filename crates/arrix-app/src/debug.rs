//! `debug_state()`: JSON of what the app drew and why (docs/UI-RENDERING.md
//! §Visual debugging). It is half of every snapshot golden, so its fields
//! are stable, ordered, and its floats rounded to six places.

use serde::Serialize;

use crate::ArrixApp;
use arrix_config::{Binding, ConfigDiagnostic};
use arrix_ui::ShellView;

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
    /// The active document's bodies as the scene holds them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bodies: Vec<BodyDebug>,
    /// The camera the active view looks through.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera: Option<CameraDebug>,
    /// The keymap in force: each key, its command and its context.
    pub keymap: Vec<Binding>,
    /// What the config files got wrong, each with its file and line.
    pub config_diagnostics: Vec<ConfigDiagnostic>,
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
}

#[derive(Debug, Serialize)]
pub struct CameraDebug {
    pub target: [f64; 3],
    pub distance: f64,
    pub yaw: f64,
    pub pitch: f64,
    pub projection: String,
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

fn round6(x: f32) -> f64 {
    (f64::from(x) * 1e6).round() / 1e6
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
                bodies: Vec::new(),
                camera: None,
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
            bodies: self.active_document().map_or_else(Vec::new, |d| {
                d.view
                    .scene
                    .entries()
                    .map(|e| BodyDebug {
                        part: e.key.part.0.to_string(),
                        feature: e.key.feature.0.to_string(),
                        slot: e.key.slot.to_string(),
                        chord_um: round6((e.chord * 1e6) as f32),
                        triangles: e.mesh.index_count / 3,
                        bounds: e.world_bounds().map(|b| {
                            [b.min.x, b.min.y, b.min.z, b.max.x, b.max.y, b.max.z].map(round6)
                        }),
                    })
                    .collect()
            }),
            camera: self.active_document().map(|d| {
                let c = &d.view.camera;
                CameraDebug {
                    target: [c.target.x, c.target.y, c.target.z].map(round6),
                    distance: round6(c.distance),
                    yaw: round6(c.yaw),
                    pitch: round6(c.pitch),
                    projection: c.projection.label().to_owned(),
                }
            }),
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
