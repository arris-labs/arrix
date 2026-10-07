//! References picked into a form's fields (plans/forms step 7): the active
//! field decides what a click in the scene or the tree means, a pick that
//! fits nothing says what the field takes, and an edit form's pick is one
//! command. Logic through the debug state; no golden.

use arrix_ui::{FieldValue, FormView};
use egui_kittest::kittest::Queryable as _;

use crate::harness;
use crate::viewport::click as click_viewport;

type Harness<'a> = egui_kittest::Harness<'a, arrix_app::ArrixApp>;

fn form(h: &Harness<'_>) -> Option<FormView> {
    h.state().debug_state().shell?.form
}

/// A reference field's value labels and whether it is the active one.
fn refs(h: &Harness<'_>, name: &str) -> (Vec<String>, bool) {
    let f = form(h).expect("a form is open");
    match &f
        .fields
        .iter()
        .find(|x| x.name == name)
        .expect("a field")
        .value
    {
        FieldValue::Reference { values, active, .. } => {
            (values.iter().map(|v| v.label.clone()).collect(), *active)
        }
        other => panic!("{name} is {other:?}"),
    }
}

fn gestures(h: &Harness<'_>) -> Option<usize> {
    h.state().debug_state().gesture_commands
}

fn hint(h: &Harness<'_>) -> String {
    h.state().debug_state().shell.unwrap().status.hint
}

fn open_form(h: &mut Harness<'_>, tab: &str, button: &str) {
    h.get_all_by_label(tab).next().unwrap().click();
    h.run_steps(3);
    h.get_by_label(button).click();
    h.run_steps(3);
}

fn on_screen(h: &Harness<'_>, p: [f64; 3]) -> egui::Pos2 {
    h.state().screen_point(p).expect("in front of the camera")
}

/// Clicks the viewport, and lets the answer reach the form.
fn click(h: &mut Harness<'_>, at: egui::Pos2) {
    click_viewport(h, at);
    h.run_steps(3);
}

/// Clicks the viewport with shift held.
fn shift_click(h: &mut Harness<'_>, at: egui::Pos2) {
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::SHIFT));
    h.step();
    click(h, at);
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    h.step();
}

const TOP: [f64; 3] = [-0.016, -0.011, 0.005];
const LEFT_EDGE: [f64; 3] = [-0.02, 0.0, 0.005];
const FAR_EDGE: [f64; 3] = [0.02, 0.0, 0.005];

#[test]
fn a_pick_goes_to_the_active_field_as_it_takes_it() {
    harness::with_app("form_pick", |h| {
        harness::open_settled(h, "tests/docs/slice");
        h.set_render_every_step(true);
        open_form(h, "Part", "New fillet");
        assert_eq!(refs(h, "target"), (vec![], true), "the first field asks");

        // A face of the body is the body.
        click(h, on_screen(h, TOP));
        let (target, _) = refs(h, "target");
        assert_eq!(target.len(), 1, "{target:?}");
        assert!(target[0].ends_with("(body)"), "{target:?}");
        assert_eq!(
            h.state().debug_state().selection,
            None,
            "it selects nothing"
        );
        assert_eq!(gestures(h), Some(0));

        // A face is not an edge.
        h.get_by_label("Edges").click();
        h.run_steps(3);
        assert_eq!(refs(h, "edges"), (vec![], true));
        click(h, on_screen(h, TOP));
        assert_eq!(refs(h, "edges").0, Vec::<String>::new());
        let say = hint(h);
        assert!(say.contains("Edges") && say.contains("edge"), "{say}");

        // Edges: a click, then shift adds and removes.
        click(h, on_screen(h, LEFT_EDGE));
        assert_eq!(refs(h, "edges").0.len(), 1);
        shift_click(h, on_screen(h, FAR_EDGE));
        println!(
            "DBG {:?} {} {:?}",
            refs(h, "edges"),
            hint(h),
            on_screen(h, FAR_EDGE)
        );
        assert_eq!(refs(h, "edges").0.len(), 2, "{:?}", refs(h, "edges"));
        shift_click(h, on_screen(h, FAR_EDGE));
        assert_eq!(refs(h, "edges").0.len(), 1);
        assert_eq!(gestures(h), Some(0), "a draft's picks issue none");
    });
}

/// This harness steps a quarter second a frame, so the delay egui allows
/// between the clicks is widened.
fn double_click(h: &mut Harness<'_>, at: egui::Pos2) {
    h.ctx
        .options_mut(|o| o.input_options.max_double_click_delay = 0.6);
    h.run_steps(8);
    h.event(egui::Event::PointerMoved(at));
    h.step();
    for pressed in [true, false, true, false] {
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
    }
    h.run_steps(3);
}

#[test]
fn the_selection_seeds_the_tree_picks_and_an_edit_pick_is_one_command() {
    harness::with_app("form_pick_tree", |h| {
        harness::open_settled(h, "tests/docs/slice");
        h.set_render_every_step(true);

        // The face selected when the command runs fills the field it fits.
        click(h, on_screen(h, TOP));
        open_form(h, "Gears", "New spur gear");
        let (plane, active) = refs(h, "plane");
        assert_eq!(
            (plane.as_slice(), active),
            (&["Face of Plate".to_owned()][..], true)
        );
        assert_eq!(gestures(h), Some(0));

        // A datum plane in the tree replaces it.
        h.get_by_label("Plate top").click();
        h.run_steps(3);
        assert_eq!(refs(h, "plane").0, ["Plate top (plane)"]);
        assert_eq!(gestures(h), Some(0));

        // A feature that is no plane says what the field takes.
        h.get_by_label("Pocket").click();
        h.run_steps(3);
        assert_eq!(refs(h, "plane").0, ["Plate top (plane)"]);
        assert!(hint(h).contains("Plane"), "{}", hint(h));

        // The remove button empties the field, still no command.
        // The tab's close button is another ×; the form's is rightmost.
        let remove = h
            .get_all_by_label("×")
            .max_by(|a, b| a.rect().min.x.total_cmp(&b.rect().min.x))
            .expect("a remove button");
        remove.click();
        h.run_steps(3);
        assert!(refs(h, "plane").0.is_empty());
        h.get_by_label("Cancel").click();
        h.run_steps(3);

        // An edit form's pick is one command.
        let at = h.get_by_label("Spur gear").rect().center();
        double_click(h, at);
        assert!(!form(h).expect("the gear's form").create);
        assert_eq!(refs(h, "plane").0, ["Plate top (plane)"]);
        h.get_by_label("Plane").click();
        h.run_steps(3);
        click(h, on_screen(h, TOP));
        assert_eq!(refs(h, "plane").0, ["Face of Plate"]);
        assert_eq!(gestures(h), Some(1), "the pick is one EditFeature");
        click(h, on_screen(h, TOP));
        assert_eq!(gestures(h), Some(0), "the same value is none");
    });
}
