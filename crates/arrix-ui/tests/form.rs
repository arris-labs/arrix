//! The form renderer, headless (plans/forms step 4): a form of every field
//! kind draws, and what is done with it comes back as intents. The renderer
//! names no document type, so the views here are built by hand.

use arrix_ui::{
    ChoiceOptionView, FeatureState, FieldValue, FormDiagnostic, FormField, FormView,
    ReferenceValueView, UiIntent,
};
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};

fn option(name: &str, title: &str) -> ChoiceOptionView {
    ChoiceOptionView {
        name: name.into(),
        title: title.into(),
    }
}

fn field(name: &str, title: &str, value: FieldValue) -> FormField {
    FormField {
        name: name.into(),
        title: title.into(),
        value,
        message: None,
    }
}

/// An extrude's form with one field of each kind.
fn view() -> FormView {
    FormView {
        id: "draft-1".into(),
        title: "Extrude".into(),
        create: true,
        name: "Extrude 1".into(),
        state: None,
        diagnostic: None,
        read_only: None,
        fields: vec![
            field(
                "region",
                "Sketch region",
                FieldValue::Reference {
                    takes: "sketch region".into(),
                    values: vec![],
                    list: false,
                    active: true,
                },
            ),
            field(
                "distance",
                "Distance",
                FieldValue::Quantity {
                    text: "10 mm".into(),
                    unit: "mm".into(),
                },
            ),
            field(
                "mode",
                "Mode",
                FieldValue::Choice {
                    selected: Some("new".into()),
                    options: vec![
                        option("new", "New body"),
                        option("join", "Join"),
                        option("cut", "Cut"),
                    ],
                    clearable: false,
                },
            ),
            field(
                "edges",
                "Edges",
                FieldValue::Reference {
                    takes: "edge".into(),
                    values: vec![
                        ReferenceValueView {
                            label: "Edge 3".into(),
                            lost: false,
                        },
                        ReferenceValueView {
                            label: "Edge 7".into(),
                            lost: true,
                        },
                    ],
                    list: true,
                    active: false,
                },
            ),
        ],
    }
}

fn harness(view: FormView) -> Harness<'static, (FormView, Vec<UiIntent>)> {
    Harness::new_ui_state(
        |ui, (view, intents): &mut (FormView, Vec<UiIntent>)| {
            intents.extend(arrix_ui::form(ui, view));
        },
        (view, Vec::new()),
    )
}

fn take(h: &mut Harness<'static, (FormView, Vec<UiIntent>)>) -> Vec<UiIntent> {
    h.run();
    std::mem::take(&mut h.state_mut().1)
}

/// Focuses the distance box (the second text box: the name is the first)
/// and types `text` into it.
fn type_into_distance(h: &mut Harness<'static, (FormView, Vec<UiIntent>)>, text: &str) {
    h.get_all_by_role(Role::TextInput).nth(1).unwrap().focus();
    h.run();
    h.get_all_by_role(Role::TextInput)
        .nth(1)
        .unwrap()
        .type_text(text);
    h.run();
}

#[test]
fn a_form_of_every_field_kind_draws() {
    let mut h = harness(view());
    h.run();
    for label in [
        "Extrude",
        "Name",
        "Sketch region",
        "pick a sketch region",
        "Distance",
        "mm",
        "Mode",
        "Edges",
        "Edge 3",
        "lost: Edge 7",
        "OK",
        "Cancel",
    ] {
        let _ = h.get_by_label(label);
    }
    assert_eq!(
        h.get_all_by_role(Role::TextInput).count(),
        2,
        "name, distance"
    );
    assert!(take(&mut h).is_empty(), "drawing says nothing");
}

#[test]
fn an_edit_form_has_no_ok_or_cancel_and_shows_its_state() {
    let mut v = view();
    v.create = false;
    v.state = Some(FeatureState::Failed);
    v.diagnostic = Some(FormDiagnostic {
        code: "kernel.degenerate.blend-too-large".into(),
        message: "the radius is too large".into(),
    });
    let mut h = harness(v);
    h.run();
    assert!(h.query_by_label("OK").is_none());
    assert!(h.query_by_label("Cancel").is_none());
    let _ = h.get_by_label("failed");
    let _ = h.get_by_label("kernel.degenerate.blend-too-large");
    let _ = h.get_by_label("the radius is too large");
}

#[test]
fn ok_and_cancel_are_intents() {
    let mut h = harness(view());
    h.run();
    h.get_by_label("OK").click();
    assert_eq!(take(&mut h), [UiIntent::FormOk]);
    h.get_by_label("Cancel").click();
    assert_eq!(take(&mut h), [UiIntent::FormCancelled]);
}

#[test]
fn a_typed_quantity_is_edited_then_committed_once() {
    let mut h = harness(view());
    h.run();
    type_into_distance(&mut h, "12 mm");
    let edited = take(&mut h);
    assert!(
        edited
            .iter()
            .all(|i| matches!(i, UiIntent::FieldEdited { field, .. } if field == "distance")),
        "{edited:?}"
    );
    assert!(!edited.is_empty());
    h.key_press(egui::Key::Enter);
    h.run();
    let committed = take(&mut h);
    let [UiIntent::FieldCommitted { field, text }] = &committed[..] else {
        panic!("{committed:?}")
    };
    assert_eq!(field, "distance");
    assert!(text.ends_with("12 mm"), "{text}");
    h.run();
    assert!(take(&mut h).is_empty(), "committed once");
}

#[test]
fn a_refused_text_stays_in_its_field_with_the_reason() {
    let mut v = view();
    v.fields[1].message = Some("expr.unit-mismatch: a length is not an angle".into());
    let mut h = harness(v);
    h.run();
    type_into_distance(&mut h, "12 deg");
    h.key_press(egui::Key::Enter);
    h.run();
    // The view still says `10 mm` (nothing was taken): the box keeps what
    // was typed, and the reason is beneath it.
    let text = h
        .get_all_by_role(Role::TextInput)
        .nth(1)
        .unwrap()
        .value()
        .unwrap_or_default();
    assert!(text.contains("12 deg"), "{text}");
    let _ = h.get_by_label("expr.unit-mismatch: a length is not an angle");
}

#[test]
fn the_box_follows_the_view_when_the_document_changes_under_it() {
    let mut h = harness(view());
    h.run();
    h.state_mut().0.fields[1].value = FieldValue::Quantity {
        text: "20 mm".into(),
        unit: "mm".into(),
    };
    h.run();
    h.run();
    let text = h
        .get_all_by_role(Role::TextInput)
        .nth(1)
        .unwrap()
        .value()
        .unwrap_or_default();
    assert_eq!(text, "20 mm");
}

#[test]
fn a_choice_and_the_reference_fields_say_what_was_done() {
    let mut h = harness(view());
    h.run();
    h.get_by_role(Role::ComboBox).click();
    h.run();
    h.get_by_label("Join").click();
    assert_eq!(
        take(&mut h),
        [UiIntent::ChoiceSelected {
            field: "mode".into(),
            value: Some("join".into())
        }]
    );
    h.get_by_label("Edges").click();
    assert_eq!(
        take(&mut h),
        [UiIntent::ReferenceFieldActivated {
            field: "edges".into()
        }]
    );
    h.get_all_by_label("×").next().unwrap().click();
    assert_eq!(
        take(&mut h),
        [UiIntent::ReferenceRemoved {
            field: "edges".into(),
            index: 0
        }]
    );
}

#[test]
fn a_read_only_form_takes_no_input() {
    let mut v = view();
    v.create = false;
    v.read_only = Some("frozen: gears is not installed".into());
    let mut h = harness(v);
    h.run();
    let _ = h.get_by_label("frozen: gears is not installed");
    for input in h.get_all_by_role(Role::TextInput) {
        assert!(input.accesskit_node().is_disabled());
    }
}
