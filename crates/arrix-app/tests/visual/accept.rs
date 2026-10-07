//! The viewport port's acceptance (plans/viewport-port): the app from the
//! empty start through two documents, a pick, the ViewCube, Escape and the
//! tabs, each document's generation unchanged throughout; and the UI frame
//! measured against its budget.

use arrix_ui::FeatureState;
use arrix_viewport::ViewOrientation;

use crate::viewport::{click as click_viewport, selection};
use crate::{camera, harness, snapshot};

type Harness<'a> = egui_kittest::Harness<'a, arrix_app::ArrixApp>;

fn tabs(h: &Harness<'_>) -> (Vec<String>, Option<usize>) {
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

fn generation(h: &Harness<'_>) -> u64 {
    h.state()
        .debug_state()
        .evaluation
        .expect("a document is open")
        .generation
}

fn camera_of(h: &Harness<'_>) -> serde_json::Value {
    serde_json::to_value(h.state().debug_state().camera).expect("a camera")
}

/// Shows the tab labelled `title`.
fn show_tab(h: &mut Harness<'_>, title: &str) {
    use egui_kittest::kittest::Queryable as _;
    h.get_by_label(title).click();
    h.run_steps(6);
}

/// The whole path, on lavapipe, against its goldens.
#[test]
fn viewport_accept() {
    harness::with_app("viewport_accept", |h| {
        // The empty start: the ribbon, the viewport, an empty tab strip and
        // the status line with its start hint, and no card.
        let d = h.state().debug_state();
        let layout = d.layout.expect("a frame ran");
        assert!(layout.tree.is_none() && layout.properties.is_none());
        assert_eq!(tabs(h), (vec![], None));
        let hint = d.shell.expect("a frame ran").status.hint;
        assert!(hint.starts_with("No document is open."), "{hint}");
        assert!(d.bodies.is_empty() && d.camera.is_none());
        snapshot::check(h, "accept_empty");

        // The slice and the gear as two tabs, the slice shown.
        h.set_render_every_step(true);
        harness::open_settled(h, "tests/docs/slice");
        let slice = generation(h);
        harness::open_settled(h, "tests/docs/gear-on-plane");
        let gear = generation(h);
        assert_eq!(
            tabs(h),
            (vec!["slice".into(), "gear-on-plane".into()], Some(1))
        );
        show_tab(h, "slice");
        assert_eq!(tabs(h).1, Some(0));

        // Settled: the slice's one final body drawn from a refined mesh, its
        // features all ok in the tree card, and no property card.
        let d = h.state().debug_state();
        let evaluation = d.evaluation.as_ref().expect("a document is open");
        assert!(evaluation.finished && evaluation.settled);
        assert!(evaluation.problems.is_empty(), "{:?}", evaluation.problems);
        assert_eq!(d.bodies.len(), 1);
        assert!(d.bodies[0].triangles > 0 && d.bodies[0].bounds.is_some());
        let layout = d.layout.as_ref().expect("a frame ran");
        assert!(layout.tree.is_some() && layout.properties.is_none());
        let features = &d.shell.as_ref().expect("a frame ran").tree.parts[0].features;
        assert!(features.iter().all(|f| f.state == FeatureState::Ok));

        // A click on the plate's top face selects its persistent name.
        let on_screen = |h: &Harness<'_>, p| h.state().screen_point(p).expect("in front");
        click_viewport(h, on_screen(h, [-0.016, -0.011, 0.005]));
        let plate = "5BE3HE4D204QM";
        assert_eq!(selection(h), Some(format!("face:sweep.{plate}.end-cap")));

        // A ViewCube click turns the camera to the top view.
        let top = h
            .state()
            .viewcube_facet(ViewOrientation::Top)
            .expect("the top facet faces the camera");
        camera::click(h, top);
        h.state_mut().skip_animations();
        h.step();
        let turned = camera_of(h);
        let half_pi = std::f64::consts::FRAC_PI_2;
        assert!((turned["pitch"].as_f64().unwrap() - half_pi).abs() < 1e-5);
        assert_eq!(
            selection(h),
            Some(format!("face:sweep.{plate}.end-cap")),
            "turning the camera does not clear the selection"
        );

        // Escape clears the selection.
        h.key_press(egui::Key::Escape);
        h.step();
        assert_eq!(selection(h), None);
        h.step();
        snapshot::check(h, "viewport_accept");

        // The gear's tab shows its own body and tree; back on the slice's,
        // the camera is still the top view.
        show_tab(h, "gear-on-plane");
        let d = h.state().debug_state();
        assert_eq!(d.bodies.len(), 1);
        assert_eq!(
            d.evaluation.as_ref().map(|e| e.document.as_str()),
            Some("gear-on-plane")
        );
        assert_ne!(camera_of(h), turned, "a camera of its own");
        assert_eq!(generation(h), gear);
        show_tab(h, "slice");
        assert_eq!(camera_of(h), turned, "the top view, as it was");
        assert_eq!(generation(h), slice, "no frame, pick or turn was an edit");
    });
}

/// One frame of the app as the CPU does it: the UI run, and egui's
/// tessellation of what it painted.
fn ui_frame(h: &mut Harness<'_>) {
    h.step();
    let output = h.output();
    let meshes = h
        .ctx
        .tessellate(output.shapes.clone(), output.pixels_per_point);
    std::hint::black_box(meshes);
}

/// The milliseconds each of `frames` calls of `frame` took, sorted.
fn time_frames(
    h: &mut Harness<'_>,
    frames: usize,
    mut frame: impl FnMut(&mut Harness<'_>),
) -> Vec<f64> {
    let mut times: Vec<f64> = (0..frames)
        .map(|_| {
            let started = web_time::Instant::now();
            frame(h);
            started.elapsed().as_secs_f64() * 1e3
        })
        .collect();
    times.sort_by(f64::total_cmp);
    times
}

fn report(what: &str, times: &[f64]) {
    let at = |q: f64| times[((times.len() - 1) as f64 * q) as usize];
    println!(
        "ui frame, slice on screen, {what}, {} frames: median {:.2} ms, p95 {:.2} ms, max {:.2} ms (target 16 ms)",
        times.len(),
        at(0.5),
        at(0.95),
        at(1.0)
    );
}

/// A UI frame with the slice on screen (the app's frame: the view built,
/// egui's layout, paint and tessellation, the viewport's and the cube's draw preparation;
/// not the GPU, which a software rasteriser says nothing about), measured
/// and printed against the 16 ms target of docs/CONCURRENCY-WASM.md
/// §Budgets: at rest, and while a middle drag orbits the camera, which
/// moves everything and asks for a hover pick each frame. Reported, not
/// asserted.
#[test]
fn ui_frame_budget() {
    harness::with_app("ui_frame_budget", |h| {
        harness::open_settled(h, "tests/docs/slice");
        let at = egui::pos2(900.0, 600.0);
        h.hover_at(at);
        h.run_steps(10);
        report("at rest", &time_frames(h, 200, ui_frame));

        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Middle,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
        let mut x = at.x;
        let orbiting = time_frames(h, 200, |h| {
            x += 3.0;
            h.hover_at(egui::pos2(x, at.y));
            ui_frame(h);
        });
        report("orbiting", &orbiting);
        let yaw = serde_json::to_value(h.state().debug_state().camera).expect("a camera")["yaw"]
            .as_f64()
            .expect("a yaw");
        assert!(yaw < -1.0 || yaw > 0.0, "the drag turned the camera: {yaw}");
    });
}
