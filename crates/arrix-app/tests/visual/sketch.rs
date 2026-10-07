//! Sketch mode (plans/sketch-mode): entering and leaving it.

use egui_kittest::kittest::Queryable as _;

use crate::{harness, snapshot};

type Harness<'a> = egui_kittest::Harness<'a, arrix_app::ArrixApp>;

/// Steps until sketch mode's camera has turned to the plane and its
/// animation is over; a minute at most.
fn wait_turned(h: &mut Harness<'_>) {
    let started = web_time::Instant::now();
    while !h
        .state()
        .debug_state()
        .sketch
        .is_some_and(|mode| mode.turned)
    {
        h.step();
        assert!(
            started.elapsed() < std::time::Duration::from_secs(60),
            "the camera did not turn: {}",
            h.state().debug_state_json()
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.state_mut().skip_animations();
    h.run_steps(3);
}

fn gesture(h: &Harness<'_>) -> Option<usize> {
    h.state().debug_state().gesture_commands
}

fn projection(h: &Harness<'_>) -> String {
    h.state()
        .debug_state()
        .camera
        .expect("a document is open")
        .projection
}

fn tree_names(h: &Harness<'_>) -> Vec<String> {
    let tree = h.state().debug_state().shell.expect("a frame ran").tree;
    tree.parts
        .into_iter()
        .flat_map(|p| p.features.into_iter().map(|f| f.name))
        .collect()
}

/// An empty document, `core.sketch.create` with nothing selected, then
/// `core.sketch.finish`: one command, then none; the camera turns square to
/// the world XY plane in orthographic projection, and returns.
#[test]
fn sketch_enter() {
    harness::with_app("sketch_enter", |h| {
        h.state_mut().seed_ids(1);
        h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::N);
        harness::wait_settled(h);
        let home = h.state().debug_state().camera.expect("a document is open");
        assert_eq!(home.projection, "Perspective");
        assert!(h.state().debug_state().sketch.is_none());

        h.get_by_label("New sketch").click();
        h.run_steps(3);
        assert_eq!(gesture(h), Some(1), "one command to create");
        assert_eq!(tree_names(h), ["Sketch 1"]);
        wait_turned(h);
        let turned = h.state().debug_state().camera.expect("a camera");
        assert_eq!(projection(h), "Orthographic");
        assert!((turned.pitch - std::f64::consts::FRAC_PI_2).abs() < 1e-3);
        assert_eq!(turned.target, [0.0, 0.0, 0.0]);
        // The pointer leaves the button, whose hint it would show.
        h.event(egui::Event::PointerMoved(egui::pos2(700.0, 500.0)));
        h.run_steps(3);
        let hint = h.state().debug_state().shell.unwrap().status.hint;
        assert_eq!(hint, "Sketching Sketch 1. Escape finishes the sketch.");
        harness::wait_settled(h);
        snapshot::check(h, "sketch_enter");

        h.key_press(egui::Key::Escape);
        h.run_steps(2);
        h.state_mut().skip_animations();
        h.run_steps(3);
        assert_eq!(gesture(h), Some(0), "finishing issues no command");
        assert!(h.state().debug_state().sketch.is_none());
        let back = h.state().debug_state().camera.expect("a camera");
        assert_eq!(back.projection, "Perspective");
        assert_eq!(
            (back.yaw, back.pitch, back.distance),
            (home.yaw, home.pitch, home.distance),
            "the camera returned"
        );
        assert_eq!(tree_names(h), ["Sketch 1"]);
        harness::wait_settled(h);
        snapshot::check(h, "sketch_left");
    });
}

/// The slice's sketches opened in sketch mode: the overlay holds their
/// points, curves, constraint glyphs and dimension labels, with the DoF and
/// solver state in the status line.
#[test]
fn sketch_view() {
    harness::with_app("sketch_view", |h| {
        harness::open_settled(h, "tests/docs/slice");
        // Sketch 5 is the pocket's: a rectangle with its dimensions.
        h.get_by_label("Sketch 5").click();
        h.run_steps(3);
        h.get_by_label("Edit sketch").click();
        h.run_steps(3);
        wait_turned(h);
        h.event(egui::Event::PointerMoved(egui::pos2(900.0, 60.0)));
        h.run_steps(3);
        harness::wait_settled(h);
        snapshot::check(h, "sketch_view");
    });
}
