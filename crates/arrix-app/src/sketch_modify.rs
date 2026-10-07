//! The modify tools: trim and extend (docs/UI-RENDERING.md §Sketch mode).
//! Hovering a curve previews, as a polyline, the piece a trim removes or
//! the stretch an extension adds; both come from `arrix_sketch::modify`'s
//! previews, which read the same pieces and the same first hit the edit
//! uses, so the click does what the hover showed. A click that changes the
//! sketch is one `SketchEdit` diffed from the solved draft before it.

use arrix_sketch::modify::{extend, extend_preview, pick_curve, trim, trim_preview};
use arrix_sketch::{Draft, Sketch};
use arrix_ui::{CurveShape, PlanePointer, SketchPreviewView};

use crate::ArrixApp;
use crate::sketch_tools::{Drawing, Tool};

impl Drawing {
    /// What the pointer's trim or extend would touch: the curve under it,
    /// and the stretch the click removes or adds.
    pub fn modify_preview(&self, sketch: &Sketch) -> Option<SketchPreviewView> {
        let pointer = self.pointer?;
        let entity = pick_curve(sketch, pointer.at, pointer.tol)?;
        let line = match self.tool {
            Tool::Trim => trim_preview(sketch, entity, pointer.at)?,
            Tool::Extend => extend_preview(sketch, entity, pointer.at)?,
            _ => return None,
        };
        Some(SketchPreviewView {
            tool: self.tool.command().to_owned(),
            cursor: round(pointer.at),
            curves: line
                .windows(2)
                .map(|w| CurveShape::Line {
                    from: round(w[0]),
                    to: round(w[1]),
                })
                .collect(),
            glyphs: Vec::new(),
            guides: Vec::new(),
            acquired: Vec::new(),
        })
    }
}

fn round(p: [f64; 2]) -> [f64; 2] {
    p.map(|x| (x * 1e9).round() / 1e9)
}

impl ArrixApp {
    /// A click with the trim or extend tool: the curve under it is cut or
    /// reaches out, as one `SketchEdit`. A click on nothing, or on what the
    /// edit refuses, issues nothing; a refusal says why in the status line.
    pub(crate) fn modify_click(&mut self, pointer: PlanePointer) {
        let ids = self.ids.clone();
        let Some(mode) = self.active_sketch_mut() else {
            return;
        };
        let Some(solved) = &mode.solved else {
            return;
        };
        let tool = mode.drawing.tool;
        let before = solved.sketch.clone();
        let Some(entity) = pick_curve(&before, pointer.at, pointer.tol) else {
            return;
        };
        let mut draft = Draft::with_sketch(before.clone(), ids);
        let done = match tool {
            Tool::Trim => trim(&mut draft, entity, pointer.at).map(drop),
            _ => extend(&mut draft, entity, pointer.at).map(drop),
        };
        if let Err(refused) = done {
            self.session.notice = Some(format!("{refused}."));
            return;
        }
        if !draft.solve().converged {
            self.session.notice =
                Some("The sketch did not solve with that; nothing was changed.".into());
            return;
        }
        self.submit_draft(&before, draft);
    }
}
