//! A throwaway capture, for looking at the app rather than pinning it
//! (docs/UI-RENDERING.md §Visual debugging). Not run by `cargo test`
//! (`test = false`); asked for by name:
//!
//! ```sh
//! cargo test -p arrix-app --test visual_scratch -- --nocapture
//! ```
//!
//! It writes `target/visual-scratch/scratch.{png,json}`. Edit the body below
//! to reach the state you want, run, then read the PNG, and the JSON when
//! the question is which number is wrong.
//!
//! **Revert your edits before committing.** This file is tracked; its
//! default body is the startup state.

#[path = "visual/harness.rs"]
#[allow(dead_code, reason = "the scratch target uses only part of the harness")]
mod harness;

#[test]
fn scratch() {
    harness::scratch(|harness| {
        // ---- edit below ------------------------------------------------
        let _ = harness;
        // ---- edit above ------------------------------------------------
    });
}
