//! Dimensions as expressions (plans/sketch-mode step 11,
//! docs/UI-RENDERING.md §Sketch mode): the dimension tool, the field a
//! double-click on a label opens, and what each does to the document.

mod common;

use arrix_app::ArrixApp;
use arrix_core::{Id, ParamId, QuantityKind};
use arrix_doc::expr::Expr;
use arrix_doc::{Command, Param};
use common::*;
use egui::{Key, Modifiers};

fn param(name: &str, text: &str) -> Command {
    Command::AddParam {
        param: ParamId(Id(7)),
        record: Param {
            name: name.into(),
            kind: QuantityKind::Length,
            expr: Expr::parse(text).unwrap(),
        },
    }
}

/// The rectangle (20,15)-(50,35), its bottom edge dimensioned with the
/// label placed below it, and the select tool back.
fn dimensioned() -> egui_kittest::Harness<'static, ArrixApp> {
    let mut h = with_rectangle();
    h.key_press(Key::D);
    h.run_steps(2);
    assert_eq!(tool(&h).as_deref(), Some("core.sketch.dimension"));
    click(&mut h, [35.0, 15.0]);
    assert_eq!(gesture(&h), Some(0), "a pick is no command");
    click(&mut h, [35.0, 5.0]);
    assert_eq!(gesture(&h), Some(1), "the placing click is one command");
    h.key_press(Key::Escape);
    h.run_steps(3);
    h
}

#[test]
fn a_line_picked_and_a_click_place_a_distance() {
    let h = dimensioned();
    assert!(stored(&h).constraints.contains(&"distance".to_owned()));
    let dims = dimensions(&h);
    assert_eq!(dims.len(), 1);
    let (kind, text, reference, at) = &dims[0];
    assert_eq!(
        (kind.as_str(), text.as_str(), *reference),
        ("distance", "30 mm", false)
    );
    assert!(
        (at[0] - 35.0).abs() < 0.01 && (at[1] - 5.0).abs() < 0.01,
        "{at:?}"
    );
}

/// What the sketch already holds is placed as a reference dimension: shown
/// and measured, not driving.
#[test]
fn a_dimension_the_sketch_holds_already_is_a_reference() {
    let mut h = dimensioned();
    h.key_press(Key::D);
    h.run_steps(2);
    click(&mut h, [35.0, 15.0]);
    click(&mut h, [35.0, -5.0]);
    assert_eq!(gesture(&h), Some(1));
    let dims = dimensions(&h);
    assert_eq!(dims.len(), 2);
    assert!(dims.iter().any(|d| d.2), "one is a reference: {dims:?}");
    assert!(
        hint(&h).contains("reference dimension"),
        "the status line says why: {}",
        hint(&h)
    );
}

#[test]
fn a_dimension_of_two_points_follows_where_its_label_stands() {
    let mut h = with_rectangle();
    h.key_press(Key::D);
    h.run_steps(2);
    click(&mut h, [20.0, 15.0]);
    click(&mut h, [50.0, 35.0]);
    // Beside them: the vertical extent.
    click(&mut h, [60.0, 25.0]);
    assert_eq!(gesture(&h), Some(1));
    assert_eq!(dimensions(&h)[0].0, "vertical_distance");
    assert_eq!(dimensions(&h)[0].1, "20 mm");
}

#[test]
fn a_circle_gets_a_radius_and_with_shift_a_diameter() {
    let mut h = sketching();
    h.key_press(Key::C);
    h.run_steps(2);
    click(&mut h, [30.0, 30.0]);
    click(&mut h, [50.0, 30.0]);
    drawn(&mut h, 1);
    h.key_press(Key::Escape);
    h.run_steps(2);
    h.key_press(Key::D);
    h.run_steps(2);
    click(&mut h, [50.0, 30.0]);
    click(&mut h, [70.0, 50.0]);
    assert_eq!(gesture(&h), Some(1));
    assert_eq!(dimensions(&h)[0].0, "radius");
    assert_eq!(dimensions(&h)[0].1, "20 mm");
}

/// Type `width` into the label's field: the dimension takes the
/// parameter's value, the sketch solves to it, one command.
#[test]
fn a_label_takes_a_parameter_and_follows_it() {
    let mut h = dimensioned();
    h.state_mut().submit(param("width", "60 mm")).unwrap();
    wait_finished(&mut h);
    let after_param = h.state().saved().unwrap().0;

    double_click(&mut h, [35.0, 5.0]);
    assert!(
        field_open(&h),
        "a double-click on the label opens its field"
    );
    assert_eq!(gesture(&h), Some(0));
    type_and_enter(&mut h, "width");
    assert_eq!(gesture(&h), Some(1), "{:?}", field_error(&h));
    assert!(!field_open(&h));
    drawn(&mut h, 4);
    assert_eq!(dimensions(&h)[0].1, "width = 60 mm");
    assert!((bottom_edge_mm(&h, 15.0).unwrap() - 60.0).abs() < 1e-3);

    // Undo takes the label back, and redo returns it.
    let labelled = h.state().saved().unwrap().0;
    h.key_press_modifiers(Modifiers::CTRL, Key::Z);
    h.run_steps(3);
    assert!(
        h.state().saved().unwrap().0 == after_param,
        "undo returns the document byte for byte"
    );
    h.key_press_modifiers(Modifiers::CTRL, Key::Y);
    h.run_steps(3);
    assert!(h.state().saved().unwrap().0 == labelled, "and redo too");
    drawn(&mut h, 4);

    // The parameter changes in the document: the sketch is evaluated again
    // and sketch mode issues nothing.
    let applied = h.state_mut().submit(set_param("80 mm")).is_ok();
    assert!(applied);
    wait(&mut h, "the sketch follows the parameter", |h| {
        dimensions(h)
            .first()
            .is_some_and(|d| d.1 == "width = 80 mm")
    });
    assert!((bottom_edge_mm(&h, 15.0).unwrap() - 80.0).abs() < 1e-3);
}

fn set_param(text: &str) -> Command {
    Command::SetParam {
        param: ParamId(Id(7)),
        expr: Expr::parse(text).unwrap(),
    }
}

fn wait_finished(h: &mut egui_kittest::Harness<'_, ArrixApp>) {
    wait(h, "the parameter has a value", |h| {
        h.state()
            .debug_state()
            .evaluation
            .is_some_and(|e| e.finished)
    });
    h.run_steps(3);
}

#[test]
fn a_text_that_does_not_make_a_value_leaves_the_field_open() {
    let mut h = dimensioned();
    let before = h.state().saved().unwrap().0;
    double_click(&mut h, [35.0, 5.0]);
    assert!(field_open(&h));
    // Not an expression, an unknown name, a number without its unit.
    for (text, error) in [
        ("3 +", "at byte"),
        ("nothing", "no parameter is named `nothing`"),
        ("40", "where a length is wanted"),
    ] {
        type_and_enter(&mut h, text);
        assert_eq!(gesture(&h), Some(0), "{text}");
        let shown = field_error(&h).unwrap_or_default();
        assert!(shown.contains(error), "{text}: {shown}");
        assert!(field_open(&h));
    }
    assert!(h.state().saved().unwrap().0 == before, "nothing was stored");
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(!field_open(&h), "Escape dismisses it");
    assert!(
        h.state().debug_state().sketch.is_some(),
        "and stays in the sketch"
    );
}

#[test]
fn the_value_it_has_issues_nothing() {
    let mut h = dimensioned();
    double_click(&mut h, [35.0, 5.0]);
    type_and_enter(&mut h, "45 mm");
    assert_eq!(gesture(&h), Some(1));
    let before = h.state().saved().unwrap().0;
    double_click(&mut h, [35.0, 5.0]);
    assert!(field_open(&h));
    type_and_enter(&mut h, "45 mm");
    assert_eq!(gesture(&h), Some(0));
    assert!(!field_open(&h));
    assert!(h.state().saved().unwrap().0 == before, "nothing was stored");
}

/// A plain value types a new length: the dimension drives to it, no
/// expression is kept.
#[test]
fn a_plain_value_drives_the_dimension() {
    let mut h = dimensioned();
    double_click(&mut h, [35.0, 5.0]);
    type_and_enter(&mut h, "45 mm");
    assert_eq!(gesture(&h), Some(1), "{:?}", field_error(&h));
    drawn(&mut h, 4);
    assert_eq!(dimensions(&h)[0].1, "45 mm");
    assert!((bottom_edge_mm(&h, 15.0).unwrap() - 45.0).abs() < 1e-3);
}

#[test]
fn escape_dismisses_the_field_and_keeps_the_sketch() {
    let mut h = dimensioned();
    double_click(&mut h, [35.0, 5.0]);
    assert!(field_open(&h));
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(!field_open(&h), "Escape dismisses it");
    assert!(
        h.state().debug_state().sketch.is_some(),
        "and stays in the sketch"
    );
}
