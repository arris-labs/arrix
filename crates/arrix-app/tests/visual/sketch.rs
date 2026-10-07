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

/// Hover and selection in the sketch: the pointer over a line is amber and
/// named in the status line, a clicked line and a shift-clicked corner are
/// blue, and none of it is a command.
#[test]
fn sketch_select() {
    harness::with_app("sketch_select", |h| {
        harness::open_settled(h, "tests/docs/slice");
        h.get_by_label("Sketch 5").click();
        h.run_steps(3);
        h.get_by_label("Edit sketch").click();
        h.run_steps(3);
        wait_turned(h);
        let at = |h: &Harness<'_>, uv: [f64; 2]| h.state().sketch_point(uv).expect("drawn");
        let click = |h: &mut Harness<'_>, uv: [f64; 2], modifiers: egui::Modifiers| {
            let pos = at(h, uv);
            h.event(egui::Event::PointerMoved(pos));
            h.step();
            for pressed in [true, false] {
                h.event(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers,
                });
                h.step();
            }
            h.run_steps(3);
        };
        click(h, [0.0, 0.003], egui::Modifiers::NONE);
        click(h, [0.006, 0.006], egui::Modifiers::SHIFT);
        assert_eq!(gesture(h), Some(0), "selecting issues no command");
        // The pointer rests over the bottom edge.
        let pos = at(h, [0.003, 0.0]);
        h.event(egui::Event::PointerMoved(pos));
        h.run_steps(3);
        let state = h.state().debug_state();
        assert_eq!(state.sketch_view.as_ref().unwrap().selected.len(), 2);
        assert_eq!(
            state.shell.unwrap().status.hint,
            "Line: click to select it."
        );
        harness::wait_settled(h);
        snapshot::check(h, "sketch_select");
    });
}

/// The rectangle tool, twice: a rectangle off the origin and one with a
/// corner snapped to it, each one command; then the line tool started on
/// a corner, its pending segment level with it, the snap's glyphs shown.
#[test]
fn sketch_draw() {
    harness::with_app("sketch_draw", |h| {
        h.state_mut().seed_ids(1);
        h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::N);
        harness::wait_settled(h);
        h.get_by_label("New sketch").click();
        h.run_steps(3);
        wait_turned(h);
        let at = |h: &Harness<'_>, uv: [f64; 2]| {
            h.state()
                .sketch_point([uv[0] * 1e-3, uv[1] * 1e-3])
                .expect("drawn")
        };
        let click = |h: &mut Harness<'_>, uv: [f64; 2]| {
            let pos = at(h, uv);
            h.event(egui::Event::PointerMoved(pos));
            h.step();
            for pressed in [true, false] {
                h.event(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
                h.step();
            }
            h.run_steps(3);
        };
        h.key_press(egui::Key::R);
        h.run_steps(2);
        click(h, [20.0, 15.0]);
        click(h, [50.0, 35.0]);
        assert_eq!(gesture(h), Some(1), "one command per rectangle");
        harness::wait_settled(h);
        click(h, [0.2, 0.3]);
        click(h, [-30.0, -20.0]);
        assert_eq!(gesture(h), Some(1));
        harness::wait_settled(h);
        h.key_press(egui::Key::L);
        h.run_steps(2);
        click(h, [50.0, 35.0]);
        assert_eq!(gesture(h), Some(0), "a line's start is no command");
        let pos = at(h, [80.0, 35.5]);
        h.event(egui::Event::PointerMoved(pos));
        h.run_steps(3);
        harness::wait_settled(h);
        snapshot::check(h, "sketch_draw");
    });
}
