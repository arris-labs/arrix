//! The forms' acceptance (plans/forms): a plate made, edited, filleted and
//! re-bound through the property card by pointer and keys only, every
//! gesture counted, against its golden.

use arrix_ui::{FeatureState, FieldValue, FormView};
use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::kittest::Queryable as _;

use crate::viewport::click as click_viewport;
use crate::{harness, snapshot};

type Harness<'a> = egui_kittest::Harness<'a, arrix_app::ArrixApp>;

const MM: f64 = 1e-3;

pub(crate) fn wait(h: &mut Harness<'_>, what: &str, done: impl Fn(&Harness<'_>) -> bool) {
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

fn form(h: &Harness<'_>) -> Option<FormView> {
    h.state().debug_state().shell?.form
}

fn gesture(h: &Harness<'_>) -> Option<usize> {
    h.state().debug_state().gesture_commands
}

fn field<'a>(f: &'a FormView, name: &str) -> &'a arrix_ui::FormField {
    f.fields
        .iter()
        .find(|x| x.name == name)
        .unwrap_or_else(|| panic!("no field {name} in {:?}", f.fields))
}

fn text(f: &FormView, name: &str) -> String {
    match &field(f, name).value {
        FieldValue::Quantity { text, .. } => text.clone(),
        other => panic!("{name} is {other:?}"),
    }
}

fn refs(f: &FormView, name: &str) -> Vec<(String, bool)> {
    match &field(f, name).value {
        FieldValue::Reference { values, .. } => {
            values.iter().map(|v| (v.label.clone(), v.lost)).collect()
        }
        other => panic!("{name} is {other:?}"),
    }
}

/// Waits until the active form's header says `state`.
fn wait_state(h: &mut Harness<'_>, state: FeatureState) -> FormView {
    wait(h, &format!("the form says {state:?}"), |h| {
        form(h).is_some_and(|f| f.state == Some(state))
    });
    form(h).unwrap()
}

fn sketch_at(h: &Harness<'_>, uv: [f64; 2]) -> egui::Pos2 {
    h.state()
        .sketch_point([uv[0] * MM, uv[1] * MM])
        .expect("the sketch is drawn")
}

pub(crate) fn button(h: &mut Harness<'_>, pos: egui::Pos2, pressed: bool) {
    h.event(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    });
    h.step();
}

fn sketch_click(h: &mut Harness<'_>, uv: [f64; 2]) {
    let pos = sketch_at(h, uv);
    h.event(egui::Event::PointerMoved(pos));
    h.step();
    button(h, pos, true);
    button(h, pos, false);
    h.run_steps(3);
}

fn key(h: &mut Harness<'_>, key: Key) {
    h.key_press(key);
    h.run_steps(2);
}

/// A double-click on `pos`; this harness steps a quarter second a frame,
/// so the delay egui allows between the clicks is widened.
pub(crate) fn double_click(h: &mut Harness<'_>, pos: egui::Pos2) {
    h.ctx
        .options_mut(|o| o.input_options.max_double_click_delay = 0.6);
    h.run_steps(8);
    h.event(egui::Event::PointerMoved(pos));
    h.step();
    for pressed in [true, false, true, false] {
        button(h, pos, pressed);
    }
    h.run_steps(3);
}

pub(crate) fn open_tree_row(h: &mut Harness<'_>, name: &str) {
    let at = h.get_by_label(name).rect().center();
    double_click(h, at);
}

/// Types `text` into the form's `n`th text box and commits it.
pub(crate) fn type_into(h: &mut Harness<'_>, n: usize, text: &str) {
    h.get_all_by_role(Role::TextInput).nth(n).unwrap().focus();
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::CTRL | Modifiers::COMMAND, Key::A);
    h.run_steps(2);
    h.get_all_by_role(Role::TextInput)
        .nth(n)
        .unwrap()
        .type_text(text);
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
}

fn open_form(h: &mut Harness<'_>, tab: &str, button: &str) {
    h.get_all_by_label(tab).next().unwrap().click();
    h.run_steps(3);
    h.get_by_label(button).click();
    h.run_steps(3);
}

fn world_click(h: &mut Harness<'_>, p: [f64; 3]) {
    let at = h.state().screen_point(p).expect("in front of the camera");
    click_viewport(h, at);
    h.run_steps(3);
}

fn bodies(h: &Harness<'_>) -> Vec<(String, u32)> {
    h.state()
        .debug_state()
        .bodies
        .iter()
        .map(|b| (b.feature.clone(), b.triangles))
        .collect()
}

#[test]
fn forms_accept() {
    harness::with_app("forms_accept", |h| {
        harness::open_settled(h, "tests/docs/sketch-mode");
        h.state_mut().seed_ids(1);
        h.set_render_every_step(true);
        let mut counts: Vec<(&str, usize)> = Vec::new();
        let mut note = |h: &Harness<'_>, what: &'static str, expect: usize| {
            let got = gesture(h).expect("a gesture was counted");
            assert_eq!(got, expect, "{what}: commands in the gesture");
            counts.push((what, got));
        };

        // The plate's sketch, drawn in sketch mode: a 50 × 30 mm rectangle.
        h.get_by_label("New sketch").click();
        h.run_steps(3);
        wait(h, "the camera turned", |h| {
            h.state().debug_state().sketch.is_some_and(|m| m.turned)
        });
        h.state_mut().skip_animations();
        h.run_steps(3);
        key(h, Key::R);
        sketch_click(h, [0.0, 0.0]);
        sketch_click(h, [50.0, 30.0]);
        wait(h, "the rectangle", |h| {
            h.state()
                .debug_state()
                .sketch_view
                .is_some_and(|v| v.curves.len() == 4)
        });
        key(h, Key::Escape);
        key(h, Key::Escape);
        key(h, Key::Escape);
        assert!(h.state().debug_state().sketch.is_none());
        harness::wait_settled(h);

        // 1. New extrude: the region picked in the viewport, the distance
        //    typed; the preview shows the plate before OK.
        open_form(h, "Part", "New extrude");
        note(h, "opening the extrude form", 0);
        world_click(h, [0.025, 0.015, 0.0]);
        note(h, "the region pick", 0);
        let f = form(h).unwrap();
        assert_eq!(
            refs(&f, "region"),
            [("Region of Sketch 1".to_owned(), false)]
        );
        type_into(h, 1, "12 mm");
        note(h, "the distance typed", 0);
        wait(h, "the preview", |h| {
            h.state()
                .debug_state()
                .preview
                .as_ref()
                .is_some_and(|p| p.settled)
        });
        let previewed = h.state().debug_state().preview.unwrap().bodies;
        assert_eq!(previewed.len(), 1, "the preview shows the plate");
        h.get_by_label("OK").click();
        h.run_steps(3);
        note(h, "OK", 1);
        harness::wait_settled(h);
        let plate = bodies(h);
        assert_eq!(
            plate,
            [(previewed[0].feature.clone(), previewed[0].triangles)],
            "the committed body has the preview's counts"
        );

        // 2. The extrude edited from the tree: a document parameter is one
        //    command, a text of the wrong unit is refused in the field with
        //    its code and issues none, and a mode shows its own fields.
        open_tree_row(h, "Extrude 1");
        note(h, "opening the edit form", 0);
        type_into(h, 1, "width");
        note(h, "the distance as a parameter", 1);
        assert_eq!(text(&form(h).unwrap(), "distance"), "width");
        type_into(h, 1, "12 deg");
        note(h, "a text of the wrong unit", 0);
        let f = form(h).unwrap();
        let why = field(&f, "distance").message.clone().expect("a reason");
        assert!(why.starts_with("expr.unit-mismatch"), "{why}");
        let pick = |h: &mut Harness<'_>, option: &str| {
            h.get_all_by_role(Role::ComboBox).next().unwrap().click();
            h.run_steps(2);
            h.get_by_label(option).click();
            h.run_steps(3);
        };
        pick(h, "Cut");
        note(h, "the mode cut", 1);
        assert!(form(h).unwrap().fields.iter().any(|f| f.name == "target"));
        pick(h, "New body");
        note(h, "the mode new again", 1);
        assert!(!form(h).unwrap().fields.iter().any(|f| f.name == "target"));
        type_into(h, 1, "12 mm");
        note(h, "the distance back to 12 mm", 1);
        key(h, Key::Escape);
        harness::wait_settled(h);

        // 3. A spur gear on a datum plane picked in the tree.
        open_form(h, "Sketch", "New datum plane");
        h.get_by_label("OK").click();
        h.run_steps(3);
        note(h, "the datum plane", 1);
        open_form(h, "Gears", "New spur gear");
        h.get_by_label("Datum plane 1").click();
        h.run_steps(3);
        note(h, "the plane picked in the tree", 0);
        type_into(h, 1, "24");
        note(h, "the teeth", 0);
        h.get_by_label("OK").click();
        h.run_steps(3);
        note(h, "the gear", 1);
        harness::wait_settled(h);
        open_tree_row(h, "Spur gear 1");
        let f = form(h).expect("the gear's form");
        assert_eq!(text(&f, "teeth"), "24");
        assert_eq!(
            refs(&f, "plane"),
            [("Datum plane 1 (plane)".to_owned(), false)]
        );
        key(h, Key::Escape);

        // 4. A fillet on an edge of the plate, too big for it.
        open_form(h, "Part", "New fillet");
        world_click(h, [0.025, 0.015, 0.012]);
        h.get_by_label("Edges").click();
        h.run_steps(3);
        world_click(h, [0.025, 0.0, 0.012]);
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
        open_tree_row(h, "Fillet 1");
        let f = wait_state(h, FeatureState::Failed);
        assert_eq!(
            f.diagnostic.expect("the diagnostic").code,
            "kernel.degenerate.blend-too-large"
        );
        type_into(h, 1, "1 mm");
        note(h, "the radius edited", 1);
        let f = wait_state(h, FeatureState::Ok);
        assert_eq!(f.diagnostic, None);
        key(h, Key::Escape);

        // 5. The plate's sketch redrawn so the filleted edge goes: the
        //    bottom line deleted and drawn again. The extrude's region and
        //    then the fillet's edge are lost, each with candidates, and
        //    each is re-bound by one command, never by itself.
        open_tree_row(h, "Sketch 1");
        wait(h, "the camera turned", |h| {
            h.state().debug_state().sketch.is_some_and(|m| m.turned)
        });
        h.state_mut().skip_animations();
        h.run_steps(3);
        sketch_click(h, [25.0, 0.0]);
        key(h, Key::Delete);
        note(h, "the bottom line deleted", 1);
        key(h, Key::L);
        sketch_click(h, [0.0, 0.0]);
        sketch_click(h, [50.0, 0.0]);
        note(h, "the line drawn again", 1);
        key(h, Key::Escape);
        key(h, Key::Escape);
        key(h, Key::Escape);
        assert!(h.state().debug_state().sketch.is_none());
        open_tree_row(h, "Extrude 1");
        let f = wait_state(h, FeatureState::Failed);
        assert_eq!(f.diagnostic.as_ref().expect("the loss").code, "ref.lost");
        assert!(refs(&f, "region")[0].1, "the region is lost");
        h.get_by_label("1. Region of Sketch 1").click();
        h.run_steps(3);
        note(h, "the region re-picked", 1);
        wait_state(h, FeatureState::Ok);
        key(h, Key::Escape);
        open_tree_row(h, "Fillet 1");
        let f = wait_state(h, FeatureState::Failed);
        assert_eq!(f.diagnostic.as_ref().expect("the loss").code, "ref.lost");
        assert!(refs(&f, "edges")[0].1, "the edge is lost");
        assert_eq!(gesture(h), Some(0), "nothing re-bound by itself");
        h.event(egui::Event::PointerMoved(egui::pos2(700.0, 800.0)));
        h.run_steps(3);
        snapshot::check(h, "forms_accept");
        h.get_by_label("1. Edge of Extrude 1").click();
        h.run_steps(3);
        note(h, "the edge re-picked", 1);
        let f = wait_state(h, FeatureState::Ok);
        assert!(!refs(&f, "edges")[0].1);
        harness::wait_settled(h);
        assert_eq!(bodies(h).len(), 2, "the filleted plate and the gear");

        // 6. Every gesture was one command or none.
        for (what, n) in &counts {
            assert!(*n <= 1, "{what}: {n} commands");
        }
        let _ = counts;
    });
}
