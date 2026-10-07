//! The modify tools: trim and extend (docs/UI-RENDERING.md §Sketch mode).
//! Hovering a curve previews, as a polyline, the piece a trim removes or
//! the stretch an extension adds; both come from `arrix_sketch::modify`'s
//! previews, which read the same pieces and the same first hit the edit
//! uses, so the click does what the hover showed. A click that changes the
//! sketch is one `SketchEdit` diffed from the solved draft before it.

use arrix_core::{IdMinter, QuantityKind};
use arrix_sketch::modify::{
    MirrorAxis, curve_samples, extend, extend_preview, mirror, offset, offset_preview, pick_curve,
    trim, trim_preview,
};
use arrix_sketch::{Draft, Entity, Sketch, SketchEntityId};
use arrix_ui::{CurveShape, HitKind, PlanePointer, SketchHit, SketchPreviewView};

use crate::ArrixApp;
use crate::sketch_dimension::{FieldState, FieldTarget};
use crate::sketch_mode::SketchMode;
use crate::sketch_tools::{Drawing, Tool};

/// What an offset's field opens with: a distance to one side. Negative is
/// the other.
const OFFSET_START: &str = "1 mm";

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
        Some(shown(self.tool, pointer.at, &[line]))
    }
}

/// `lines` as a tool's preview: what the click would write or remove.
fn shown(tool: Tool, cursor: [f64; 2], lines: &[Vec<[f64; 2]>]) -> SketchPreviewView {
    SketchPreviewView {
        tool: tool.command().to_owned(),
        cursor: round(cursor),
        curves: lines
            .iter()
            .flat_map(|line| line.windows(2))
            .map(|w| CurveShape::Line {
                from: round(w[0]),
                to: round(w[1]),
            })
            .collect(),
        glyphs: Vec::new(),
        guides: Vec::new(),
        acquired: Vec::new(),
    }
}

/// The line the pointer rests on, as a mirror's axis: any line, a
/// construction one included.
fn axis_of(sketch: &Sketch, hit: &SketchHit) -> Option<SketchEntityId> {
    let id = hit
        .id
        .parse::<SketchEntityId>()
        .ok()
        .filter(|_| hit.kind == HitKind::Curve)?;
    sketch.entity(id).is_some_and(Entity::is_line).then_some(id)
}

/// The mirror of the selection about the line under the pointer, as the
/// click would write it: the edit run on a trial draft, its copies drawn.
pub fn mirror_preview(mode: &SketchMode, sketch: &Sketch) -> Option<SketchPreviewView> {
    if mode.drawing.tool != Tool::Mirror {
        return None;
    }
    let axis = axis_of(sketch, mode.hover.as_ref()?)?;
    let selection: Vec<_> = mode.selection.iter().copied().collect();
    let mut trial = Draft::with_sketch(sketch.clone(), IdMinter::new(0));
    let edit = mirror(&mut trial, &selection, MirrorAxis::Line(axis)).ok()?;
    let lines: Vec<_> = edit
        .pairs
        .iter()
        .filter_map(|(_, copy)| curve_samples(&trial, *copy))
        .collect();
    Some(shown(Tool::Mirror, mode.drawing.pointer?.at, &lines))
}

fn round(p: [f64; 2]) -> [f64; 2] {
    p.map(|x| (x * 1e9).round() / 1e9)
}

impl ArrixApp {
    /// A click with the trim or extend tool: the curve under it is cut or
    /// reaches out, as one `SketchEdit`. A click on nothing, or on what the
    /// edit refuses, issues nothing; a refusal says why in the status line.
    pub(crate) fn modify_click(&mut self, pointer: PlanePointer) {
        if self
            .active_sketch()
            .is_some_and(|m| m.drawing.tool == Tool::Offset)
        {
            self.open_offset_field(pointer);
            return;
        }
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

impl ArrixApp {
    /// A click with the offset tool: the chain under it gets its distance
    /// field, which previews the copy as it is typed. No command yet.
    fn open_offset_field(&mut self, pointer: PlanePointer) {
        let Some(mode) = self.active_sketch_mut() else {
            return;
        };
        let Some(entity) = mode
            .solved
            .as_ref()
            .and_then(|s| pick_curve(&s.sketch, pointer.at, pointer.tol))
        else {
            return;
        };
        mode.field = Some(FieldState {
            target: FieldTarget::Offset {
                entity,
                at: pointer.at,
            },
            text: OFFSET_START.to_owned(),
            error: None,
        });
    }

    /// The offset the open field's text would make, drawn over the sketch;
    /// nothing while the text is not a distance.
    pub(crate) fn offset_preview(&self) -> Option<SketchPreviewView> {
        let mode = self.active_sketch()?;
        let field = mode.field.as_ref()?;
        let FieldTarget::Offset { entity, at } = field.target else {
            return None;
        };
        let (distance, _) = self
            .typed_quantity(&field.text, QuantityKind::Length)
            .ok()?;
        let lines = offset_preview(&mode.solved.as_ref()?.sketch, entity, distance).ok()?;
        Some(shown(Tool::Offset, at, &lines))
    }

    /// `DimensionCommitted` for an offset's field: the distance is an
    /// expression, a parameter of the document may be named, and the copy is
    /// one `SketchEdit` whose distance rows keep the expression. A text that
    /// is not a distance, a distance the geometry cannot take or a solve that
    /// fails leaves the field open with the reason and issues nothing.
    pub(crate) fn commit_offset(&mut self, text: &str) {
        let ids = self.ids.clone();
        let Some(mode) = self.active_sketch() else {
            return;
        };
        let (
            Some(FieldState {
                target: FieldTarget::Offset { entity, .. },
                ..
            }),
            Some(solved),
        ) = (&mode.field, &mode.solved)
        else {
            return;
        };
        let before = solved.sketch.clone();
        let (distance, expr) = match self.typed_quantity(text, QuantityKind::Length) {
            Ok(typed) => typed,
            Err(reason) => return self.field_error(reason),
        };
        let mut draft = Draft::with_sketch(before.clone(), ids);
        if let Err(refused) = offset(&mut draft, *entity, distance, expr.as_deref()) {
            return self.field_error(format!("{refused}."));
        }
        if !draft.solve().converged {
            return self.field_error("The sketch does not solve with that offset.".into());
        }
        if self.submit_draft(&before, draft) {
            self.close_dimension_field();
        }
    }

    /// A click with the mirror tool: the selected curves are copied across
    /// the line under the pointer, one `SketchEdit`. With nothing selected,
    /// or on anything but a line, it issues nothing and says what to do.
    pub(crate) fn mirror_click(&mut self, hit: Option<SketchHit>) {
        let ids = self.ids.clone();
        let Some(mode) = self.active_sketch() else {
            return;
        };
        let Some(solved) = &mode.solved else {
            return;
        };
        let before = solved.sketch.clone();
        let selection: Vec<_> = mode.selection.iter().copied().collect();
        let Some(axis) = hit.as_ref().and_then(|h| axis_of(&before, h)) else {
            self.session.notice = Some("Click the line to mirror about.".into());
            return;
        };
        let mut draft = Draft::with_sketch(before.clone(), ids);
        if let Err(refused) = mirror(&mut draft, &selection, MirrorAxis::Line(axis)) {
            self.session.notice = Some(format!("{refused}."));
            return;
        }
        if !draft.solve().converged {
            self.session.notice =
                Some("The sketch did not solve with that; nothing was mirrored.".into());
            return;
        }
        if self.submit_draft(&before, draft) {
            self.set_tool(Tool::Select);
        }
    }
}
