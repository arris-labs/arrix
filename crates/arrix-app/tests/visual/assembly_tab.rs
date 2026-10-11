//! The Assembly ribbon tab on a document with an assembly: the import and
//! the new assembly, both enabled (plans/c2-m2 step 7, ADR-0030 §6).

use egui_kittest::kittest::Queryable as _;

use crate::{harness, snapshot};

#[test]
fn the_assembly_tab_offers_the_import_and_a_new_assembly() {
    harness::with_app(
        "the_assembly_tab_offers_the_import_and_a_new_assembly",
        |h| {
            harness::open_settled(h, "tests/docs/assembly");
            h.get_by_label("Assembly").click();
            h.run_steps(3);
            let ribbon = h.state().debug_state().shell.expect("a frame ran").ribbon;
            assert_eq!(ribbon.active, "core.assembly");
            let buttons: Vec<_> = ribbon
                .groups
                .iter()
                .flat_map(|g| g.commands.iter().map(|c| (c.title.as_str(), c.enabled)))
                .collect();
            assert_eq!(buttons, [("Import STEP…", true), ("New assembly", true)]);
            snapshot::check(h, "assembly_tab");
        },
    );
}
