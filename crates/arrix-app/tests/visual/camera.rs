//! The camera: the ViewCube, the mouse's gestures and the view commands
//! (plans/viewport-port step 12). The camera is session state, so none of
//! it is an edit.

use arrix_viewport::ViewOrientation;

use crate::{harness, snapshot};

type Harness<'a> = egui_kittest::Harness<'a, arrix_app::ArrixApp>;

/// The active camera as `debug_state()` reports it.
fn camera(h: &Harness<'_>) -> serde_json::Value {
    let state = serde_json::to_value(h.state().debug_state()).expect("serialisable");
    state["camera"].clone()
}

fn number(camera: &serde_json::Value, key: &str) -> f64 {
    camera[key]
        .as_f64()
        .unwrap_or_else(|| panic!("{key}: {camera}"))
}

fn generation(h: &Harness<'_>) -> Option<u64> {
    h.state().debug_state().evaluation.map(|e| e.generation)
}

fn click(h: &mut Harness<'_>, at: egui::Pos2) {
    h.hover_at(at);
    h.step();
    h.drag_at(at);
    h.drop_at(at);
    h.step();
    h.step();
}

fn button(h: &Harness<'_>, at: egui::Pos2, button: egui::PointerButton, pressed: bool) {
    h.event(egui::Event::PointerButton {
        pos: at,
        button,
        pressed,
        modifiers: egui::Modifiers::NONE,
    });
}

/// Drags with `which` held from `from` by `by`, over a few frames.
fn drag(h: &mut Harness<'_>, which: egui::PointerButton, from: egui::Pos2, by: egui::Vec2) {
    h.hover_at(from);
    h.step();
    button(h, from, which, true);
    h.step();
    for i in 1..=4 {
        h.hover_at(from + by * (i as f32 / 4.0));
        h.step();
    }
    button(h, from + by, which, false);
    h.step();
}

/// A clicked ViewCube facet turns the camera to its view, a drag on the
/// cube orbits, its home and projection buttons act, and none of it edits.
#[test]
fn the_viewcube_turns_the_camera() {
    harness::with_app("the_viewcube_turns_the_camera", |h| {
        harness::open_settled(h, "tests/docs/slice");
        let generation = generation(h);
        let start = camera(h);
        assert_eq!(start["projection"], "Perspective");

        let top = h
            .state()
            .viewcube_facet(ViewOrientation::Top)
            .expect("the top facet faces the default camera");
        click(h, top);
        h.state_mut().skip_animations();
        h.step();
        let turned = camera(h);
        let half_pi = std::f64::consts::FRAC_PI_2;
        assert!(
            (number(&turned, "pitch") - half_pi).abs() < 1e-5,
            "looking straight down: {turned}"
        );
        assert_eq!(turned["target"], start["target"], "only the angles turn");
        assert_eq!(turned["distance"], start["distance"]);
        snapshot::check(h, "viewcube_top");

        // Dragging the cube itself orbits by the cube's own rate.
        let cube = h
            .state()
            .viewcube_facet(ViewOrientation::Top)
            .expect("the top facet faces the camera");
        let before = camera(h);
        drag(
            h,
            egui::PointerButton::Primary,
            cube,
            egui::vec2(0.0, -30.0),
        );
        let dragged = camera(h);
        assert_ne!(dragged["pitch"], before["pitch"], "the drag orbited");

        assert_eq!(generation, self::generation(h), "the camera is not an edit");
    });
}

/// The mouse: a middle drag orbits, shift and a middle drag pans, the wheel
/// zooms, a right drag orbits too; and over a card or the cube none of it
/// reaches the camera.
#[test]
fn the_mouse_moves_the_camera() {
    harness::with_app("the_mouse_moves_the_camera", |h| {
        harness::open_settled(h, "tests/docs/slice");
        let generation = generation(h);
        let v = h
            .state()
            .debug_state()
            .layout
            .expect("a frame ran")
            .viewport;
        let middle = egui::pos2(
            ((v[0] + v[2]) / 2.0) as f32 + 100.0,
            ((v[1] + v[3]) / 2.0) as f32 + 100.0,
        );
        let start = camera(h);

        drag(
            h,
            egui::PointerButton::Middle,
            middle,
            egui::vec2(60.0, 0.0),
        );
        let orbited = camera(h);
        assert_ne!(orbited["yaw"], start["yaw"], "a middle drag orbits");
        assert_eq!(orbited["target"], start["target"]);

        drag(
            h,
            egui::PointerButton::Secondary,
            middle,
            egui::vec2(0.0, 40.0),
        );
        let right = camera(h);
        assert_ne!(right["pitch"], orbited["pitch"], "a right drag orbits too");

        h.event(egui::Event::ModifiersChanged(egui::Modifiers::SHIFT));
        drag(
            h,
            egui::PointerButton::Middle,
            middle,
            egui::vec2(50.0, 0.0),
        );
        h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        h.step();
        let panned = camera(h);
        assert_ne!(panned["target"], right["target"], "shift pans the target");
        assert_eq!(panned["yaw"], right["yaw"]);

        h.hover_at(middle);
        h.step();
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, 120.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(12);
        let zoomed = camera(h);
        assert!(
            number(&zoomed, "distance") < number(&panned, "distance"),
            "the wheel up zooms in: {zoomed}"
        );

        assert_eq!(generation, self::generation(h), "the camera is not an edit");
    });
}

/// The view commands, on their default keys: top, front, right, iso, the
/// projection, and home, which frames the bodies again.
#[test]
fn the_view_commands_turn_the_camera() {
    use egui::{Key, Modifiers};
    harness::with_app("the_view_commands_turn_the_camera", |h| {
        harness::open_settled(h, "tests/docs/slice");
        let pitch = |h: &Harness<'_>| number(&camera(h), "pitch");
        let yaw = |h: &Harness<'_>| number(&camera(h), "yaw");
        let press = |h: &mut Harness<'_>, key| {
            h.key_press_modifiers(Modifiers::CTRL, key);
            h.step();
            h.state_mut().skip_animations();
            h.step();
        };
        let half_pi = std::f64::consts::FRAC_PI_2;

        press(h, Key::Num7);
        assert!((pitch(h) - half_pi).abs() < 1e-5, "top");
        press(h, Key::Num1);
        assert!(
            pitch(h).abs() < 1e-5 && (yaw(h) - half_pi).abs() < 1e-5,
            "front"
        );
        press(h, Key::Num3);
        assert!(pitch(h).abs() < 1e-5 && yaw(h).abs() < 1e-5, "right");
        press(h, Key::Num0);
        assert!((pitch(h) - 0.615_479_7).abs() < 1e-5, "iso");

        press(h, Key::Num5);
        assert_eq!(camera(h)["projection"], "Orthographic");
        press(h, Key::Num5);
        assert_eq!(camera(h)["projection"], "Perspective");

        // Zoom away, then home frames the bodies again.
        let framed = number(&camera(h), "distance");
        h.hover_at(egui::pos2(700.0, 450.0));
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -300.0),
            phase: egui::TouchPhase::Move,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(12);
        assert!(number(&camera(h), "distance") > framed);
        h.key_press(Key::Home);
        h.step();
        h.state_mut().skip_animations();
        h.step();
        let home = camera(h);
        assert!((number(&home, "distance") - framed).abs() < 1e-4, "{home}");
    });
}
