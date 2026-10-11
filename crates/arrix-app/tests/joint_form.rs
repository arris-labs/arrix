//! The joint's form (plans/c2-m2 step 9, ADR-0030 §5): a draft needs both
//! anchors before OK, a double-click on a joint row edits it field by
//! field, one `EditJoint` each, and an anchor whose face is gone shows its
//! candidates, and a pick of the user's replaces it. The create form's
//! picks and look are `visual joint_form`.

mod common;

use arrix_app::ArrixApp;
use arrix_core::FeatureId;
use arrix_doc::Command;
use arrix_ui::{FieldValue, FormView};
use common::{gesture, run_command, settle};
use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::{Harness, kittest::Queryable as _};

/// The scenario's bracket's bore (a cut downstream of its plate).
const BORE: &str = "7TZ1340B9RSMB";

fn app() -> Harness<'static, ArrixApp> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1440.0, 900.0))
        .with_step_dt(0.02)
        .build_eframe(|cc| ArrixApp::new(cc));
    h.run();
    h.state_mut().seed_ids(11);
    h.state_mut()
        .open_path_without(std::path::Path::new("../../tests/docs/assembly"), &[])
        .expect("the scenario opens");
    settle(&mut h);
    h.run_steps(5);
    h
}

fn form(h: &Harness<'_, ArrixApp>) -> Option<FormView> {
    h.state().debug_state().shell?.form
}

fn field<'a>(form: &'a FormView, name: &str) -> &'a FieldValue {
    &form
        .fields
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no field {name}"))
        .value
}

fn text(form: &FormView, name: &str) -> String {
    match field(form, name) {
        FieldValue::Quantity { text, .. } => text.clone(),
        other => panic!("{name} is {other:?}"),
    }
}

fn type_into(h: &mut Harness<'_, ArrixApp>, n: usize, text: &str) {
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

fn choose(h: &mut Harness<'_, ArrixApp>, combo: usize, option: &str) {
    h.get_all_by_role(Role::ComboBox)
        .nth(combo)
        .unwrap()
        .click();
    h.run_steps(2);
    h.get_by_label(option).click();
    h.run_steps(3);
}

/// The scenario's joints in the saved document, by name.
fn stored(h: &Harness<'_, ArrixApp>) -> Vec<(String, serde_json::Value)> {
    let saved = h.state().saved().expect("a document is open").0;
    let (_, bytes) = saved
        .iter()
        .find(|(p, _)| p.starts_with("assemblies/"))
        .expect("an assembly is saved");
    let assembly: serde_json::Value = serde_json::from_slice(bytes).unwrap();
    assembly["joints"]
        .as_object()
        .unwrap()
        .values()
        .map(|j| (j["name"].as_str().unwrap().to_owned(), j.clone()))
        .collect()
}

fn hinge(h: &Harness<'_, ArrixApp>) -> serde_json::Value {
    stored(h)
        .into_iter()
        .find(|(n, _)| n == "hinge")
        .expect("the hinge")
        .1
}

/// Steps until the evaluation of generation `generation` has finished.
fn after(h: &mut Harness<'_, ArrixApp>, generation: u64) {
    common::wait(h, "the generation settled", |h| {
        h.state()
            .debug_state()
            .evaluation
            .is_some_and(|e| e.settled && e.generation >= generation)
    });
    h.run_steps(5);
}

fn double_click(h: &mut Harness<'_, ArrixApp>, label: &str) {
    // Clicks closer in time than this to the pair would make a triple.
    h.run_steps(35);
    let pos = h.get_by_label(label).rect().center();
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
}

/// A draft with no anchor picked cannot be added: the status line says
/// what is missing, the form stays, and the document is untouched.
#[test]
fn a_draft_without_its_anchors_is_not_added() {
    let mut h = app();
    run_command(&mut h, "Revolute joint");
    let f = form(&h).expect("a draft is open");
    assert!(f.create && f.title == "Joint");
    let names: Vec<_> = f.fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "kind",
            "parent",
            "parent_origin",
            "child",
            "child_origin",
            "value",
            "offset",
            "lower",
            "upper",
            "flip"
        ]
    );
    assert_eq!(text(&f, "value"), "0");
    h.key_press(Key::Enter);
    h.run_steps(5);
    assert!(form(&h).is_some(), "the draft stays");
    assert_eq!(stored(&h).len(), 1);
    assert!(
        h.query_by_label_contains("pick the parent anchor")
            .is_some()
    );

    // A fixed joint has no numbers.
    choose(&mut h, 0, "fixed");
    let f = form(&h).unwrap();
    assert!(!f.fields.iter().any(|f| f.name == "value"));
}

/// Each field of an edit form is one `EditJoint`; the same value, a refused
/// unit and a half-given pair of limits are none.
#[test]
fn a_joint_is_edited_field_by_field() {
    let mut h = app();
    double_click(&mut h, "hinge");
    let f = form(&h).expect("an edit form");
    assert!(!f.create && f.title == "Joint");
    assert_eq!(text(&f, "value"), "angle");

    type_into(&mut h, 1, "45 deg");
    assert_eq!(gesture(&h), Some(1));
    assert_eq!(hinge(&h)["value"], "45 deg");
    type_into(&mut h, 1, "45 deg");
    assert_eq!(gesture(&h), Some(0));
    type_into(&mut h, 1, "45 mm");
    assert_eq!(gesture(&h), Some(0));
    assert_eq!(hinge(&h)["value"], "45 deg");

    // One limit alone is not a pair: nothing is committed until both are.
    type_into(&mut h, 3, "-90 deg");
    assert_eq!(gesture(&h), Some(0));
    assert!(hinge(&h).get("limits").is_none_or(|l| l.is_null()));
    type_into(&mut h, 4, "90 deg");
    assert_eq!(gesture(&h), Some(1));
    assert_eq!(
        hinge(&h)["limits"],
        serde_json::json!(["-90 deg", "90 deg"])
    );

    choose(&mut h, 1, "Half a turn about x");
    assert_eq!(gesture(&h), Some(1));
    assert_eq!(hinge(&h)["flip"], true);

    // Undo takes the flip back, and the form follows the record.
    h.key_press_modifiers(Modifiers::CTRL, Key::Z);
    h.run_steps(30);
    assert!(hinge(&h).get("flip").is_none_or(|f| f == false));
    assert_eq!(hinge(&h)["value"], "45 deg");
    assert!(form(&h).is_some_and(|f| !f.fields.is_empty()));
}

/// The bracket's bore is deleted: the hinge's parent anchor is lost, the
/// form asks for it again and lists candidates, and taking one is one
/// command by the user, never a re-bind by the app.
#[test]
fn a_lost_anchor_offers_candidates_and_a_pick_replaces_it() {
    let mut h = app();
    let lost = hinge(&h)["parent"]["axis"].clone();
    h.state_mut()
        .submit(Command::DeleteFeature {
            feature: BORE.parse::<FeatureId>().expect("the bore's id"),
        })
        .expect("the bore is deleted");
    after(&mut h, 1);
    assert_eq!(hinge(&h)["parent"]["axis"], lost, "nothing re-bound it");

    double_click(&mut h, "hinge");
    let f = form(&h).expect("an edit form");
    assert!(f.diagnostic.as_ref().is_some_and(|d| d.code == "ref.lost"));
    let FieldValue::Reference {
        values,
        candidates,
        active,
        ..
    } = field(&f, "parent")
    else {
        panic!("parent is a reference")
    };
    assert!(values[0].lost, "{values:?}");
    assert!(*active, "a lost anchor asks to be picked again");
    assert!(!candidates.is_empty() && candidates.len() <= 5);
    let first = candidates[0].clone();

    h.get_by_label(&first).click();
    h.run_steps(5);
    assert_eq!(gesture(&h), Some(1));
    assert_ne!(hinge(&h)["parent"]["axis"], lost);
    after(&mut h, 2);
    let f = form(&h).unwrap();
    let FieldValue::Reference { values, .. } = field(&f, "parent") else {
        panic!("parent is a reference")
    };
    assert!(!values[0].lost, "{values:?}");
}

/// A feature's create form adds to the part the tab looks at, not to the
/// document's first part (found in plans/c2-m2 step 8).
#[test]
fn a_feature_form_adds_to_the_part_in_view() {
    let mut h = app();
    let features = |h: &Harness<'_, ArrixApp>, part: &str| -> usize {
        let saved = h.state().saved().expect("a document is open").0;
        let (_, bytes) = saved
            .iter()
            .find(|(p, _)| p.starts_with("parts/") && p.contains(part))
            .expect("the part is saved");
        let part: serde_json::Value = serde_json::from_slice(bytes).unwrap();
        part["features"].as_object().unwrap().len()
    };
    let (bracket, servo) = (features(&h, "BC6X598GYWPZN"), features(&h, "ESTFRCC4SPS9K"));
    h.state_mut().set_context("Servo").expect("by name");
    h.run_steps(3);
    run_command(&mut h, "New spur gear");
    h.key_press(Key::Enter);
    h.run_steps(5);
    assert_eq!(gesture(&h), Some(1));
    assert_eq!(features(&h, "ESTFRCC4SPS9K"), servo + 1);
    assert_eq!(features(&h, "BC6X598GYWPZN"), bracket);
}
