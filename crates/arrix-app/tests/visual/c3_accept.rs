//! C3's accept in the app (plans/c3-m3 step 16): the ribbon's `Place
//! screws` on the north star, `tests/docs/north-star`, with the three
//! counterbores' rims picked. OK expands and submits one command; a screw
//! stands on each hole beside the servo and the horn, and one undo takes
//! them all.

use arrix_ui::FieldValue;
use egui_kittest::kittest::Queryable as _;

use crate::viewport::click as click_viewport;
use crate::{harness, snapshot};

type Harness<'a> = egui_kittest::Harness<'a, arrix_app::ArrixApp>;

const PLUGIN: &str = "plugins/fasteners";
const DOC: &str = "tests/docs/north-star";
/// The rim of the three counterbores (4 mm in radius) on the plate's
/// top, at x = 24 mm; the holes are 10 mm apart in y.
const FIRST_RIM: [f64; 3] = [0.024, -0.012, 0.005];
const SECOND_RIM: [f64; 3] = [0.024, -0.002, 0.005];
const THIRD_RIM: [f64; 3] = [0.024, 0.008, 0.005];

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

/// The servo sits over the plate's holes; the hinge turns it off them.
fn turn_servo_aside(h: &mut Harness<'_>, angle: &str) {
    let assembly = saved_assembly(h);
    let mut joint: arrix_doc::Joint =
        serde_json::from_value(assembly["joints"]["EC2HEE1BT7K57"].clone()).unwrap();
    joint.value = angle.parse().unwrap();
    let command = arrix_doc::Command::EditJoint {
        assembly: assembly["id"].as_str().unwrap().parse().unwrap(),
        joint: Box::new(joint),
    };
    h.state_mut().submit(command).unwrap();
    h.run_steps(12);
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

/// Steps until the saved assembly holds `count` components.
fn wait_for_components(h: &mut Harness<'_>, count: usize) {
    let started = web_time::Instant::now();
    while saved_assembly(h)["components"].as_object().unwrap().len() != count {
        assert!(
            started.elapsed() < std::time::Duration::from_secs(60),
            "the screws were not placed: {}",
            h.state().debug_state_json()
        );
        h.step();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
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
fn c3_accept() {
    harness::with_app("c3_accept", |h| {
        h.state_mut().seed_ids(1010);
        harness::open_settled_with(h, DOC, &[PLUGIN]);
        h.set_render_every_step(true);
        h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::Num7);
        h.run_steps(4);
        turn_servo_aside(h, "90 deg");
        let before = saved_assembly(h);
        let components = before["components"].as_object().unwrap().len();
        let joints = before["joints"].as_object().unwrap().len();

        h.get_all_by_label("Fasteners").next().unwrap().click();
        h.run_steps(3);
        h.get_by_label("Place screws").click();
        h.run_steps(4);
        pick_rim(h, FIRST_RIM, false);
        pick_rim(h, SECOND_RIM, true);
        pick_rim(h, THIRD_RIM, true);
        assert_eq!(picks(h), 3, "{:?}", form(h));
        assert_eq!(h.state().debug_state().gesture_commands, Some(0));

        // The counterbores are 8 mm: the size is M3, as the accept's
        // command does, so the heads fit.
        h.get_all_by_role(egui::accesskit::Role::ComboBox)
            .next()
            .unwrap()
            .click();
        h.run_steps(3);
        h.get_by_label("M3 socket head").click();
        h.run_steps(3);
        h.get_by_label("OK").click();
        harness::wait_settled(h);
        // The document can read as settled before the expansion's command
        // is submitted, so wait for the screws themselves.
        wait_for_components(h, components + 3);
        h.run_steps(4);
        let after = saved_assembly(h);
        assert_eq!(
            after["components"].as_object().unwrap().len(),
            components + 3
        );
        assert_eq!(after["joints"].as_object().unwrap().len(), joints + 3);
        assert_eq!(h.state().debug_state().gesture_commands, Some(1));
        snapshot::check(h, "c3_accept");

        h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::Z);
        harness::wait_settled(h);
        let undone = saved_assembly(h);
        assert_eq!(undone["components"].as_object().unwrap().len(), components);
    });
}
