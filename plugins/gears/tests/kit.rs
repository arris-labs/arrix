//! `gears` under the test kit (docs/PLUGINS.md §The test kit): the checks
//! `arrix test` runs, on this project's fixtures, compiled in as Tier 0 and
//! as the component `tests/guest_build.rs` builds.

use std::path::Path;
use std::sync::Arc;

use arrix_plugin_test::{Tier0, run_tier0};

#[test]
fn passes_the_kit_compiled_in() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let plugin = Tier0::Feature(Arc::new(arrix_gears::Gears));
    run_tier0(dir, arrix_gears::MANIFEST, plugin)
        .unwrap_or_else(|d| panic!("{}", d.join("\n")))
        .assert_passed();
}
