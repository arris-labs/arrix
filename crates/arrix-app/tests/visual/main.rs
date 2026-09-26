//! The visual scenarios (docs/UI-RENDERING.md §Visual debugging).

mod coarse;
mod harness;
mod snapshot;

/// The shell at startup: no document content, the Sketch tab open.
#[test]
fn empty_shell() {
    harness::with_app("empty_shell", |h| snapshot::check(h, "empty_shell"));
}

/// Frames are a function of the app's state alone. Established on lavapipe
/// in plans/c1-m0 step 4: identical pixels from two app instances, from a
/// re-render after further frames, across processes, and with 1, 4 or all
/// of lavapipe's threads. So the coarse frame's tolerance only has to absorb
/// a different Mesa version, not run-to-run noise.
#[test]
fn empty_shell_renders_identically() {
    let mut first = None;
    harness::with_app("empty_shell_renders_identically", |h| {
        let a = harness::render(h);
        h.run_steps(10);
        let again = harness::render(h);
        assert!(a == again, "re-rendering the same state changed pixels");
        first = Some((a, h.state().debug_state_json()));
    });
    harness::with_app("empty_shell_renders_identically", |h| {
        let (a, state) = first.take().expect("first instance ran");
        assert!(
            harness::render(h) == a,
            "a second app instance rendered different pixels"
        );
        assert_eq!(h.state().debug_state_json(), state);
    });
}

/// The five regions of the shell tile the window: ribbon on top, status at
/// the bottom, tree, viewport and properties between them, left to right.
#[test]
fn empty_shell_tiles_the_window() {
    harness::with_app("empty_shell_tiles_the_window", |h| {
        let state = h.state().debug_state();
        let [w, hgt] = state.window.expect("a frame ran");
        let l = state.layout.expect("a frame ran");
        assert_eq!(
            [w, hgt],
            [f64::from(harness::SIZE.x), f64::from(harness::SIZE.y)]
        );
        assert_eq!(l.ribbon, [0.0, 0.0, w, l.tree[1]]);
        assert_eq!(l.status, [0.0, l.tree[3], w, hgt]);
        assert_eq!(l.tree[2], l.viewport[0]);
        assert_eq!(l.viewport[2], l.properties[0]);
        assert_eq!(l.properties[2], w);
        for row in [l.tree, l.viewport, l.properties] {
            assert_eq!([row[1], row[3]], [l.tree[1], l.tree[3]]);
        }
        assert!(
            l.viewport[2] - l.viewport[0] > w / 2.0,
            "the viewport is the largest region"
        );
    });
}
