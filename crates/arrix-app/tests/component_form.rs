//! The component's form (plans/c2-m2 step 8, ADR-0030 §5): `Insert
//! component` opens a draft, OK is one `AddComponent`, a double-click on a
//! component row edits it field by field, one `EditComponent` each, and a
//! refusal by the document is shown in the form. Logic only; the draft's
//! look is `visual component_form`.

mod common;

use arrix_app::ArrixApp;
use arrix_core::{Id, PartId};
use arrix_doc::{Command, Component, ComponentOf};
use arrix_ui::{FieldValue, FormView};
use common::{gesture, run_command, settle};
use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::{Harness, kittest::Queryable as _};

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

fn text(form: &FormView, name: &str) -> String {
    let field = form.fields.iter().find(|f| f.name == name).unwrap();
    match &field.value {
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

/// The components of the scenario's assembly in the saved document, by
/// name, with their `pose` as stored.
fn stored(h: &Harness<'_, ArrixApp>) -> Vec<(String, serde_json::Value)> {
    let saved = h.state().saved().expect("a document is open").0;
    let (_, bytes) = saved
        .iter()
        .find(|(p, _)| p.starts_with("assemblies/"))
        .expect("an assembly is saved");
    let assembly: serde_json::Value = serde_json::from_slice(bytes).unwrap();
    assembly["components"]
        .as_object()
        .unwrap()
        .values()
        .map(|c| (c["name"].as_str().unwrap().to_owned(), c.clone()))
        .collect()
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

#[test]
fn a_component_is_made_and_edited_by_its_form() {
    let mut h = app();
    assert_eq!(stored(&h).len(), 2);

    // The command opens a draft and applies nothing.
    run_command(&mut h, "Insert component");
    let f = form(&h).expect("a draft is open");
    assert!(f.create && f.title == "Component");
    let names: Vec<_> = f.fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        ["of", "grounded", "x", "y", "z", "roll", "pitch", "yaw"]
    );
    assert_eq!(stored(&h).len(), 2);

    // Servo, 50 mm along X; OK is one command, and the tree selects it.
    choose(&mut h, 0, "Part: Servo");
    type_into(&mut h, 1, "50 mm");
    assert_eq!(text(&form(&h).unwrap(), "x"), "50 mm");
    h.key_press(Key::Enter);
    h.run_steps(5);
    assert_eq!(gesture(&h), Some(1));
    assert!(form(&h).is_none());
    let made = stored(&h);
    assert_eq!(made.len(), 3);
    let (_, servo2) = made
        .iter()
        .find(|(n, _)| n == "servo 2")
        .expect("named free");
    assert_eq!(servo2["pose"]["position"][0], "50 mm");

    // A double-click edits it: one command per field.
    double_click(&mut h, "servo 2");
    let f = form(&h).expect("an edit form");
    assert!(!f.create);
    assert_eq!(text(&f, "x"), "50 mm");
    type_into(&mut h, 3, "30 mm");
    assert_eq!(gesture(&h), Some(1));
    let (_, servo2) = stored(&h)
        .into_iter()
        .find(|(n, _)| n == "servo 2")
        .unwrap();
    assert_eq!(servo2["pose"]["position"][2], "30 mm");
    // The same value again, and a refused unit, are none.
    type_into(&mut h, 3, "30 mm");
    assert_eq!(gesture(&h), Some(0));
    type_into(&mut h, 3, "30 deg");
    assert_eq!(gesture(&h), Some(0));
    assert!(
        form(&h)
            .unwrap()
            .fields
            .iter()
            .find(|f| f.name == "z")
            .unwrap()
            .message
            .is_some()
    );
    // Placing it by a joint drops the pose, and the pose fields go.
    choose(&mut h, 1, "By a joint");
    assert_eq!(gesture(&h), Some(1));
    let (_, servo2) = stored(&h)
        .into_iter()
        .find(|(n, _)| n == "servo 2")
        .unwrap();
    assert!(servo2.get("pose").is_none_or(|p| p.is_null()), "{servo2}");
    assert!(!form(&h).unwrap().fields.iter().any(|f| f.name == "x"));
    // Undo takes that back, and the form follows the record.
    h.key_press_modifiers(Modifiers::CTRL, Key::Z);
    h.run_steps(30);
    assert!(form(&h).unwrap().fields.iter().any(|f| f.name == "x"));
}

/// A component of a part the document does not hold is refused as
/// `assembly.dangling`, and the open form says so.
#[test]
fn a_refusal_is_shown_in_the_form() {
    let mut h = app();
    run_command(&mut h, "Insert component");
    let context = h.state().debug_state().context.expect("an assembly");
    let assembly = context.id.parse().unwrap();
    let ghost = Component {
        id: arrix_core::ComponentId(Id(77)),
        name: "ghost".into(),
        of: ComponentOf::Part(PartId(Id(78))),
        pose: None,
    };
    let refused = h.state_mut().submit(Command::AddComponent {
        assembly,
        component: Box::new(ghost),
    });
    assert!(refused.is_err());
    h.run_steps(3);
    let d = form(&h).unwrap().diagnostic.expect("the refusal");
    assert_eq!(d.code, "assembly.dangling");
    assert_eq!(stored(&h).len(), 2);
}
