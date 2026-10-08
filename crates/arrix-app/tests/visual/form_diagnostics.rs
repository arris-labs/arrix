//! Diagnostics on the edit form (plans/forms step 10): a failed feature's
//! diagnostic heads its form and its entities are marked in the scene, a
//! lost reference shows lost in its field with its candidates marked and
//! is re-bound by a pick (one command, never by itself), and a frozen
//! feature opens read only with the reason. Logic through the debug state;
//! no golden.

use arrix_ui::{FeatureState, FieldValue, FormView};
use egui::accesskit::Role;
use egui_kittest::kittest::Queryable as _;

use crate::harness;

type Harness<'a> = egui_kittest::Harness<'a, arrix_app::ArrixApp>;

fn form(h: &Harness<'_>) -> Option<FormView> {
    h.state().debug_state().shell?.form
}

fn gestures(h: &Harness<'_>) -> Option<usize> {
    h.state().debug_state().gesture_commands
}

/// Steps until `until` holds of the form, or fails after a minute.
fn wait_form(h: &mut Harness<'_>, what: &str, until: impl Fn(&FormView) -> bool) -> FormView {
    let started = web_time::Instant::now();
    loop {
        h.step();
        if let Some(f) = form(h)
            && until(&f)
        {
            return f;
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(60),
            "{what}: {}",
            h.state().debug_state_json()
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
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

fn open_from_tree(h: &mut Harness<'_>, name: &str) {
    let at = h.get_by_label(name).rect().center();
    double_click(h, at);
    assert!(form(h).is_some_and(|f| !f.create), "{name}'s form opened");
}

fn type_into(h: &mut Harness<'_>, n: usize, text: &str) {
    h.get_all_by_role(Role::TextInput).nth(n).unwrap().focus();
    h.run_steps(2);
    h.key_press_modifiers(
        egui::Modifiers::CTRL | egui::Modifiers::COMMAND,
        egui::Key::A,
    );
    h.run_steps(2);
    h.get_all_by_role(Role::TextInput)
        .nth(n)
        .unwrap()
        .type_text(text);
    h.run_steps(2);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
}

/// A reference field's values as `(label, lost)`, and whether it is active.
fn reference(f: &FormView, name: &str) -> (Vec<(String, bool)>, bool) {
    match &f
        .fields
        .iter()
        .find(|x| x.name == name)
        .expect("a field")
        .value
    {
        FieldValue::Reference { values, active, .. } => (
            values.iter().map(|v| (v.label.clone(), v.lost)).collect(),
            *active,
        ),
        other => panic!("{name} is {other:?}"),
    }
}

#[test]
fn a_failed_features_diagnostic_heads_its_form_and_its_edge_is_marked() {
    harness::with_app("form_failed", |h| {
        harness::open_settled(h, "tests/docs/oversize-fillet");
        h.set_render_every_step(true);
        open_from_tree(h, "Fillet");
        let f = wait_form(h, "the failure", |f| f.state == Some(FeatureState::Failed));
        let d = f.diagnostic.clone().expect("the diagnostic");
        assert_eq!(d.code, "kernel.degenerate.blend-too-large");
        assert_eq!(f.read_only, None);
        assert_eq!(
            reference(&f, "edges").0,
            [("Edge of Plate".to_owned(), false)],
            "an edge the kernel refused is not a lost one"
        );
        assert_eq!(
            h.state().debug_state().marked.len(),
            1,
            "its edge is marked"
        );
        assert_eq!(gestures(h), Some(0), "opening issues none");

        // A radius that fits: one command, and the form heads fine.
        type_into(h, 1, "1 mm");
        assert_eq!(gestures(h), Some(1));
        let f = wait_form(h, "the fix", |f| f.state == Some(FeatureState::Ok));
        assert_eq!(f.diagnostic, None);
    });
}

/// `oversize-fillet` with its fillet's edge named for one the plate does
/// not have: lost, with the plate's other edges to re-pick.
fn lost_fillet() -> std::path::PathBuf {
    let from =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/docs/oversize-fillet");
    let to = std::env::temp_dir().join(format!("arrix-lost-fillet-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&to);
    std::fs::create_dir_all(to.join("parts")).expect("create the copy");
    for file in ["document.json", "params.json"] {
        std::fs::copy(from.join(file), to.join(file)).expect("copy a file");
    }
    for entry in std::fs::read_dir(from.join("parts")).expect("read the parts") {
        let entry = entry.expect("an entry");
        let text = std::fs::read_to_string(entry.path()).expect("read a part");
        let text = text.replace("end-edge.307H7HYN02QE5", "end-edge.0000000000001");
        std::fs::write(to.join("parts").join(entry.file_name()), text).expect("write a part");
    }
    to
}

#[test]
fn a_lost_reference_shows_lost_with_candidates_and_a_pick_rebinds_it() {
    let dir = lost_fillet();
    harness::with_app("form_lost", |h| {
        h.state_mut()
            .open_path_without(&dir, &[])
            .unwrap_or_else(|e| panic!("{e}"));
        harness::wait_settled(h);
        h.set_render_every_step(true);
        open_from_tree(h, "Fillet");
        let f = wait_form(h, "the loss", |f| {
            f.diagnostic.as_ref().is_some_and(|d| d.code == "ref.lost")
        });
        let (values, active) = reference(&f, "edges");
        assert_eq!(values.len(), 1);
        assert!(values[0].1, "the edge is lost: {values:?}");
        assert!(active, "its field asks to be re-picked");
        let message = f
            .fields
            .iter()
            .find(|x| x.name == "edges")
            .and_then(|x| x.message.clone())
            .expect("the field says why");
        assert!(message.starts_with("lost"), "{message}");
        let FieldValue::Reference { candidates, .. } =
            &f.fields.iter().find(|x| x.name == "edges").unwrap().value
        else {
            panic!("edges is a reference");
        };
        assert!(candidates.len() > 1, "{candidates:?}");
        assert_eq!(candidates[0], "1. Edge of Plate");
        let marked = h.state().debug_state().marked;
        assert!(!marked.is_empty(), "the candidates are marked");
        assert_eq!(gestures(h), Some(0), "nothing re-bound by itself");

        // A failed feature leaves the part no body to click, so the form
        // lists the candidates: taking one is one command, and it evaluates.
        h.get_by_label("1. Edge of Plate").click();
        h.run_steps(3);
        assert_eq!(gestures(h), Some(1), "the re-pick is one EditFeature");
        let f = wait_form(h, "the re-bind", |f| {
            f.diagnostic.as_ref().is_none_or(|d| d.code != "ref.lost")
        });
        let (values, _) = reference(&f, "edges");
        assert_eq!(values.len(), 1);
        assert!(!values[0].1, "the edge is bound again: {values:?}");
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_frozen_feature_opens_read_only_with_the_reason() {
    harness::with_app("form_frozen", |h| {
        harness::open_settled(h, "tests/docs/slice");
        let saved = h.state().saved().expect("a document is open");
        h.state_mut()
            .open_without("slice (without gears)", &saved, &["gears"])
            .expect("the saved slice opens");
        harness::wait_settled(h);
        open_from_tree(h, "Spur gear");
        let f = form(h).expect("the form");
        assert_eq!(f.state, Some(FeatureState::Frozen));
        let why = f.read_only.expect("the reason");
        assert!(
            why.contains("gears.spur") && why.contains("not available"),
            "{why}"
        );
        assert!(!f.fields.is_empty(), "the stored values show by name");
        // A read-only form takes no edit.
        type_into(h, 1, "99");
        assert_eq!(gestures(h), Some(0));
    });
}
