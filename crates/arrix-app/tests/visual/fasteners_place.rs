//! A plugin's command from the ribbon (plans/c3-m3 step 10, ADR-0038 §10):
//! `fasteners.place`, a Tier 2 Python plugin, on the bracket's counterbored
//! holes. Its button is on the plugin's tab, its form lists the inputs and
//! parameters of its spec, two shift-clicked hole edges fill the list, and
//! OK expands then submits: one command, one undo entry.

use arrix_ui::FieldValue;
use egui_kittest::kittest::Queryable as _;

use crate::viewport::click as click_viewport;
use crate::{harness, snapshot};

type Harness<'a> = egui_kittest::Harness<'a, arrix_app::ArrixApp>;

const PLUGIN: &str = "plugins/fasteners";
const DOC: &str = "plugins/fasteners/fixtures/bracket";
/// The rim of the first two counterbores (4 mm in radius) on the plate's
/// top, at x = 24 mm; the holes are 10 mm apart in y.
const FIRST_RIM: [f64; 3] = [0.024, -0.012, 0.005];
const SECOND_RIM: [f64; 3] = [0.024, -0.002, 0.005];

fn click(h: &mut Harness<'_>, at: egui::Pos2) {
    click_viewport(h, at);
    h.run_steps(3);
}

fn shift_click(h: &mut Harness<'_>, at: egui::Pos2) {
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::SHIFT));
    h.step();
    click(h, at);
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    h.step();
}

/// Clicks around `rim` until the click lands on an edge of the bracket's
/// counterbore, so the pick is the rim and not the face beside it.
fn pick_rim(h: &mut Harness<'_>, rim: [f64; 3], shift: bool) {
    let at = h.state().screen_point(rim).expect("in front");
    for d in [
        0.0f32, -1.0, 1.0, -2.0, 2.0, -3.0, 3.0, -4.0, 4.0, -6.0, 6.0,
    ] {
        let there = at + egui::vec2(d, 0.0);
        if shift {
            shift_click(h, there);
        } else {
            click(h, there);
        }
        let landed = h
            .state()
            .last_click()
            .is_some_and(|hit| hit.name.kind == arrix_core::TopoKind::Edge && hit.path.len() == 1);
        if landed && picks(h) > 0 {
            return;
        }
    }
    panic!("no edge found near {rim:?}");
}

fn form(h: &Harness<'_>) -> arrix_ui::FormView {
    h.state()
        .debug_state()
        .shell
        .unwrap()
        .form
        .expect("a form is open")
}

/// The saved document's one assembly.
fn saved_assembly(h: &Harness<'_>) -> serde_json::Value {
    let saved = h.state().saved().expect("a document").0;
    let (_, bytes) = saved
        .iter()
        .find(|(p, _)| p.starts_with("assemblies/"))
        .expect("an assembly is saved");
    serde_json::from_slice(bytes).unwrap()
}

fn picks(h: &Harness<'_>) -> usize {
    match &form(h)
        .fields
        .iter()
        .find(|f| f.name == "holes")
        .unwrap()
        .value
    {
        FieldValue::Reference { values, .. } => values.len(),
        other => panic!("holes is {other:?}"),
    }
}

#[test]
fn fasteners_place_from_the_ribbon_on_two_picked_holes() {
    harness::with_app("fasteners_place", |h| {
        h.state_mut().seed_ids(1010);
        harness::open_settled_with(h, DOC, &[PLUGIN]);
        h.set_render_every_step(true);
        h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::Num7);
        h.run_steps(4);

        // The command is a button on the plugin's tab.
        h.get_all_by_label("Fasteners").next().unwrap().click();
        h.run_steps(3);
        h.get_by_label("Place screws").click();
        h.run_steps(4);
        let f = form(h);
        assert!(f.create && f.title == "Place screws" && !f.named, "{f:?}");
        let names: Vec<&str> = f.fields.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["size", "holes", "length"]);
        assert_eq!(picks(h), 0);
        assert_eq!(h.state().debug_state().gesture_commands, Some(0));

        // Two picks fill the list; the form issues nothing.
        pick_rim(h, FIRST_RIM, false);
        pick_rim(h, SECOND_RIM, true);
        assert_eq!(picks(h), 2, "{:?}", form(h));
        assert_eq!(h.state().debug_state().gesture_commands, Some(0));
        snapshot::check(h, "fasteners_place_form");

        // OK expands, then submits: one command, and the screws stand.
        h.get_by_label("OK").click();
        harness::wait_settled(h);
        h.run_steps(4);
        let assembly = saved_assembly(h);
        assert_eq!(assembly["components"].as_object().unwrap().len(), 3);
        assert_eq!(assembly["joints"].as_object().unwrap().len(), 2);
        assert!(h.state().debug_state().shell.unwrap().form.is_none());
        assert_eq!(h.state().debug_state().gesture_commands, Some(1));
        snapshot::check(h, "fasteners_place");

        // One undo takes them all.
        h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::Z);
        harness::wait_settled(h);
        let assembly = saved_assembly(h);
        assert_eq!(assembly["components"].as_object().unwrap().len(), 1);
    });
}
