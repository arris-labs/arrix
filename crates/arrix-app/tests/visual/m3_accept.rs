//! M3's acceptance (plans/m3-accept): a part made through the UI by pointer
//! and keys only, from a sketch drawn with its inferences to a fillet too
//! big for its edge failing in the tree while the rest evaluates, every
//! gesture counted.

use arrix_ui::FeatureState;
use egui::Key;
use egui_kittest::kittest::Queryable as _;

use crate::forms_accept::{
    bodies, form, gesture, key, open_form, refs, sketch_at, sketch_click, type_into, wait,
    wait_state, world_click,
};
use crate::{harness, snapshot};

type Harness<'a> = egui_kittest::Harness<'a, arrix_app::ArrixApp>;

/// The tree's feature states by name, in history order.
fn tree(h: &Harness<'_>) -> Vec<(String, FeatureState)> {
    let shell = h.state().debug_state().shell.expect("a frame ran");
    shell
        .tree
        .parts
        .iter()
        .flat_map(|p| p.features.iter().map(|f| (f.name.clone(), f.state)))
        .collect()
}

/// Zooms the camera toward `at` with the wheel. The camera was framed on
/// an empty document, and a preview is never framed, so the first plate is
/// a speck until the view is brought to it.
fn zoom_on(h: &mut Harness<'_>, at: egui::Pos2, wheel: f32) {
    h.hover_at(at);
    h.step();
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, wheel),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(12);
}

#[test]
fn m3_accept() {
    harness::with_app("m3_accept", |h| {
        harness::open_settled(h, "tests/docs/sketch-mode");
        h.state_mut().seed_ids(1);
        h.set_render_every_step(true);
        let mut counts: Vec<(&str, usize)> = Vec::new();
        let mut note = |h: &Harness<'_>, what: &'static str, expect: usize| {
            let got = gesture(h).expect("a gesture was counted");
            assert_eq!(got, expect, "{what}: commands in the gesture");
            counts.push((what, got));
        };

        // 1. The plate's sketch: a rectangle whose first corner snaps to
        //    the origin, the glyph shown before the click commits it.
        h.get_by_label("New sketch").click();
        h.run_steps(3);
        note(h, "the sketch", 1);
        wait(h, "the camera turned", |h| {
            h.state().debug_state().sketch.is_some_and(|m| m.turned)
        });
        h.state_mut().skip_animations();
        h.run_steps(3);
        key(h, Key::R);
        let origin = sketch_at(h, [0.0, 0.0]);
        h.event(egui::Event::PointerMoved(origin));
        h.run_steps(3);
        let shown: Vec<String> = h
            .state()
            .debug_state()
            .sketch_view
            .and_then(|v| v.preview)
            .expect("the tool's preview")
            .glyphs
            .into_iter()
            .flat_map(|g| g.commits)
            .collect();
        assert!(
            shown.contains(&"coincident_to_datum".to_owned()),
            "the snap shows the origin: {shown:?}"
        );
        sketch_click(h, [0.0, 0.0]);
        note(h, "the first corner", 0);
        sketch_click(h, [50.0, 30.0]);
        note(h, "the rectangle", 1);
        wait(h, "the rectangle", |h| {
            h.state()
                .debug_state()
                .sketch_view
                .is_some_and(|v| v.curves.len() == 4)
        });
        let saved = h.state().saved().expect("a document is open").0;
        let committed = saved
            .iter()
            .filter(|(path, _)| path.starts_with("parts/"))
            .any(|(_, bytes)| String::from_utf8_lossy(bytes).contains("coincident_to_datum"));
        assert!(committed, "the corner holds what the glyph showed");
        key(h, Key::Escape);
        key(h, Key::Escape);
        key(h, Key::Escape);
        assert!(h.state().debug_state().sketch.is_none());
        harness::wait_settled(h);

        // 2. The extrude: its region picked, its distance typed, the preview
        //    shown before OK, then the plate.
        open_form(h, "Part", "New extrude");
        note(h, "opening the extrude form", 0);
        world_click(h, [0.025, 0.015, 0.0]);
        note(h, "the region pick", 0);
        type_into(h, 1, "12 mm");
        note(h, "the distance", 0);
        wait(h, "the preview", |h| {
            h.state()
                .debug_state()
                .preview
                .as_ref()
                .is_some_and(|p| p.settled)
        });
        let plate = h
            .state()
            .screen_point([0.025, 0.015, 0.006])
            .expect("in front of the camera");
        for _ in 0..4 {
            zoom_on(h, plate, 500.0);
        }
        h.event(egui::Event::PointerMoved(egui::pos2(700.0, 800.0)));
        h.run_steps(3);
        snapshot::check(h, "m3_accept_preview");
        h.get_by_label("OK").click();
        h.run_steps(3);
        note(h, "the extrude's OK", 1);
        harness::wait_settled(h);
        key(h, Key::Home);
        h.state_mut().skip_animations();
        h.run_steps(3);
        assert_eq!(bodies(h).len(), 1, "the plate");

        // 3. A face of the plate picked: it selects, and seeds the datum
        //    plane's form, whose OK is the plane.
        world_click(h, [0.025, 0.015, 0.012]);
        note(h, "the face pick", 0);
        assert!(
            h.state().debug_state().selection.is_some(),
            "the face is selected"
        );
        open_form(h, "Sketch", "New datum plane");
        note(h, "opening the datum plane's form", 0);
        h.get_by_label("OK").click();
        h.run_steps(3);
        note(h, "the datum plane", 1);
        harness::wait_settled(h);

        // 4. The spur gear, from the Gears tab, on the plane picked in the
        //    tree.
        open_form(h, "Gears", "New spur gear");
        note(h, "opening the gear's form", 0);
        h.get_by_label("Datum plane 1").click();
        h.run_steps(3);
        note(h, "the plane picked in the tree", 0);
        type_into(h, 1, "16");
        note(h, "the teeth", 0);
        wait(h, "the gear's preview", |h| {
            h.state()
                .debug_state()
                .preview
                .as_ref()
                .is_some_and(|p| p.settled)
        });
        h.event(egui::Event::PointerMoved(egui::pos2(700.0, 800.0)));
        h.run_steps(3);
        snapshot::check(h, "m3_accept_gear");
        h.get_by_label("OK").click();
        h.run_steps(3);
        note(h, "the gear", 1);
        harness::wait_settled(h);
        assert_eq!(bodies(h).len(), 2, "the plate and the gear");

        // 5. The gear joined to the plate: the plate picked as the target
        //    by a face, the gear as the tool by the tree.
        open_form(h, "Part", "New boolean");
        note(h, "opening the boolean's form", 0);
        world_click(h, [0.025, 0.015, 0.012]);
        note(h, "the target", 0);
        h.get_by_label("Bodies to fuse in, cut away or intersect with")
            .click();
        h.run_steps(3);
        h.get_by_label("Spur gear 1").click();
        h.run_steps(3);
        note(h, "the tool", 0);
        let f = form(h).expect("the boolean's form");
        assert_eq!(refs(&f, "tools").len(), 1);
        h.get_by_label("OK").click();
        h.run_steps(3);
        note(h, "the join", 1);
        harness::wait_settled(h);
        assert_eq!(bodies(h).len(), 1, "the joined body");

        // 6. A fillet too big for its plate edge: it fails in the tree and
        //    the rest evaluates.
        open_form(h, "Part", "New fillet");
        world_click(h, [0.025, 0.015, 0.012]);
        h.get_by_label("Edges").click();
        h.run_steps(3);
        world_click(h, [0.05, 0.015, 0.012]);
        note(h, "the edge", 0);
        type_into(h, 1, "50 mm");
        let f = wait_state(h, FeatureState::Failed);
        assert_eq!(
            f.diagnostic.expect("the preview's diagnostic").code,
            "kernel.degenerate.blend-too-large"
        );
        h.get_by_label("OK").click();
        h.run_steps(3);
        note(h, "the fillet", 1);
        harness::wait_settled(h);
        h.event(egui::Event::PointerMoved(egui::pos2(700.0, 800.0)));
        h.run_steps(3);
        let states = tree(h);
        assert_eq!(
            states.last(),
            Some(&("Fillet 1".to_owned(), FeatureState::Failed)),
            "{states:?}"
        );
        assert!(
            states[..states.len() - 1]
                .iter()
                .all(|(_, s)| *s == FeatureState::Ok),
            "the rest evaluates: {states:?}"
        );
        assert_eq!(bodies(h).len(), 1, "the joined body is still drawn");
        snapshot::check(h, "m3_accept_failed");

        for (what, n) in &counts {
            assert!(*n <= 1, "{what}: {n} commands");
        }
    });
}
