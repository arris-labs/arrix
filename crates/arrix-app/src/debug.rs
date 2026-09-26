//! `debug_state()`: JSON of what the app drew and why (docs/UI-RENDERING.md
//! §Visual debugging). It is half of every snapshot golden, so its fields
//! are stable, ordered, and its floats rounded to six places.

use serde::Serialize;

use crate::ArrixApp;
use arrix_ui::ShellView;

/// What the last frame drew. `None` fields mean no frame has run yet.
#[derive(Debug, Serialize)]
pub struct DebugState {
    /// The window, `[width, height]` in points.
    pub window: Option<[f64; 2]>,
    pub layout: Option<LayoutDebug>,
    pub shell: Option<ShellView>,
}

/// Each shell region as `[min_x, min_y, max_x, max_y]` in points.
#[derive(Debug, Serialize)]
pub struct LayoutDebug {
    pub ribbon: [f64; 4],
    pub tree: [f64; 4],
    pub viewport: [f64; 4],
    pub properties: [f64; 4],
    pub status: [f64; 4],
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
                shell: None,
            };
        };
        let l = &drawn.layout;
        DebugState {
            window: Some([round6(drawn.window.width()), round6(drawn.window.height())]),
            layout: Some(LayoutDebug {
                ribbon: rect(l.ribbon),
                tree: rect(l.tree),
                viewport: rect(l.viewport),
                properties: rect(l.properties),
                status: rect(l.status),
            }),
            shell: Some(drawn.view.clone()),
        }
    }

    /// [`Self::debug_state`] as pretty JSON: what goldens and the scratch
    /// render hold.
    pub fn debug_state_json(&self) -> String {
        serde_json::to_string_pretty(&self.debug_state())
            .unwrap_or_else(|err| format!("{{\"error\": \"{err}\"}}"))
    }
}
