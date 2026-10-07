//! The create form (plans/forms step 5, ADR-0018): `<type>.create` opens a
//! draft in the property card, a field's text is checked as the form can
//! check it, `shown_when` hides what a choice leaves out, OK is one
//! `AddFeature` and Cancel and Escape issue none. Logic only; the
//! acceptance scenario is `visual forms_accept`.

mod common;

use arrix_app::ArrixApp;
use arrix_core::{Id, ParamId, QuantityKind};
use arrix_doc::expr::Expr;
use arrix_doc::{Command, Param};
use arrix_ui::{FieldValue, FormView};
use common::{gesture, settle, wait};
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
    h.key_press_modifiers(Modifiers::CTRL, Key::N);
    settle(&mut h);
    h
}

fn form(h: &Harness<'_, ArrixApp>) -> Option<FormView> {
    h.state().debug_state().shell?.form
}

fn field<'a>(form: &'a FormView, name: &str) -> &'a arrix_ui::FormField {
    form.fields
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no field {name} in {:?}", form.fields))
}

fn text(form: &FormView, name: &str) -> String {
    match &field(form, name).value {
        FieldValue::Quantity { text, .. } => text.clone(),
        other => panic!("{name} is {other:?}"),
    }
}

fn names(form: &FormView) -> Vec<&str> {
    form.fields.iter().map(|f| f.name.as_str()).collect()
}

/// The features the saved document holds, as `(type, record)`.
fn features(h: &Harness<'_, ArrixApp>) -> Vec<(String, serde_json::Value)> {
    let saved = h.state().saved().expect("a document is open").0;
    let mut out = Vec::new();
    for (path, bytes) in saved.iter().filter(|(p, _)| p.starts_with("parts/")) {
        let part: serde_json::Value = serde_json::from_slice(bytes).expect("a part file is JSON");
        let _ = path;
        for record in part["features"].as_object().into_iter().flatten() {
            out.push((
                record.1["type_id"].as_str().unwrap_or_default().to_owned(),
                record.1.clone(),
            ));
        }
    }
    out
}

/// A ribbon tab's button: the tab's title is also its first group's.
fn tab(h: &mut Harness<'_, ArrixApp>, title: &str) {
    h.get_all_by_label(title).next().unwrap().click();
    h.run_steps(2);
}

/// Opens the form of `command` (a ribbon button) in its tab.
fn open(h: &mut Harness<'_, ArrixApp>, tab_title: &str, button: &str) {
    tab(h, tab_title);
    h.get_by_label(button).click();
    h.run_steps(3);
}

/// Types `text` into the `n`th text box and commits it with Enter.
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

/// Every `<type>.create` command of the ribbon but the sketch's: the tab
/// it is in, its button, and the type.
fn creations(h: &mut Harness<'_, ArrixApp>) -> Vec<(String, String, String)> {
    let tabs: Vec<_> = h
        .state()
        .debug_state()
        .shell
        .unwrap()
        .ribbon
        .tabs
        .iter()
        .map(|t| t.title.clone())
        .collect();
    let mut out = Vec::new();
    for tab in tabs {
        self::tab(h, &tab);
        let ribbon = h.state().debug_state().shell.unwrap().ribbon;
        for c in ribbon.groups.iter().flat_map(|g| &g.commands) {
            if let Some(ty) = c.id.strip_suffix(".create")
                && ty != "core.sketch"
            {
                out.push((tab.clone(), c.title.clone(), ty.to_owned()));
            }
        }
    }
    out
}

#[test]
fn every_type_with_a_create_command_is_created_by_one_command() {
    let mut h = app();
    let all = creations(&mut h);
    let ids: Vec<_> = all.iter().map(|(_, _, ty)| ty.as_str()).collect();
    assert_eq!(
        ids,
        [
            "core.datum-plane",
            "core.boolean",
            "core.chamfer",
            "core.extrude",
            "core.fillet",
            "core.hole",
            "core.mirror",
            "core.pattern-circular",
            "core.pattern-linear",
            "core.revolve",
            "gears.spur",
        ]
        .map(|id| id)
    );
    for (n, (tab, button, ty)) in all.iter().enumerate() {
        open(&mut h, tab, button);
        assert!(form(&h).is_some(), "{ty}: the form opened");
        assert_eq!(gesture(&h), Some(0), "{ty}: opening issues none");
        assert!(h.state().debug_state().shell.unwrap().cards.properties);
        h.get_by_label("OK").click();
        h.run_steps(3);
        assert_eq!(gesture(&h), Some(1), "{ty}: OK is one command");
        assert!(form(&h).is_none(), "{ty}: the form closed");
        let made = features(&h);
        assert_eq!(made.len(), n + 1, "{ty}");
        assert!(made.iter().any(|(t, _)| t == ty), "{ty}: {made:?}");
    }
}

#[test]
fn cancel_and_escape_issue_none() {
    let mut h = app();
    let before = h.state().saved().unwrap().0;
    open(&mut h, "Part", "New extrude");
    assert!(form(&h).is_some());
    h.get_by_label("Cancel").click();
    h.run_steps(3);
    assert!(form(&h).is_none());
    assert_eq!(gesture(&h), Some(0));
    open(&mut h, "Part", "New extrude");
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(form(&h).is_none(), "Escape cancels");
    assert_eq!(gesture(&h), Some(0));
    assert_eq!(h.state().saved().unwrap().0, before, "nothing was written");
}

#[test]
fn enter_is_ok_when_no_field_has_focus() {
    let mut h = app();
    open(&mut h, "Part", "New extrude");
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert!(form(&h).is_none());
    assert_eq!(gesture(&h), Some(1));
    assert_eq!(features(&h).len(), 1);
}

#[test]
fn a_form_is_seeded_from_the_spec_and_a_field_is_checked_by_its_kind() {
    let mut h = app();
    open(&mut h, "Part", "New extrude");
    let f = form(&h).unwrap();
    assert_eq!(
        (f.title.as_str(), f.name.as_str()),
        ("Extrude", "Extrude 1")
    );
    assert!(f.create);
    assert_eq!(text(&f, "distance"), "10 mm");
    assert_eq!(
        names(&f),
        ["mode", "region", "distance"],
        "no target in new"
    );

    type_into(&mut h, 1, "12 deg");
    let f = form(&h).unwrap();
    assert_eq!(text(&f, "distance"), "12 deg", "the text is kept");
    let why = field(&f, "distance").message.clone().expect("a reason");
    assert!(why.starts_with("expr.unit-mismatch"), "{why}");
    assert_eq!(gesture(&h), Some(0));

    // OK waits while a field is refused.
    h.get_by_label("OK").click();
    h.run_steps(3);
    assert!(form(&h).is_some());
    assert_eq!(gesture(&h), Some(0));
    assert!(features(&h).is_empty());

    type_into(&mut h, 1, "12 mm");
    let f = form(&h).unwrap();
    assert_eq!(field(&f, "distance").message, None);
    h.get_by_label("OK").click();
    h.run_steps(3);
    let made = features(&h);
    assert_eq!(made.len(), 1);
    assert_eq!(made[0].1["params"]["distance"], "12 mm");
}

#[test]
fn a_parameter_of_the_document_is_a_value() {
    let mut h = app();
    h.state_mut()
        .submit(Command::AddParam {
            param: ParamId(Id(7)),
            record: Param {
                name: "w".into(),
                kind: QuantityKind::Length,
                expr: Expr::parse("30 mm").unwrap(),
            },
        })
        .unwrap();
    wait(&mut h, "the parameter was heard", |h| {
        h.state()
            .debug_state()
            .evaluation
            .is_some_and(|e| e.generation == 1 && e.finished)
    });
    open(&mut h, "Part", "New extrude");
    type_into(&mut h, 1, "w");
    let f = form(&h).unwrap();
    assert_eq!(text(&f, "distance"), "w");
    assert_eq!(field(&f, "distance").message, None);
}

#[test]
fn a_choice_shows_the_fields_it_leaves_in() {
    let mut h = app();
    open(&mut h, "Part", "New hole");
    let f = form(&h).unwrap();
    assert_eq!(
        names(&f),
        ["extent", "kind", "target", "points", "diameter", "depth"]
    );
    // The kind combo is the second.
    let pick = |h: &mut Harness<'_, ArrixApp>, combo: usize, option: &str| {
        h.get_all_by_role(Role::ComboBox)
            .nth(combo)
            .unwrap()
            .click();
        h.run_steps(2);
        h.get_by_label(option).click();
        h.run_steps(3);
    };
    pick(&mut h, 1, "Counterbore");
    let f = form(&h).unwrap();
    assert_eq!(
        names(&f),
        [
            "extent",
            "kind",
            "target",
            "points",
            "diameter",
            "depth",
            "cb-diameter",
            "cb-depth"
        ]
    );
    pick(&mut h, 0, "Through all");
    let f = form(&h).unwrap();
    assert!(!names(&f).contains(&"depth"), "{:?}", names(&f));
    assert_eq!(gesture(&h), Some(0));
    h.get_by_label("OK").click();
    h.run_steps(3);
    let made = features(&h);
    assert_eq!(made[0].1["choices"]["kind"], "counterbore");
    assert_eq!(made[0].1["choices"]["extent"], "through");
}

#[test]
fn a_choice_without_a_default_may_be_cleared() {
    let mut h = app();
    open(&mut h, "Sketch", "New datum plane");
    let f = form(&h).unwrap();
    let FieldValue::Choice {
        selected,
        clearable,
        ..
    } = &field(&f, "world").value
    else {
        panic!("world is a choice")
    };
    assert_eq!((selected.as_deref(), *clearable), (None, true));
}

#[test]
fn the_name_is_the_forms_and_a_second_feature_gets_a_free_one() {
    let mut h = app();
    open(&mut h, "Part", "New extrude");
    h.get_all_by_role(Role::TextInput).next().unwrap().focus();
    h.run_steps(2);
    h.key_press_modifiers(Modifiers::CTRL | Modifiers::COMMAND, Key::A);
    h.run_steps(2);
    h.get_all_by_role(Role::TextInput)
        .next()
        .unwrap()
        .type_text("Plate");
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(form(&h).unwrap().name, "Plate");
    h.get_by_label("OK").click();
    h.run_steps(3);
    assert_eq!(features(&h)[0].1["name"], "Plate");
    open(&mut h, "Part", "New extrude");
    assert_eq!(form(&h).unwrap().name, "Extrude 1");
}
