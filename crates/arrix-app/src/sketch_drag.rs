//! Dragging a point or a curve in the select tool (docs/UI-RENDERING.md
//! §Sketch mode). The press starts a `DragSession` over the solved draft;
//! each frame steps it to the cursor on the UI thread, the overlay drawing
//! the draft as the solver leaves it; the release is one `SketchEdit`,
//! diffed from the draft as it was before the press, and none when nothing
//! moved. A frame the solver cannot follow leaves the geometry where it
//! was and the status line says so.

use arrix_doc::Command;
use arrix_sketch::{DragSession, Sketch, SketchEdit, SketchEntityId};
use arrix_ui::{HitKind, PlanePointer, SketchHit};
use web_time::Instant;

use crate::ArrixApp;
use crate::sketch_tools::Tool;
use crate::sketch_view::Solved;

/// A drag under way: session state, per open sketch.
#[derive(Clone, Debug)]
pub struct ActiveDrag {
    session: DragSession,
    /// The draft as the press found it: what the release diffs against.
    before: Sketch,
    /// What is dragged, for the status line.
    what: &'static str,
    /// Frames stepped so far.
    pub frames: usize,
    /// The last frame's solve, in microseconds.
    pub solve_us: u64,
    /// The last frame could not follow the cursor.
    pub blocked: bool,
}

impl ActiveDrag {
    /// The status line's words while the drag is on.
    pub fn hint(&self) -> String {
        match self.blocked {
            true => format!(
                "Dragging a {}: it cannot go further, the constraints hold it there.",
                self.what
            ),
            false => format!("Dragging a {}.", self.what),
        }
    }
}

impl ArrixApp {
    /// The button went down on `hit` and moved: with the select tool, a
    /// point or a curve starts a drag. A fixed point does not move, and a
    /// glyph or a dimension is not dragged.
    pub(crate) fn drag_started(&mut self, hit: Option<SketchHit>, pointer: Option<PlanePointer>) {
        let Some(mode) = self.active_sketch_mut() else {
            return;
        };
        let (Some(solved), Some(hit), Some(pointer)) = (&mode.solved, hit, pointer) else {
            return;
        };
        if mode.drawing.tool != Tool::Select {
            return;
        }
        let Ok(id) = hit.id.parse::<SketchEntityId>() else {
            return;
        };
        let sketch = &solved.sketch;
        let (session, what) = match hit.kind {
            HitKind::Point => match sketch.point(id) {
                Some(p) if !p.fixed => (DragSession::new(sketch, id), "point"),
                _ => return,
            },
            HitKind::Curve => (DragSession::new_entity(sketch, id, pointer.at), "curve"),
            HitKind::Glyph | HitKind::Dimension => return,
        };
        mode.drag = Some(ActiveDrag {
            session,
            before: sketch.clone(),
            what,
            frames: 0,
            solve_us: 0,
            blocked: false,
        });
        mode.hover = None;
    }

    /// One frame of the drag: the solver steps toward the pointer, and the
    /// draft the overlay draws is what it left.
    pub(crate) fn drag_moved(&mut self, pointer: PlanePointer) {
        let Some(mode) = self.active_sketch_mut() else {
            return;
        };
        let (Some(drag), Some(solved)) = (&mut mode.drag, &mut mode.solved) else {
            return;
        };
        let started = Instant::now();
        let frame = drag
            .session
            .step(&mut solved.sketch, pointer.at[0], pointer.at[1]);
        drag.solve_us = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
        drag.frames += 1;
        drag.blocked = frame.blocked || !frame.converged;
    }

    /// The button went up: what the drag moved is one `SketchEdit`, or
    /// nothing was moved and nothing is issued.
    pub(crate) fn drag_ended(&mut self) {
        let Some(document) = self.documents.get_mut(self.active) else {
            return;
        };
        let base = document.replica.generation();
        let Some(mode) = &mut document.sketch else {
            return;
        };
        let (Some(drag), Some(solved)) = (mode.drag.take(), &mut mode.solved) else {
            return;
        };
        let edit = SketchEdit::diff(&drag.before, &solved.sketch);
        if edit.is_empty() {
            return;
        }
        let after = solved.sketch.clone();
        let command = Command::SketchEdit {
            feature: mode.feature,
            edit,
        };
        match document.submit(command) {
            Ok(_) => {
                if let Some(mode) = &mut document.sketch {
                    mode.solved = Some(Solved::local(base, after));
                }
            }
            Err(reason) => {
                if let Some(solved) = document.sketch.as_mut().and_then(|m| m.solved.as_mut()) {
                    solved.sketch = drag.before;
                }
                self.session.notice = Some(reason);
            }
        }
    }
}
