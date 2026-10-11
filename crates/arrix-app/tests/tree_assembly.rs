//! Activating a feature in the tree looks at its part, whatever the tab
//! looked at (plans/c2-m2 step 5, ADR-0030 §1). Logic only: a double click
//! needs a frame step shorter than its gap, which the visual harness's is
//! not; the tree's own scenario is `visual tree_assembly`.

use arrix_app::ArrixApp;
use egui::Modifiers;
use egui_kittest::{Harness, kittest::Queryable as _};

#[test]
fn double_clicking_a_feature_looks_at_its_part() {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1440.0, 900.0))
        .with_step_dt(0.02)
        .build_eframe(|cc| ArrixApp::new(cc));
    h.run();
    h.state_mut()
        .open_path_without(std::path::Path::new("../../tests/docs/assembly"), &[])
        .expect("the scenario opens");
    let started = web_time::Instant::now();
    while !h
        .state()
        .debug_state()
        .evaluation
        .is_some_and(|e| e.settled)
    {
        assert!(started.elapsed().as_secs() < 60, "it never settled");
        h.step();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    h.run_steps(35);
    let kind = |h: &Harness<'_, ArrixApp>| h.state().debug_state().context.map(|c| c.kind);
    assert_eq!(kind(&h).as_deref(), Some("assembly"));

    let pos = h.get_by_label("Plate").rect().center();
    h.event(egui::Event::PointerMoved(pos));
    h.step();
    for pressed in [true, false, true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        h.step();
    }
    h.run_steps(3);
    assert_eq!(kind(&h).as_deref(), Some("part"));
    let tree = h.state().debug_state().shell.expect("a frame ran").tree;
    assert!(tree.parts.iter().any(|p| p.current && p.name == "Bracket"));
}
