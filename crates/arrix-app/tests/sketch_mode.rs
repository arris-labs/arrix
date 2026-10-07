//! Entering and leaving sketch mode, by the tree and by commands, on the
//! slice document (docs/UI-RENDERING.md §Sketch mode). Logic only.

use arrix_app::ArrixApp;
use egui::{Key, Modifiers};
use egui_kittest::{Harness, kittest::Queryable as _};

fn app() -> Harness<'static, ArrixApp> {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1440.0, 900.0))
        // Short steps, so two clicks are a double-click to egui's clock.
        .with_step_dt(0.02)
        .build_eframe(|cc| ArrixApp::new(cc));
    harness.run();
    harness.state_mut().seed_ids(7);
    harness
        .state_mut()
        .open_path(&std::path::Path::new("../..").join("tests/docs/slice"))
        .expect("the slice opens");
    settle(&mut harness);
    harness
}

/// Steps until the document has said everything, which the session's
/// threads do on their own clock.
fn settle(h: &mut Harness<'_, ArrixApp>) {
    let started = web_time::Instant::now();
    while !h
        .state()
        .debug_state()
        .evaluation
        .is_some_and(|e| e.settled)
    {
        h.step();
        assert!(
            started.elapsed().as_secs() < 60,
            "the document did not settle"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.run();
}

/// Steps until the camera has turned to the sketch's plane.
fn turned(h: &mut Harness<'_, ArrixApp>) {
    let started = web_time::Instant::now();
    while !h
        .state()
        .debug_state()
        .sketch
        .is_some_and(|mode| mode.turned)
    {
        h.step();
        assert!(started.elapsed().as_secs() < 60, "the camera did not turn");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.state_mut().skip_animations();
    h.run_steps(2);
}

fn double_click(h: &mut Harness<'_, ArrixApp>, label: &str) {
    let pos = h.get_by_label(label).rect().center();
    h.event(egui::Event::PointerMoved(pos));
    h.run_steps(2);
    for _ in 0..2 {
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            });
            h.step();
        }
    }
    h.run_steps(2);
}

fn gesture(h: &Harness<'_, ArrixApp>) -> Option<usize> {
    h.state().debug_state().gesture_commands
}

fn generation(h: &Harness<'_, ArrixApp>) -> u64 {
    h.state().debug_state().evaluation.unwrap().generation
}

fn mode(h: &Harness<'_, ArrixApp>) -> Option<String> {
    h.state().debug_state().sketch.map(|m| m.feature)
}

#[test]
fn a_double_click_on_a_sketch_opens_it_without_a_command() {
    let mut h = app();
    let before = generation(&h);
    double_click(&mut h, "Sketch 0");
    h.run_steps(3);
    assert!(mode(&h).is_some());
    assert_eq!(gesture(&h), Some(0), "opening a sketch issues no command");
    assert_eq!(generation(&h), before);
    turned(&mut h);
    let camera = h.state().debug_state().camera.unwrap();
    assert_eq!(camera.projection, "Orthographic");

    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(mode(&h).is_none());
    assert_eq!(gesture(&h), Some(0));
    assert_eq!(generation(&h), before);
}

#[test]
fn a_sketch_made_with_a_datum_plane_selected_stands_on_it() {
    let mut h = app();
    h.get_by_label("Plate top").click();
    h.run_steps(3);
    h.get_by_label("New sketch").click();
    h.run_steps(3);
    assert_eq!(gesture(&h), Some(1));
    turned(&mut h);
    // The datum plane is offset up the Z axis; the view is square to it.
    let camera = h.state().debug_state().camera.unwrap();
    assert!(camera.target[2] > 0.0, "{:?}", camera.target);
    assert!((camera.pitch - std::f64::consts::FRAC_PI_2).abs() < 1e-3);
    let saved = h.state().saved().unwrap();
    let text = String::from_utf8_lossy(&saved.0["parts/F7Y3N3HJPEEDE.json"]).into_owned();
    assert!(
        text.contains("\"Sketch 1\""),
        "the new sketch is in the part"
    );
    assert!(
        text.matches("\"slot\": \"plane\"").count() >= 2,
        "it stands on the datum plane's slot"
    );
}

#[test]
fn undoing_the_sketch_that_is_open_leaves_sketch_mode() {
    let mut h = app();
    h.get_by_label("New sketch").click();
    h.run_steps(3);
    assert_eq!(gesture(&h), Some(1));
    turned(&mut h);
    h.key_press_modifiers(Modifiers::CTRL, Key::Z);
    h.run_steps(4);
    assert_eq!(gesture(&h), Some(1));
    assert!(mode(&h).is_none(), "nothing is left to edit");
    assert_eq!(
        h.state().debug_state().camera.unwrap().projection,
        "Perspective"
    );
}

#[test]
fn finishing_and_editing_are_off_where_they_do_not_apply() {
    let mut h = app();
    let off = |h: &Harness<'_, ArrixApp>, command: &str| {
        let ribbon = h.state().debug_state().shell.unwrap().ribbon;
        let enabled = ribbon
            .groups
            .iter()
            .flat_map(|g| &g.commands)
            .find(|c| c.id == command)
            .map(|c| c.enabled);
        enabled == Some(false)
    };
    assert!(off(&h, "core.sketch.finish"));
    assert!(off(&h, "core.sketch.edit"), "no sketch is selected");
    h.get_by_label("Sketch 5").click();
    h.run_steps(3);
    assert!(!off(&h, "core.sketch.edit"));
    h.get_by_label("Edit sketch").click();
    h.run_steps(3);
    assert!(mode(&h).is_some());
    assert!(off(&h, "core.sketch.edit") && off(&h, "core.sketch.create"));
    assert!(!off(&h, "core.sketch.finish"));
}
