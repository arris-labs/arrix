//! The component's create form in the property card, beside the assembly
//! it adds to (plans/c2-m2 step 8, ADR-0030 §5); its behaviour is
//! `tests/component_form.rs`.

use egui_kittest::kittest::Queryable as _;

use crate::{harness, snapshot};

#[test]
fn the_component_form_is_a_draft_until_ok() {
    harness::with_app("the_component_form_is_a_draft_until_ok", |h| {
        harness::open_settled(h, "tests/docs/assembly");
        h.get_by_label("Assembly").click();
        h.run_steps(3);
        h.get_by_label("Insert component").click();
        h.run_steps(4);
        let form = h.state().debug_state().shell.expect("a frame ran").form;
        let form = form.expect("a draft is open");
        assert!(form.create);
        assert_eq!(form.title, "Component");
        snapshot::check(h, "component_form");
    });
}
