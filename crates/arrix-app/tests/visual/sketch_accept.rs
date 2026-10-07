//! Sketch mode's acceptance (plans/sketch-mode): a part's sketch made
//! through the UI by pointer and keys only, every gesture counted, the
//! regions shaded as the evaluator takes them, and every command undone in
//! reverse to the byte.

use arrix_doc::{Evaluator, FeatureOutcome, Registry, SlotView};
use egui::{Key, Modifiers};
use egui_kittest::kittest::Queryable as _;

use crate::{harness, snapshot};

type Harness<'a> = egui_kittest::Harness<'a, arrix_app::ArrixApp>;

const MM: f64 = 1e-3;

fn wait(h: &mut Harness<'_>, what: &str, done: impl Fn(&Harness<'_>) -> bool) {
    let started = web_time::Instant::now();
    while !done(h) {
        h.step();
        assert!(
            started.elapsed() < std::time::Duration::from_secs(60),
            "{what}: timed out\n{}",
            h.state().debug_state_json()
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.run_steps(2);
}

fn at(h: &Harness<'_>, uv: [f64; 2]) -> egui::Pos2 {
    h.state()
        .sketch_point([uv[0] * MM, uv[1] * MM])
        .expect("the sketch is drawn")
}

fn hover(h: &mut Harness<'_>, uv: [f64; 2]) {
    let pos = at(h, uv);
    h.event(egui::Event::PointerMoved(pos));
    h.run_steps(3);
}

fn button(h: &mut Harness<'_>, pos: egui::Pos2, pressed: bool) {
    h.event(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    });
    h.step();
}

fn click(h: &mut Harness<'_>, uv: [f64; 2]) {
    let pos = at(h, uv);
    h.event(egui::Event::PointerMoved(pos));
    h.step();
    button(h, pos, true);
    button(h, pos, false);
    h.run_steps(3);
}

/// A double-click: two clicks a frame apart. This harness steps a quarter
/// second a frame, so the delay egui allows between them is widened (not
/// so far that the click before them makes a triple, which a pause rules
/// out).
fn double_click(h: &mut Harness<'_>, uv: [f64; 2]) {
    h.ctx
        .options_mut(|o| o.input_options.max_double_click_delay = 0.6);
    h.run_steps(8);
    let pos = at(h, uv);
    h.event(egui::Event::PointerMoved(pos));
    h.step();
    for pressed in [true, false, true, false] {
        button(h, pos, pressed);
    }
    h.run_steps(3);
}

/// A press at `from`, the pointer walked to `to`, a release there.
fn drag(h: &mut Harness<'_>, from: [f64; 2], to: [f64; 2]) {
    let (a, b) = (at(h, from), at(h, to));
    h.event(egui::Event::PointerMoved(a));
    h.step();
    button(h, a, true);
    for i in 1..=5 {
        let t = i as f32 / 5.0;
        h.event(egui::Event::PointerMoved(a + (b - a) * t));
        h.step();
    }
    button(h, b, false);
    h.run_steps(3);
}

fn key(h: &mut Harness<'_>, key: Key) {
    h.key_press(key);
    h.run_steps(2);
}

fn undo(h: &mut Harness<'_>) {
    h.key_press_modifiers(Modifiers::CTRL, Key::Z);
    h.run_steps(4);
}

fn gesture(h: &Harness<'_>) -> Option<usize> {
    h.state().debug_state().gesture_commands
}

fn view(h: &Harness<'_>) -> arrix_ui::SketchView {
    h.state()
        .debug_state()
        .sketch_view
        .expect("sketch mode is on")
}

/// Waits until the overlay draws `curves` curves: the edit is evaluated.
fn drawn(h: &mut Harness<'_>, curves: usize) {
    wait(h, "the edit was drawn", |h| {
        h.state()
            .debug_state()
            .sketch_view
            .is_some_and(|v| v.curves.len() == curves)
    });
}

/// What the preview's glyphs commit, sorted.
fn shown(h: &Harness<'_>) -> Vec<String> {
    let mut kinds: Vec<String> = view(h)
        .preview
        .expect("a drawing tool's preview")
        .glyphs
        .into_iter()
        .flat_map(|g| g.commits)
        .collect();
    kinds.sort();
    kinds.dedup();
    kinds
}

/// The stored sketch's constraint kinds, sorted.
fn stored(h: &Harness<'_>) -> Vec<String> {
    let saved = h.state().saved().expect("a document is open").0;
    let sketch = saved
        .iter()
        .filter(|(path, _)| path.starts_with("parts/"))
        .find_map(|(_, bytes)| {
            let part: serde_json::Value = serde_json::from_slice(bytes).ok()?;
            let features = part["features"].as_object()?;
            features.values().find_map(|f| f.get("sketch").cloned())
        })
        .expect("a sketch is stored");
    let mut kinds: Vec<String> = sketch["constraints"]
        .as_object()
        .map(|o| {
            o.values()
                .filter_map(|r| r["constraint"].as_object()?.keys().next().cloned())
                .collect()
        })
        .unwrap_or_default();
    kinds.sort();
    kinds
}

fn dimensions(h: &Harness<'_>) -> Vec<(String, String)> {
    view(h)
        .dimensions
        .into_iter()
        .map(|d| (d.kind, d.text))
        .collect()
}

/// The keys of the regions the evaluator finds in the sketch of `source`.
fn evaluated(source: &arrix_doc::MemorySource) -> (Vec<String>, usize) {
    let document = arrix_doc::open(source).expect("it opens");
    let evaluation = Evaluator::new(Registry::with_core_types()).evaluate(&document);
    let mut keys = Vec::new();
    let mut diagnostics = 0;
    for feature in document.parts().values().flat_map(|p| p.features.keys()) {
        let Some(outcome) = evaluation.outcome(*feature) else {
            continue;
        };
        let FeatureOutcome::Ok { slots, .. } = outcome else {
            diagnostics += 1;
            continue;
        };
        for slot in slots.values() {
            if let SlotView::Sketch(view) = slot {
                keys.extend(
                    view.regions
                        .iter()
                        .map(|r| serde_json::to_string(&r.key).unwrap()),
                );
                diagnostics += view.redundant.len();
            }
        }
    }
    keys.sort();
    (keys, diagnostics)
}

/// The whole flow of the plan's acceptance, against its golden.
#[test]
fn sketch_accept() {
    harness::with_app("sketch_accept", |h| {
        harness::open_settled(h, "tests/docs/sketch-mode");
        h.state_mut().seed_ids(1);
        // Every command's document, to undo against: the document as
        // opened first.
        let mut states = vec![h.state().saved().unwrap().0];
        // `(what, commands)` per gesture, to count them at the end.
        let mut counts: Vec<(&str, usize)> = Vec::new();
        let mut note = |h: &Harness<'_>, what: &'static str, expect: usize| {
            let got = gesture(h).expect("a gesture was counted");
            assert_eq!(got, expect, "{what}: commands in the gesture");
            counts.push((what, got));
        };
        let mut keep = |h: &Harness<'_>| states.push(h.state().saved().unwrap().0);

        // 1. A sketch, with nothing selected: the world plane, square to the
        //    camera, orthographic.
        h.get_by_label("New sketch").click();
        h.run_steps(3);
        note(h, "create the sketch", 1);
        keep(h);
        wait(h, "the camera turned", |h| {
            h.state().debug_state().sketch.is_some_and(|m| m.turned)
        });
        h.state_mut().skip_animations();
        h.run_steps(3);
        let camera = h.state().debug_state().camera.expect("a camera");
        assert_eq!(camera.projection, "Orthographic");
        assert!((camera.pitch - std::f64::consts::FRAC_PI_2).abs() < 1e-3);

        // 2. A rectangle with a corner snapped to the origin.
        key(h, Key::R);
        hover(h, [0.0, 0.0]);
        assert_eq!(
            shown(h),
            ["coincident_to_datum"],
            "the snap shows the origin"
        );
        click(h, [0.0, 0.0]);
        note(h, "the rectangle's first corner", 0);
        click(h, [50.0, 30.0]);
        note(h, "the rectangle", 1);
        keep(h);
        drawn(h, 4);
        let held = stored(h);
        assert!(
            held.contains(&"coincident_to_datum".to_owned()),
            "the corner holds what the glyph showed: {held:?}"
        );
        key(h, Key::Escape);

        // 3. The bottom edge dimensioned and set to `width`, the height to
        //    40 mm.
        key(h, Key::D);
        click(h, [25.0, 0.0]);
        note(h, "the pick of the bottom edge", 0);
        click(h, [25.0, -8.0]);
        note(h, "the width's dimension", 1);
        keep(h);
        key(h, Key::Escape);
        double_click(h, [25.0, -8.0]);
        note(h, "opening the width's field", 0);
        h.event(egui::Event::Text("width".to_owned()));
        h.run_steps(2);
        h.key_press(Key::Enter);
        h.run_steps(4);
        note(h, "the width as a parameter", 1);
        keep(h);
        drawn(h, 4);
        wait(h, "the width follows the parameter", |h| {
            dimensions(h).iter().any(|d| d.1 == "width = 60 mm")
        });

        key(h, Key::D);
        click(h, [60.0, 15.0]);
        click(h, [72.0, 15.0]);
        note(h, "the height's dimension", 1);
        keep(h);
        key(h, Key::Escape);
        double_click(h, [72.0, 15.0]);
        h.event(egui::Event::Text("40 mm".to_owned()));
        h.run_steps(2);
        h.key_press(Key::Enter);
        h.run_steps(4);
        note(h, "the height's value", 1);
        keep(h);
        drawn(h, 4);
        wait(h, "the height is 40", |h| {
            dimensions(h).iter().any(|d| d.1 == "40 mm")
        });
        assert_eq!(view(h).status.dof, 0, "the rectangle is fully constrained");

        // 4. A circle inside it, its centre dragged (one command on the
        //    release, none for a click that moves nothing), a 10 mm radius.
        key(h, Key::C);
        click(h, [30.0, 20.0]);
        note(h, "the circle's centre", 0);
        click(h, [38.0, 20.0]);
        note(h, "the circle", 1);
        keep(h);
        drawn(h, 5);
        key(h, Key::Escape);
        click(h, [30.0, 20.0]);
        note(h, "a click on the centre moves nothing", 0);
        drag(h, [30.0, 20.0], [35.0, 18.0]);
        note(h, "the centre dragged", 1);
        keep(h);
        wait(h, "the centre moved", |h| {
            view(h)
                .points
                .iter()
                .any(|p| (p.at[0] / MM - 35.0).abs() < 0.5 && (p.at[1] / MM - 18.0).abs() < 0.5)
        });
        key(h, Key::D);
        click(h, [43.0, 18.0]);
        note(h, "the pick of the circle", 0);
        click(h, [52.0, 30.0]);
        note(h, "the radius dimension", 1);
        keep(h);
        key(h, Key::Escape);
        double_click(h, [52.0, 30.0]);
        h.event(egui::Event::Text("10 mm".to_owned()));
        h.run_steps(2);
        h.key_press(Key::Enter);
        h.run_steps(4);
        note(h, "the radius value", 1);
        keep(h);
        wait(h, "the radius is 10", |h| {
            dimensions(h)
                .iter()
                .any(|d| d.0 == "radius" && d.1 == "10 mm")
        });

        // 5. A line across the rectangle, trimmed back to it at both ends,
        //    made construction, and mirrored about the left edge.
        key(h, Key::L);
        click(h, [-12.0, 34.0]);
        note(h, "the line's start", 0);
        click(h, [72.0, 34.0]);
        note(h, "the line across", 1);
        keep(h);
        drawn(h, 6);
        key(h, Key::Escape);
        key(h, Key::Escape);
        key(h, Key::T);
        click(h, [66.0, 34.0]);
        note(h, "the right end trimmed", 1);
        keep(h);
        click(h, [-6.0, 34.0]);
        note(h, "the left end trimmed", 1);
        keep(h);
        key(h, Key::Escape);
        click(h, [30.0, 34.0]);
        note(h, "selecting the line", 0);
        key(h, Key::Q);
        note(h, "the line made construction", 1);
        keep(h);
        assert!(
            view(h).curves.iter().any(|c| c.construction),
            "one curve is construction"
        );
        key(h, Key::M);
        click(h, [0.0, 20.0]);
        note(h, "the construction line mirrored", 1);
        keep(h);
        drawn(h, 7);

        // 6. The regions, shaded as extrude will take them: the rectangle
        //    less the circle, and the circle.
        wait(h, "the regions are shaded", |h| view(h).regions.len() == 2);
        let mut shaded: Vec<String> = view(h).regions.iter().map(|r| r.key.clone()).collect();
        shaded.sort();
        harness::wait_settled(h);
        h.event(egui::Event::PointerMoved(egui::pos2(1000.0, 700.0)));
        h.run_steps(3);
        snapshot::check(h, "sketch_accept");

        // 7. Finish: no command. Saved, the document evaluates to the same
        //    region keys and a sketch with no diagnostic.
        for _ in 0..3 {
            if h.state().debug_state().sketch.is_none() {
                break;
            }
            key(h, Key::Escape);
            assert_eq!(gesture(h), Some(0), "finishing issues no command");
        }
        assert!(
            h.state().debug_state().sketch.is_none(),
            "sketch mode ended"
        );
        let saved = h.state().saved().unwrap();
        let (keys, diagnostics) = evaluated(&saved);
        assert_eq!(keys, shaded, "the evaluator takes the regions shown");
        assert_eq!(diagnostics, 0, "no diagnostic");
        assert!(
            saved.0 == *states.last().unwrap(),
            "finishing stores nothing"
        );

        // 8. Every gesture one command or none, and every command undone in
        //    reverse returns the document to the byte.
        let commands = counts.iter().map(|c| c.1).sum::<usize>();
        assert_eq!(states.len(), commands + 1, "one document per command");
        for n in (0..states.len() - 1).rev() {
            undo(h);
            assert!(
                h.state().saved().unwrap().0 == states[n],
                "undo {} returns the document to state {n}",
                states.len() - 1 - n
            );
        }
    });
}
