//! The joint drag (plans/c2-m2 step 10, ADR-0030 §3 and §4): a left drag
//! on the hinged servo turns it about its joint, previewed by forward
//! kinematics with nothing uploaded, and the release is one command that
//! lands the servo where the preview showed; a release in place commits
//! nothing; a grounded servo does not drag; a literal value is edited in
//! place and an expression refuses the drag. The frame budget is gated.

use arrix_core::{ComponentId, Id};
use arrix_doc::Command;

use crate::component_picks::{click_on, open_with_second_servo, servo_2};
use crate::harness;

type Harness<'a> = egui_kittest::Harness<'a, arrix_app::ArrixApp>;

const SERVO: &str = "FEJ0R7MAVP776";
const HINGE: &str = "0G7W8ZGKFDKEV";

fn servo() -> ComponentId {
    ComponentId(SERVO.parse::<Id>().expect("the servo's id"))
}

pub(crate) fn button(h: &mut Harness<'_>, pos: egui::Pos2, pressed: bool) {
    h.event(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    });
    h.step();
}

/// The pose of the instance of `path`'s first body: origin, x and z.
pub(crate) fn pose(h: &Harness<'_>, path: &str) -> [[f64; 3]; 3] {
    let state = h.state().debug_state();
    let i = state
        .instances
        .iter()
        .find(|i| i.path == [path.to_owned()])
        .expect("the component's instance");
    [i.origin, i.x_axis, i.z_axis]
}

/// The angle between two orientations, from their x and z axes.
pub(crate) fn turned(a: [[f64; 3]; 3], b: [[f64; 3]; 3]) -> f64 {
    let dot = |u: [f64; 3], v: [f64; 3]| u[0] * v[0] + u[1] * v[1] + u[2] * v[2];
    let cross = |u: [f64; 3], v: [f64; 3]| {
        [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ]
    };
    let (ya, yb) = (cross(a[2], a[1]), cross(b[2], b[1]));
    // The trace of Rb·Raᵀ is the sum over the axes of their dots.
    let trace = dot(a[1], b[1]) + dot(ya, yb) + dot(a[2], b[2]);
    ((trace - 1.0) / 2.0).clamp(-1.0, 1.0).acos()
}

/// Presses at `from` and walks the pointer to `to` in `steps`, waiting for
/// the press's pick on the way; the button stays down.
pub(crate) fn press_and_walk(h: &mut Harness<'_>, from: egui::Pos2, to: egui::Pos2, steps: usize) {
    h.event(egui::Event::PointerMoved(from));
    h.step();
    button(h, from, true);
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        h.event(egui::Event::PointerMoved(from + (to - from) * t));
        h.step();
        // The drag starts past egui's threshold, and asks for its pick.
        let started = web_time::Instant::now();
        while h.state().debug_state().picking {
            assert!(started.elapsed().as_secs() < 10, "the pick never came back");
            std::thread::sleep(std::time::Duration::from_millis(2));
            h.step();
        }
    }
}

pub(crate) fn hint(h: &Harness<'_>) -> String {
    h.state().debug_state().shell.expect("a frame").status.hint
}

/// A point on the hinged servo, found by clicking near its centre.
fn on_servo(h: &mut Harness<'_>) -> egui::Pos2 {
    let centre = pose(h, SERVO)[0];
    click_on(h, centre, &[servo()])
}

fn param(h: &Harness<'_>, name: &str) -> String {
    let saved = h.state().saved().expect("a document").0;
    let params: serde_json::Value =
        serde_json::from_slice(&saved["params.json"]).expect("params.json");
    params
        .as_object()
        .unwrap()
        .values()
        .find(|p| p["name"] == name)
        .expect("the parameter")["expr"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn a_drag_turns_the_hinged_servo_and_its_release_lands_where_it_showed() {
    harness::with_app("a_drag_turns_the_hinged_servo", |h| {
        open_with_second_servo(h);
        h.set_render_every_step(true);
        let at = on_servo(h);
        let before = pose(h, SERVO);
        let other = pose(h, &servo_2().0.to_string());
        let bodies = serde_json::to_value(h.state().debug_state().bodies).unwrap();

        press_and_walk(h, at, at + egui::vec2(70.0, 40.0), 6);
        let drag = h.state().debug_state().joint_drag.expect("dragging");
        assert_eq!(drag.joint, HINGE);
        assert!(drag.value.ends_with(" deg") && drag.q != 0.0, "{drag:?}");
        assert!(hint(h).starts_with("Joint hinge: "), "{}", hint(h));
        let preview = pose(h, SERVO);
        // FK at the trial value: the servo turned through q about its
        // hinge, and the grounded one stayed.
        assert!(
            (turned(before, preview) - drag.q.abs()).abs() < 1e-6,
            "turned {} for q {}",
            turned(before, preview),
            drag.q
        );
        assert_eq!(pose(h, &servo_2().0.to_string()), other);
        assert_eq!(
            serde_json::to_value(h.state().debug_state().bodies).unwrap(),
            bodies,
            "no body changed: the instances only moved"
        );

        // The release: one command, the parameter the hinge reads.
        button(h, at + egui::vec2(70.0, 40.0), false);
        assert_eq!(h.state().debug_state().gesture_commands, Some(1));
        let last = h.state().debug_state().gestures.last().cloned().unwrap();
        assert_eq!(last.what, "joint drag");
        assert_eq!(param(h, "angle"), drag.value, "the value is the angle's");
        assert_eq!(pose(h, SERVO), preview, "held until the evaluation");
        harness::wait_settled(h);
        h.run_steps(2);
        let landed = pose(h, SERVO);
        for (a, b) in landed.iter().flatten().zip(preview.iter().flatten()) {
            assert!(
                (a - b).abs() <= 2e-9,
                "landed {landed:?}, shown {preview:?}"
            );
        }
        assert!(h.state().debug_state().joint_drag.is_none());
    });
}

#[test]
fn a_release_in_place_commits_nothing_and_a_grounded_servo_does_not_drag() {
    harness::with_app("a_release_in_place_commits_nothing", |h| {
        open_with_second_servo(h);
        h.set_render_every_step(true);
        let at = on_servo(h);
        let before = pose(h, SERVO);
        press_and_walk(h, at, at + egui::vec2(60.0, -30.0), 4);
        assert_ne!(pose(h, SERVO), before, "it moved while dragged");
        for i in 1..=4 {
            let t = i as f32 / 4.0;
            h.event(egui::Event::PointerMoved(
                at + egui::vec2(60.0, -30.0) * (1.0 - t),
            ));
            h.step();
        }
        button(h, at, false);
        assert_eq!(h.state().debug_state().gesture_commands, Some(0));
        assert_eq!(pose(h, SERVO), before, "back where it started");
        assert_eq!(param(h, "angle"), "0 deg");

        let other = servo_2().0.to_string();
        let centre = pose(h, &other)[0];
        let at = click_on(h, centre, &[servo_2()]);
        let gestures = h.state().debug_state().gestures.len();
        press_and_walk(h, at, at + egui::vec2(50.0, 20.0), 4);
        assert!(h.state().debug_state().joint_drag.is_none());
        assert!(hint(h).contains("grounded"), "{}", hint(h));
        button(h, at + egui::vec2(50.0, 20.0), false);
        assert_eq!(
            h.state().debug_state().gestures.len(),
            gestures,
            "no gesture"
        );
        assert!(
            hint(h).contains("grounded"),
            "the reason stays: {}",
            hint(h)
        );
    });
}

#[test]
fn a_literal_is_edited_in_place_and_an_expression_refuses_the_drag() {
    harness::with_app("a_literal_is_edited_in_place", |h| {
        let assembly = open_with_second_servo(h);
        h.set_render_every_step(true);
        let set_value = |h: &mut Harness<'_>, text: &str| {
            let saved = h.state().saved().unwrap().0;
            let (_, bytes) = saved
                .iter()
                .find(|(p, _)| p.starts_with("assemblies/"))
                .unwrap();
            let record: serde_json::Value = serde_json::from_slice(bytes).unwrap();
            let mut joint: arrix_doc::Joint =
                serde_json::from_value(record["joints"][HINGE].clone()).unwrap();
            joint.value = arrix_doc::expr::Expr::parse(text).unwrap();
            h.state_mut()
                .submit(Command::EditJoint {
                    assembly,
                    joint: Box::new(joint),
                })
                .unwrap();
            harness::wait_settled(h);
            h.run_steps(2);
        };
        let joint_value = |h: &Harness<'_>| {
            let saved = h.state().saved().unwrap().0;
            let (_, bytes) = saved
                .iter()
                .find(|(p, _)| p.starts_with("assemblies/"))
                .unwrap();
            let record: serde_json::Value = serde_json::from_slice(bytes).unwrap();
            record["joints"][HINGE]["value"]
                .as_str()
                .unwrap()
                .to_owned()
        };

        set_value(h, "10 deg");
        let at = on_servo(h);
        press_and_walk(h, at, at + egui::vec2(-50.0, 50.0), 5);
        let drag = h.state().debug_state().joint_drag.expect("dragging");
        button(h, at + egui::vec2(-50.0, 50.0), false);
        assert_eq!(h.state().debug_state().gesture_commands, Some(1));
        assert_eq!(joint_value(h), drag.value, "the literal is replaced");
        assert_eq!(param(h, "angle"), "0 deg", "no parameter is touched");
        harness::wait_settled(h);

        set_value(h, "angle * 2");
        let before = pose(h, SERVO);
        let at = on_servo(h);
        press_and_walk(h, at, at + egui::vec2(50.0, 50.0), 4);
        assert!(h.state().debug_state().joint_drag.is_none());
        assert!(
            hint(h).contains("edit it in the joint's form"),
            "{}",
            hint(h)
        );
        button(h, at + egui::vec2(50.0, 50.0), false);
        assert_eq!(pose(h, SERVO), before);
        assert_eq!(joint_value(h), "angle * 2");
    });
}

/// A UI frame while a drag turns the hinged servo (FK on the UI thread,
/// the instances moved, the app's frame as the CPU does it), against the
/// 16 ms of docs/CONCURRENCY-WASM.md §Budgets: gated on the median.
#[test]
fn joint_drag_frame_budget() {
    harness::with_app("joint_drag_frame_budget", |h| {
        open_with_second_servo(h);
        h.set_render_every_step(true);
        let at = on_servo(h);
        press_and_walk(h, at, at + egui::vec2(30.0, 0.0), 3);
        assert!(h.state().debug_state().joint_drag.is_some(), "dragging");
        // The CPU's frame, as `ui_frame_budget` measures it: lavapipe's
        // raster says nothing about a GPU.
        h.set_render_every_step(false);
        let mut times: Vec<f64> = (0..200)
            .map(|i| {
                let angle = i as f32 * 0.05;
                let to = at + egui::vec2(angle.cos() * 60.0, angle.sin() * 60.0);
                let started = web_time::Instant::now();
                h.event(egui::Event::PointerMoved(to));
                h.step();
                started.elapsed().as_secs_f64() * 1e3
            })
            .collect();
        assert!(
            h.state().debug_state().joint_drag.is_some(),
            "still dragging"
        );
        times.sort_by(f64::total_cmp);
        let q = |f: f64| times[((times.len() - 1) as f64 * f) as usize];
        println!(
            "joint-drag frame, assembly on screen, {} frames: median {:.2} ms, p95 {:.2} ms, max {:.2} ms (gate 16 ms)",
            times.len(),
            q(0.5),
            q(0.95),
            q(1.0)
        );
        assert!(
            q(0.5) <= 16.0,
            "the median joint-drag frame is {} ms",
            q(0.5)
        );
    });
}
