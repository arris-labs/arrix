//! Trim and extend (plans/sketch-mode step 12, docs/UI-RENDERING.md
//! §Sketch mode): a hover previews the stretch the click removes or adds,
//! and the click is one command that does exactly that.

mod common;

use arrix_ui::CurveShape;
use common::*;
use egui::{Key, Modifiers};

/// A level line `(10,20)-(50,20)` crossed by a plumb one `(30,5)-(30,40)`,
/// and a short level line `(10,30)-(20,30)` that stops short of the plumb
/// one; the select tool back.
fn crossed() -> egui_kittest::Harness<'static, arrix_app::ArrixApp> {
    let mut h = sketching();
    h.key_press(Key::L);
    h.run_steps(2);
    for (a, b, n) in [
        ([10.0, 20.0], [50.0, 20.0], 1),
        ([30.0, 5.0], [30.0, 40.0], 2),
        ([10.0, 30.0], [20.0, 30.0], 3),
    ] {
        click(&mut h, a);
        click(&mut h, b);
        drawn(&mut h, n);
        h.key_press(Key::Escape);
        h.run_steps(2);
    }
    h
}

/// The ends of the previewed stretch, in millimetres, sorted.
fn previewed(h: &egui_kittest::Harness<'_, arrix_app::ArrixApp>) -> Vec<[f64; 2]> {
    let mut ends: Vec<[f64; 2]> = preview(h)
        .curves
        .iter()
        .flat_map(|c| match c {
            CurveShape::Line { from, to } => [*from, *to],
            _ => panic!("a line was expected"),
        })
        .map(|p| p.map(|x| (x / MM * 1e3).round() / 1e3))
        .collect();
    ends.sort_by(|a, b| a.partial_cmp(b).unwrap());
    ends.dedup();
    ends
}

#[test]
fn a_trim_removes_what_the_hover_showed_in_one_command() {
    let mut h = crossed();
    let before = h.state().saved().unwrap().0;
    h.key_press(Key::T);
    h.run_steps(2);
    hover(&mut h, [40.0, 20.0]);
    assert_eq!(previewed(&h), [[30.0, 20.0], [50.0, 20.0]]);
    assert!(point_at(&h, [50.0, 20.0]).is_some());

    click(&mut h, [40.0, 20.0]);
    assert_eq!(gesture(&h), Some(1));
    wait(&mut h, "the trim was drawn", |h| {
        point_at(h, [50.0, 20.0]).is_none()
    });
    assert!(
        point_at(&h, [10.0, 20.0]).is_some(),
        "the far end is untouched"
    );
    assert!(
        point_at(&h, [30.0, 20.0]).is_some(),
        "the line ends at the crossing"
    );

    h.key_press_modifiers(Modifiers::CTRL, Key::Z);
    h.run_steps(3);
    assert_eq!(
        h.state().saved().unwrap().0,
        before,
        "undo is byte for byte"
    );
}

#[test]
fn an_extend_adds_what_the_hover_showed_in_one_command() {
    let mut h = crossed();
    h.key_press(Key::E);
    h.run_steps(2);
    hover(&mut h, [19.0, 30.0]);
    assert_eq!(previewed(&h), [[20.0, 30.0], [30.0, 30.0]]);

    click(&mut h, [19.0, 30.0]);
    assert_eq!(gesture(&h), Some(1));
    wait(&mut h, "the extension was drawn", |h| {
        point_at(h, [30.0, 30.0]).is_some()
    });
    assert!(point_at(&h, [20.0, 30.0]).is_none(), "the end moved");
    assert!(point_at(&h, [10.0, 30.0]).is_some());
}

#[test]
fn a_click_that_changes_nothing_issues_nothing() {
    let mut h = crossed();
    h.key_press(Key::T);
    h.run_steps(2);
    click(&mut h, [60.0, 50.0]);
    assert_eq!(gesture(&h), Some(0), "empty space");

    // The plumb line's end has no crossing along its continuation but the
    // level lines': extending it upward finds none.
    h.key_press(Key::E);
    h.run_steps(2);
    hover(&mut h, [30.0, 38.0]);
    assert!(
        h.state()
            .debug_state()
            .sketch_view
            .is_some_and(|v| v.preview.is_none()),
        "no stretch to show"
    );
    click(&mut h, [30.0, 38.0]);
    assert_eq!(gesture(&h), Some(0));
    assert_eq!(hint(&h), "there is no crossing to reach.");
}
