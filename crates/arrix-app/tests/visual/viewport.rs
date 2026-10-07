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
        let tree = d.shell.as_ref().expect("a frame ran").tree.clone();
        assert!(d.layout.as_ref().expect("a frame ran").tree.is_some());
        assert!(tree.parts[0].features.len() >= 6);
        assert!(
            tree.parts[0]
                .features
                .iter()
                .all(|f| f.state == arrix_ui::FeatureState::Ok),
            "{tree:?}"
        );
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
        // `auto` shows the tree of a document with a part; F4 hides it on
        // this tab only.
        assert!(h.state().debug_state().layout.unwrap().tree.is_some());
        h.key_press(egui::Key::F4);
        h.run();
        assert_eq!(h.state().debug_state().layout.unwrap().tree, None);

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
        assert!(
            d.layout.unwrap().tree.is_some(),
            "the cards stay with their tab"
        );

        h.get_by_label("slice").click();
        h.run();
        assert_eq!(tabs(h).1, Some(0));
        assert_eq!(camera(h), slice_camera, "the camera as it was");
        assert_eq!(
            h.state().debug_state().layout.unwrap().tree,
            None,
            "its card as it was"
        );

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

/// The slice opened as a build without `gears`: the tree card shows by
/// itself (`auto`, there being a part), the gear frozen among features that
/// are fine, and its body still drawn from the stored result.
#[test]
fn the_tree_shows_a_frozen_feature() {
    use arrix_ui::FeatureState;
    harness::with_app("the_tree_shows_a_frozen_feature", |h| {
        // The gear's stored result is made by evaluating it, so the slice
        // is opened and saved first, and the saved one opened without it.
        harness::open_settled(h, "tests/docs/slice");
        let saved = h.state().saved().expect("a document is open");
        h.state_mut()
            .open_without("slice (without gears)", &saved, &["gears"])
            .expect("the saved slice opens");
        harness::wait_settled(h);
        let d = h.state().debug_state();
        let shell = d.shell.expect("a frame ran");
        assert!(
            d.layout.expect("a frame ran").tree.is_some(),
            "auto shows it"
        );
        let features = &shell.tree.parts[0].features;
        let frozen: Vec<_> = features
            .iter()
            .filter(|f| f.state == FeatureState::Frozen)
            .map(|f| f.kind.as_str())
            .collect();
        assert_eq!(frozen, ["gears.spur"], "{features:?}");
        assert!(
            features
                .iter()
                .all(|f| matches!(f.state, FeatureState::Frozen | FeatureState::Ok))
        );
        assert_eq!(d.bodies.len(), 1);
        snapshot::check(h, "tree_frozen");
    });
}

/// Clicks at `at` and steps, rendering every frame, until the pick it asked
/// for has been answered (the read-back is asynchronous). A click the
/// viewport does not take asks for no pick and returns at once.
fn click(h: &mut egui_kittest::Harness<'_, arrix_app::ArrixApp>, at: egui::Pos2) {
    h.hover_at(at);
    h.drag_at(at);
    h.drop_at(at);
    let started = web_time::Instant::now();
    loop {
        h.step();
        if !h.state().debug_state().picking {
            return;
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(10),
            "the pick was never answered"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

/// The selection's text form, if any.
fn selection(h: &egui_kittest::Harness<'_, arrix_app::ArrixApp>) -> Option<String> {
    h.state()
        .debug_state()
        .selection
        .map(|name| name.to_string())
}

/// Picking through the ID buffer: a click on the slice's plate selects its
/// top face, edge or corner by persistent name, a vertex winning over the
/// edges and faces about it; a click on the tree card picks nothing, one on
/// empty space clears the selection, and none of it is an edit.
#[test]
fn picking() {
    harness::with_app("picking", |h| {
        harness::open_settled(h, "tests/docs/slice");
        // The read-back is made when a frame is rendered.
        h.set_render_every_step(true);
        let generation = h.state().debug_state().evaluation.map(|e| e.generation);
        let on_screen = |h: &egui_kittest::Harness<'_, arrix_app::ArrixApp>, p| {
            h.state().screen_point(p).expect("in front of the camera")
        };
        // The plate is 40 × 30 × 5 mm about the origin; the gear stands on
        // its middle.
        let plate = "5BE3HE4D204QM";

        click(h, on_screen(h, [-0.016, -0.011, 0.005]));
        assert_eq!(
            selection(h),
            Some(format!("face:sweep.{plate}.end-cap")),
            "the plate's top face"
        );

        // The middle of the top's left edge, then its corner.
        click(h, on_screen(h, [-0.02, 0.0, 0.005]));
        let edge = selection(h).expect("something picked");
        assert!(edge.starts_with("edge:"), "{edge}");
        click(h, on_screen(h, [-0.02, -0.015, 0.005]));
        let corner = selection(h).expect("something picked");
        assert!(corner.starts_with("vertex:"), "{corner}");

        // The tree card is over the viewport and takes the click.
        let card = h
            .state()
            .debug_state()
            .layout
            .unwrap()
            .tree
            .expect("auto shows it");
        click(h, egui::pos2(card[0] as f32 + 20.0, card[3] as f32 - 10.0));
        assert_eq!(
            selection(h),
            Some(corner),
            "the card's click picked nothing"
        );

        // Empty space, in the viewport's bottom right.
        let v = h.state().debug_state().layout.unwrap().viewport;
        click(h, egui::pos2(v[2] as f32 - 40.0, v[3] as f32 - 40.0));
        assert_eq!(selection(h), None);

        assert_eq!(
            h.state().debug_state().evaluation.map(|e| e.generation),
            generation,
            "picking is not an edit"
        );
    });
}
