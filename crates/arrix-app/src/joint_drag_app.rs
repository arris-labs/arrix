//! The app's side of the joint drag (ADR-0030 §3 and §4): a left drag on
//! a component in an assembly's view asks for a pick where the button went
//! down, moves the joint that places it each frame by model matrices only,
//! and its release is one gesture of one command, or of none where the
//! value is where it started. The math is `joint_drag.rs`.

use arrix_core::AssemblyId;
use arrix_doc::{Generation, PlacedComponent};
use arrix_viewport::Hit;
use serde::Serialize;

use crate::ArrixApp;
use crate::context::Context;
use crate::documents::OpenDocument;
use crate::joint_drag::{Reader, Target, Values, axis_frame, preview, release, target};
use crate::viewport::SceneDrag;

/// A left drag in an assembly's view, from the press to the release's
/// evaluation. Session state.
#[derive(Clone, Debug)]
pub enum JointDrag {
    /// The pick where the button went down is on its way; the pointer is
    /// at `at`.
    Picking { origin: egui::Pos2, at: egui::Pos2 },
    /// A joint is being moved.
    Moving(Box<Moving>),
    /// The drag moves nothing; the notice says why when there is a reason.
    Idle,
    /// Released with a command: the preview stays until `root`'s solve of a
    /// generation after `after` is heard, so nothing jumps back meanwhile.
    Released {
        root: AssemblyId,
        after: Generation,
        placed: Vec<PlacedComponent>,
    },
}

#[derive(Clone, Debug)]
pub struct Moving {
    root: AssemblyId,
    target: Target,
    values: Values,
    reader: Reader,
    /// The trial value as the release would commit it, and its SI value.
    text: String,
    si: f64,
    placed: Vec<PlacedComponent>,
}

/// The drag under way, as `debug_state()` reports it.
#[derive(Clone, Debug, Serialize)]
pub struct JointDragDebug {
    /// The joint's id and name.
    pub joint: String,
    pub name: String,
    /// The trial value as the release would commit it.
    pub value: String,
    /// The joint's coordinate at it, `value + offset` in SI: what forward
    /// kinematics was given.
    pub q: f64,
}

impl JointDrag {
    /// The placements to draw `root` with instead of its last solve's.
    pub fn shown(&self, looking: AssemblyId) -> Option<&[PlacedComponent]> {
        match self {
            Self::Moving(m) if m.root == looking => Some(&m.placed),
            Self::Released { root, placed, .. } if *root == looking => Some(placed),
            _ => None,
        }
    }
}

impl OpenDocument {
    /// `root`'s solve of `generation` was heard: a released drag's preview
    /// gives way to it once it is of the release's command.
    pub(crate) fn heard_assembled(&mut self, assembly: AssemblyId, generation: Generation) {
        if let Some(JointDrag::Released { root, after, .. }) = &self.view.joint_drag
            && *root == assembly
            && generation > *after
        {
            self.view.joint_drag = None;
        }
    }

    /// Whether a left drag in the view moves a joint: in an assembly's
    /// view, outside sketch mode.
    pub(crate) fn drags_joints(&self) -> bool {
        self.sketch.is_none() && matches!(self.context(), Some(Context::Assembly(_)))
    }

    /// The trial value the pointer at `at` reads, and the preview placed
    /// at it; nothing is uploaded, the instances only move.
    fn follow(&mut self, rect: egui::Rect, at: egui::Pos2) {
        let Some(JointDrag::Moving(moving)) = &mut self.view.joint_drag else {
            return;
        };
        let delta = moving.reader.read(&self.view.camera, rect, at);
        let (text, si) = moving.values.trial(moving.target.kind, delta);
        if text == moving.text && !moving.placed.is_empty() {
            return;
        }
        let q = si + moving.values.offset;
        let Some(placed) = preview(&self.replica, moving.root, &moving.target, q) else {
            return;
        };
        moving.text = text;
        moving.si = si;
        moving.placed = placed;
        self.refresh_view();
    }

    pub(crate) fn joint_drag_debug(&self) -> Option<JointDragDebug> {
        let Some(JointDrag::Moving(m)) = &self.view.joint_drag else {
            return None;
        };
        Some(JointDragDebug {
            joint: m.target.joint.0.to_string(),
            name: m.target.name.clone(),
            value: m.text.clone(),
            q: m.si + m.values.offset,
        })
    }
}

impl ArrixApp {
    /// The viewport's left drag, a step at a time.
    pub(crate) fn joint_drag_step(&mut self, step: SceneDrag) {
        let Some(rect) = self.drawn.as_ref().map(|d| d.layout.viewport) else {
            return;
        };
        let Some(document) = self.documents.get_mut(self.active) else {
            return;
        };
        match step {
            SceneDrag::Started(origin) => {
                document.view.joint_drag = Some(JointDrag::Picking { origin, at: origin });
            }
            SceneDrag::Moved(now) => match &mut document.view.joint_drag {
                Some(JointDrag::Picking { at, .. }) => *at = now,
                Some(JointDrag::Moving(_)) => document.follow(rect, now),
                _ => {}
            },
            SceneDrag::Ended => match document.view.joint_drag.take() {
                Some(JointDrag::Moving(moving)) => self.release_joint_drag(*moving),
                // Released before the pick came back, or on nothing that
                // drags: nothing to commit.
                _ => document.refresh_view(),
            },
        }
    }

    /// The pick where the drag began came back: the joint it moves, or the
    /// notice why none does.
    pub(crate) fn begin_joint_drag(&mut self, hit: Option<Hit>) {
        let Some(rect) = self.drawn.as_ref().map(|d| d.layout.viewport) else {
            return;
        };
        let Some(document) = self.documents.get_mut(self.active) else {
            return;
        };
        let Some(JointDrag::Picking { origin, at }) = document.view.joint_drag else {
            return;
        };
        document.view.joint_drag = Some(JointDrag::Idle);
        let (Some(Context::Assembly(root)), Some(hit)) = (document.context(), hit) else {
            return;
        };
        if hit.path.is_empty() {
            return;
        }
        let started = (|| {
            let target = target(&document.replica, root, &hit.path)?;
            let values = Values::of(&document.replica, &target)?;
            let axis = axis_frame(&document.replica, root, &target)
                .ok_or_else(|| format!("Joint {} is not placed.", target.name))?;
            let camera = &document.view.camera;
            let reader = Reader::start(target.kind, &axis, camera, rect, origin, &target.name)?;
            Ok::<_, String>(Moving {
                root,
                target,
                values,
                reader,
                text: String::new(),
                si: 0.0,
                placed: Vec::new(),
            })
        })();
        match started {
            Ok(moving) => {
                document.view.joint_drag = Some(JointDrag::Moving(Box::new(moving)));
                document.follow(rect, at);
            }
            Err(why) => self.session.notice = Some(why),
        }
    }

    /// The release: one gesture, one command for a value that moved, none
    /// for one where it started.
    fn release_joint_drag(&mut self, moving: Moving) {
        self.gesture("joint drag", |app| {
            let Some(document) = app.documents.get_mut(app.active) else {
                return;
            };
            let command = release(
                document.replica.document(),
                &moving.target,
                &moving.values,
                &moving.text,
                moving.si,
            );
            let after = document.replica.generation();
            let applied = match command {
                Some(command) => document.submit(command),
                None => Ok(false),
            };
            match applied {
                Ok(true) => {
                    document.view.joint_drag = Some(JointDrag::Released {
                        root: moving.root,
                        after,
                        placed: moving.placed,
                    });
                }
                Ok(false) => document.refresh_view(),
                Err(reason) => {
                    document.refresh_view();
                    app.session.notice = Some(reason);
                }
            }
        });
    }

    /// The status line during a drag: the joint and its trial value.
    pub(crate) fn joint_drag_hint(&self) -> Option<String> {
        let document = self.active_document()?;
        let Some(JointDrag::Moving(m)) = &document.view.joint_drag else {
            return None;
        };
        Some(format!(
            "Joint {}: {}. Release to set it.",
            m.target.name, m.text
        ))
    }
}
