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

/// Opens the slice's pocket sketch, a 6 mm square at the origin, and waits
/// until it is drawn.
fn open_pocket_sketch() -> Harness<'static, ArrixApp> {
    let mut h = app();
    h.get_by_label("Sketch 5").click();
    h.run_steps(3);
    h.get_by_label("Edit sketch").click();
    h.run_steps(3);
    turned(&mut h);
    h
}

fn at(h: &Harness<'_, ArrixApp>, uv: [f64; 2]) -> egui::Pos2 {
    h.state().sketch_point(uv).expect("the sketch is drawn")
}

fn hover(h: &mut Harness<'_, ArrixApp>, uv: [f64; 2]) {
    let pos = at(h, uv);
    h.event(egui::Event::PointerMoved(pos));
    h.run_steps(3);
}

fn click(h: &mut Harness<'_, ArrixApp>, uv: [f64; 2], modifiers: Modifiers) {
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
}

fn hint(h: &Harness<'_, ArrixApp>) -> String {
    h.state().debug_state().shell.unwrap().status.hint
}

fn selected(h: &Harness<'_, ArrixApp>) -> usize {
    h.state()
        .debug_state()
        .sketch_view
        .map_or(0, |v| v.selected.len())
}

#[test]
fn the_pointer_names_what_is_under_it_and_a_click_selects_it() {
    let mut h = open_pocket_sketch();
    // The middle of the right edge: a line, with its mark beside it.
    hover(&mut h, [0.006, 0.003]);
    assert_eq!(hint(&h), "Line: click to select it.");
    click(&mut h, [0.006, 0.003], Modifiers::NONE);
    assert_eq!(gesture(&h), Some(0), "selecting issues no command");
    assert_eq!(selected(&h), 1);
    assert_eq!(hint(&h), "Line: selected. Delete removes it.");
    // A corner is a point, though two lines end on it.
    hover(&mut h, [0.006, 0.006]);
    assert_eq!(hint(&h), "Point: click to select it.");
    // Shift adds, and a second one removes.
    click(&mut h, [0.006, 0.006], Modifiers::SHIFT);
    assert_eq!(selected(&h), 2);
    click(&mut h, [0.006, 0.006], Modifiers::SHIFT);
    assert_eq!(selected(&h), 1);
    // A click on the mark of a constraint selects the constraint.
    let mark = h.state().debug_state().sketch_view.unwrap().glyphs[0].at;
    let box_ = at(&h, mark) + egui::vec2(10.0, -10.0);
    h.event(egui::Event::PointerMoved(box_));
    h.run_steps(3);
    assert!(
        hint(&h).ends_with("constraint: click to select it."),
        "{}",
        hint(&h)
    );
    // Empty space inside the square clears the selection.
    click(&mut h, [0.003, 0.003], Modifiers::NONE);
    assert_eq!(selected(&h), 0);
    assert_eq!(gesture(&h), Some(0));
}

#[test]
fn delete_removes_the_selection_as_one_command_and_undo_restores_it() {
    let mut h = open_pocket_sketch();
    let before = h.state().saved().unwrap().0;
    let curves =
        |h: &Harness<'_, ArrixApp>| h.state().debug_state().sketch_view.unwrap().curves.len();
    assert_eq!(curves(&h), 4);

    click(&mut h, [0.006, 0.003], Modifiers::NONE);
    h.key_press(Key::Delete);
    h.run_steps(3);
    assert_eq!(
        gesture(&h),
        Some(1),
        "one command for the line and its marks"
    );
    assert_ne!(h.state().saved().unwrap().0, before);
    // The evaluation of the edited sketch arrives on its own clock.
    let started = web_time::Instant::now();
    while curves(&h) != 3 {
        h.step();
        assert!(started.elapsed().as_secs() < 60, "the edit was not drawn");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(selected(&h), 0, "what was deleted is not selected");

    h.key_press_modifiers(Modifiers::CTRL, Key::Z);
    h.run_steps(3);
    assert_eq!(gesture(&h), Some(1));
    assert_eq!(h.state().saved().unwrap().0, before, "byte for byte");
    let started = web_time::Instant::now();
    while curves(&h) != 4 {
        h.step();
        assert!(started.elapsed().as_secs() < 60, "the undo was not drawn");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn delete_with_nothing_selected_is_off() {
    let mut h = open_pocket_sketch();
    let generation_before = generation(&h);
    h.key_press(Key::Delete);
    h.run_steps(3);
    assert_eq!(gesture(&h), Some(0));
    assert_eq!(generation(&h), generation_before);
}
