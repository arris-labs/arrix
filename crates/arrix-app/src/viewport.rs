//! The viewport's frame: the camera's gestures and animation, the ViewCube,
//! the pick a click or the resting pointer asks for, and the draw
//! (docs/UI-RENDERING.md §The viewport). All of it is session state: nothing
//! here issues a command.

use arrix_ui::viewcube::{self, ViewCubeAction, arrows};
use arrix_ui::{CardsView, SketchHit, paint_sketch, viewcube_rect};
use arrix_viewport::{Highlights, OrbitCamera, Renderer, StandardView};
use web_time::Instant;

use crate::documents::{PickFor, ViewState};
use crate::sketch_view::{SketchOverlay, plane_transform};

/// What a frame of the viewport left for the app to do.
pub struct Frame {
    /// The pointer left the viewport, or a hover pick must be asked for
    /// again: the hover it had is over.
    pub hover_left: bool,
    /// What the pointer did in the open sketch, when there is one.
    pub sketch: Option<SketchPointer>,
}

/// The pointer in the open sketch's plane, for what changed this frame.
pub struct SketchPointer {
    /// The item under the pointer, when that is not what the view already
    /// shows hovered.
    pub hover: Option<Option<SketchHit>>,
    /// A click, with what was under it and whether shift was held.
    pub click: Option<(Option<SketchHit>, bool)>,
}

/// How near an item the pointer is to hit it, in points.
const HIT_TOLERANCE: f32 = 7.0;

/// The camera as a comparison sees it: where it looks from and at.
fn pose(camera: &OrbitCamera) -> ([f32; 3], f32, f32, f32) {
    (
        camera.target.to_array(),
        camera.distance,
        camera.yaw,
        camera.pitch,
    )
}

/// Draws the viewport into `ui`'s rect: the view's scene through
/// `renderer` (an app without a wgpu render state draws nothing), the cube
/// over it (when `cube`: an empty start has nothing to turn, and shows
/// nothing else, ADR-0017), the open sketch over the scene, and takes the pointer's gestures. A pick a click or a resting
/// pointer asked for is left in `view.pick`.
pub fn show(
    ui: &mut egui::Ui,
    view: &mut ViewState,
    renderer: Option<&mut Renderer>,
    cards: &CardsView,
    cube: bool,
    sketch: Option<&SketchOverlay>,
) -> Frame {
    let rect = ui.max_rect();
    // The viewport's own response: a card over it is a layer above and
    // takes its clicks, so a click on a card picks nothing; the cube,
    // interacted with after it, is above it too.
    let response = ui.interact(
        rect,
        ui.id().with("arrix.viewport"),
        egui::Sense::click_and_drag(),
    );
    let before = pose(&view.camera);
    if view.camera.step_animation(Instant::now()) {
        ui.ctx().request_repaint();
    }
    gestures(ui, &response, view);

    let click = response
        .clicked()
        .then(|| response.interact_pointer_pos())
        .flatten();
    let hover = response.hover_pos();
    // The pointer is held down for a drag (the cube's included), which is
    // not a hover.
    let pressed = ui.input(|i| i.pointer.any_down());
    // A hover pick is asked for when the pointer has moved and none is on
    // its way; a click's wins over it.
    let moved = hover
        .filter(|p| view.hover_at != Some(*p))
        .filter(|_| !pressed && view.pick.is_none());
    // In sketch mode the pointer is the sketch's: nothing in the scene is
    // picked or hovered.
    let wanted = click
        .map(|at| (at, PickFor::Click))
        .or(moved.map(|at| (at, PickFor::Hover)))
        .filter(|_| sketch.is_none());
    let mut hover_left = false;
    if hover.is_none() && view.hover_at.take().is_some() {
        hover_left = true;
    }
    if let Some(renderer) = renderer {
        let highlights = Highlights {
            hover: view.hover.as_ref(),
            selection: view.selection.as_ref(),
        };
        let pick = renderer.paint(
            ui.painter(),
            rect,
            &view.scene,
            &view.camera,
            highlights,
            wanted.map(|(at, _)| at),
        );
        if let (Some(pick), Some((at, purpose))) = (pick, wanted) {
            if purpose == PickFor::Hover {
                view.hover_at = Some(at);
            }
            view.pick = Some((pick, purpose));
        }
    }

    // Painted after the scene, so over it.
    let mut sketch_pointer = None;
    if let Some(overlay) = sketch
        && let Some(to_screen) = plane_transform(&view.camera, rect, &overlay.plane)
    {
        let ctx = ui.ctx().clone();
        let hit = |at| arrix_ui::hit_test(&ctx, &overlay.view, &to_screen, at, HIT_TOLERANCE);
        let now = hover.map(hit);
        sketch_pointer = Some(SketchPointer {
            hover: now.filter(|h| *h != overlay.view.hover).or(hover
                .is_none()
                .then_some(None)
                .filter(|_| overlay.view.hover.is_some())),
            click: click.map(|at| (hit(at), shift_at_release(ui))),
        });
        paint_sketch(&ui.painter_at(rect), &to_screen, &overlay.view);
    }
    if cube {
        let camera = &view.camera;
        let cube = viewcube::viewcube(
            ui,
            viewcube_rect(rect, cards),
            camera.yaw,
            camera.pitch,
            camera.projection,
        );
        if let Some(action) = cube.action {
            view.apply_cube(action);
            ui.ctx().request_repaint();
        }
    }

    // What is under a pointer that has not moved changed with the camera.
    if pose(&view.camera) != before {
        view.hover_at = None;
    }
    Frame {
        hover_left,
        sketch: sketch_pointer,
    }
}

/// Whether shift was held when the button was released this frame: the
/// release event's own modifiers, which are the ones the click had even
/// when the key went up in the same frame.
fn shift_at_release(ui: &egui::Ui) -> bool {
    ui.input(|i| {
        i.events
            .iter()
            .rev()
            .find_map(|event| match event {
                egui::Event::PointerButton {
                    pressed: false,
                    modifiers,
                    ..
                } => Some(modifiers.shift),
                _ => None,
            })
            .unwrap_or(i.modifiers.shift)
    })
}

/// The mouse's gestures on the camera: middle drag (or right) orbits,
/// shift and middle drag pans, the wheel zooms toward the cursor. The left
/// button picks. Fixed for now: bindings in the keymap are a later plan.
fn gestures(ui: &egui::Ui, response: &egui::Response, view: &mut ViewState) {
    let rect = response.rect;
    let delta = response.drag_delta();
    let shift = ui.input(|i| i.modifiers.shift);
    if response.dragged_by(egui::PointerButton::Middle) && shift {
        view.camera.pan(delta.x, delta.y);
    } else if response.dragged_by(egui::PointerButton::Middle)
        || response.dragged_by(egui::PointerButton::Secondary)
    {
        let (yaw, pitch) = viewcube::widget::drag_orbit(delta);
        view.camera.orbit(yaw, pitch);
    }
    let scroll = ui.input(|i| i.smooth_scroll_delta.y);
    if let (true, Some(at)) = (scroll != 0.0, response.hover_pos()) {
        let ndc = (
            (at.x - rect.min.x) / rect.width().max(1.0) * 2.0 - 1.0,
            1.0 - (at.y - rect.min.y) / rect.height().max(1.0) * 2.0,
        );
        let aspect = rect.width().max(1.0) / rect.height().max(1.0);
        view.camera.zoom_to_cursor(scroll, ndc, aspect);
    }
}

impl ViewState {
    /// Flies the camera to frame every body, from the home orientation.
    pub fn home(&mut self) {
        if let Some((centre, radius)) = self.scene.bounding_sphere() {
            self.camera.animate_home(centre, radius);
        }
    }

    /// Flies the camera to look along `view`, where it is and as far.
    pub fn turn_to(&mut self, view: StandardView) {
        self.camera.animate_to_standard_view(view);
    }

    /// Does what the cube asked of the camera: every one is a call the
    /// camera already has.
    pub fn apply_cube(&mut self, action: ViewCubeAction) {
        match action {
            ViewCubeAction::Select(orientation) => self.camera.animate_to_orientation(orientation),
            ViewCubeAction::Orbit {
                delta_yaw,
                delta_pitch,
            } => self.camera.orbit(delta_yaw, delta_pitch),
            ViewCubeAction::Step {
                delta_yaw,
                delta_pitch,
            } => arrows::step_camera(&mut self.camera, delta_yaw, delta_pitch),
            ViewCubeAction::Home => self.home(),
            ViewCubeAction::ToggleProjection => self.camera.toggle_projection(),
        }
    }

    /// Finishes a running camera animation at once: how a scenario reaches
    /// the view a click flew to without waiting for the clock.
    pub fn skip_animation(&mut self) {
        let far = Instant::now() + std::time::Duration::from_secs(3600);
        self.camera.step_animation(far);
    }
}
