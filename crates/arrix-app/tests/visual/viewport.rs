//! The renderer on a fixture mesh, apart from any document
//! (plans/viewport-port step 6).

use crate::{fixture, harness, snapshot};

/// The pixel at the centre of the viewport's rect.
fn centre_pixel(
    h: &egui_kittest::Harness<'_, arrix_app::ArrixApp>,
    frame: &image::RgbaImage,
) -> [u8; 4] {
    let l = h.state().debug_state().layout.expect("a frame ran");
    let ppp = f64::from(h.ctx.pixels_per_point());
    let x = (l.viewport[0] + l.viewport[2]) / 2.0 * ppp;
    let y = (l.viewport[1] + l.viewport[3]) / 2.0 * ppp;
    frame.get_pixel(x as u32, y as u32).0
}

/// A body shown in the viewport is drawn, lit, over the background, and
/// the frame is a function of the state: the same pixels again after more
/// frames, and from a second app instance.
#[test]
fn a_mesh_draws_and_draws_identically() {
    let show = |h: &mut egui_kittest::Harness<'_, arrix_app::ArrixApp>| {
        h.state_mut()
            .show_mesh(&fixture::box_key(), &fixture::box_mesh([1.2, 0.8, 0.5]));
        h.run();
    };
    let mut first = None;
    harness::with_app("a_mesh_draws_and_draws_identically", |h| {
        let empty = harness::render(h);
        show(h);
        let drawn = harness::render(h);
        let (before, after) = (centre_pixel(h, &empty), centre_pixel(h, &drawn));
        assert_ne!(before, after, "the box covers the viewport's centre");
        // The fixture's faces are a grey blue, bluer than they are red.
        assert!(
            after[2] > after[0] + 20,
            "a face's colour, not the background's: {after:?}"
        );
        assert_eq!(
            empty.get_pixel(4, 4),
            drawn.get_pixel(4, 4),
            "the ribbon is untouched"
        );

        h.run_steps(10);
        assert!(
            harness::render(h) == drawn,
            "re-rendering the same state changed pixels"
        );
        first = Some(drawn);
    });
    harness::with_app("a_mesh_draws_and_draws_identically", |h| {
        show(h);
        let drawn = first.take().expect("first instance ran");
        assert!(
            harness::render(h) == drawn,
            "a second app instance rendered different pixels"
        );
    });
}

/// The slice, opened: its one final body (the pocketed plate) meshed
/// refined and framed by the camera, the generation unchanged, settled.
/// Pinned after reading the frame (`scripts/snapshot-baseline`).
#[test]
fn slice_viewport() {
    harness::with_app("slice_viewport", |h| {
        harness::open_settled(h, "tests/docs/slice");
        let d = h.state().debug_state();
        let evaluation = d.evaluation.expect("a document is open");
        assert!(evaluation.finished && evaluation.settled);
        assert!(evaluation.problems.is_empty(), "{:?}", evaluation.problems);
        assert_eq!(d.bodies.len(), 1, "{evaluation:?}");
        let before = harness::render(h);
        h.run_steps(5);
        assert!(
            harness::render(h) == before,
            "a settled document draws the same frame"
        );
        assert_eq!(
            h.state().debug_state().evaluation.map(|e| e.generation),
            Some(evaluation.generation),
            "meshing is not an edit"
        );
        snapshot::check(h, "slice_viewport");
    });
}

/// The tab labels of the strip, and which is active.
fn tabs(h: &egui_kittest::Harness<'_, arrix_app::ArrixApp>) -> (Vec<String>, Option<usize>) {
    let documents = h
        .state()
        .debug_state()
        .shell
        .expect("a frame ran")
        .documents;
    let active = documents.tabs.iter().position(|t| t.active);
    (
        documents.tabs.into_iter().map(|t| t.title).collect(),
        active,
    )
}

fn camera(h: &egui_kittest::Harness<'_, arrix_app::ArrixApp>) -> serde_json::Value {
    serde_json::to_value(h.state().debug_state().camera).expect("a camera")
}

/// Two documents, a tab each: the second shows its own body, camera and
/// cards, switching by click and by key brings the first back as it was,
/// and closing the last returns to the empty start.
#[test]
fn document_tabs() {
    use egui_kittest::kittest::Queryable as _;
    let source = arrix_config::MemorySource::with([(
        "keybindings.toml",
        "[[bind]]\nkey = \"f4\"\ncommand = \"core.panel.tree.toggle\"\n",
    )]);
    harness::with_config("document_tabs", &source, |h| {
        harness::open_settled(h, "tests/docs/slice");
        let slice_camera = camera(h);
        h.key_press(egui::Key::F4);
        h.run();
        assert!(h.state().debug_state().layout.unwrap().tree.is_some());

        harness::open_settled(h, "tests/docs/gear-on-plane");
        assert_eq!(
            tabs(h),
            (vec!["slice".into(), "gear-on-plane".into()], Some(1))
        );
        let d = h.state().debug_state();
        assert_eq!(
            d.evaluation.map(|e| e.document).as_deref(),
            Some("gear-on-plane")
        );
        assert_ne!(camera(h), slice_camera, "a camera of its own");
        assert_eq!(
            d.layout.unwrap().tree,
            None,
            "the cards stay with their tab"
        );

        h.get_by_label("slice").click();
        h.run();
        assert_eq!(tabs(h).1, Some(0));
        assert_eq!(camera(h), slice_camera, "the camera as it was");
        assert!(h.state().debug_state().layout.unwrap().tree.is_some());

        h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::Tab);
        h.run();
        assert_eq!(tabs(h).1, Some(1));
        h.key_press_modifiers(
            egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
            egui::Key::Tab,
        );
        h.run();
        assert_eq!(tabs(h).1, Some(0));
        snapshot::check(h, "document_tabs");

        h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::W);
        h.run();
        assert_eq!(tabs(h), (vec!["gear-on-plane".into()], Some(0)));
        assert_eq!(h.state().debug_state().bodies.len(), 1);
        h.get_by_label("×").click();
        h.run();
        let d = h.state().debug_state();
        assert_eq!(tabs(h), (vec![], None));
        assert!(d.evaluation.is_none() && d.bodies.is_empty() && d.camera.is_none());
    });
}
