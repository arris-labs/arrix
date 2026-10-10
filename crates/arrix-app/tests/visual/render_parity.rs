//! The headless render (`arrix-app render`, ADR-0025) against the harness:
//! the same document, settled, drawn by both, agrees as a scenario's golden
//! would.

use arrix_app::render::{RenderOptions, render_with};

use crate::coarse::{CELL, CoarseFrame, Tolerance, check};
use crate::harness;

/// The slice drawn by the product's render and by the visual harness on one
/// device: identical pixels (measured so in plans/boundary-m2 step 2; the
/// coarse check first, so a regression names its regions) and the same
/// `debug_state()`.
#[test]
fn the_render_draws_what_the_harness_draws() {
    harness::with_app("the_render_draws_what_the_harness_draws", |h| {
        harness::open_settled(h, "tests/docs/slice");
        h.state_mut().skip_animations();
        h.run();
        let expected = harness::render(h);
        let state = h.state().debug_state_json();

        let options = RenderOptions {
            size: harness::SIZE,
            ..RenderOptions::default()
        };
        let rendered = render_with(&options, |app| {
            app.open_path_without(std::path::Path::new("../../tests/docs/slice"), &[])
        })
        .unwrap_or_else(|err| panic!("{err}"));
        let [w, hgt] = rendered.size;
        let actual = image::RgbaImage::from_raw(w, hgt, rendered.pixels).expect("RGBA pixels");

        let differing = expected
            .pixels()
            .zip(actual.pixels())
            .filter(|(a, b)| a != b)
            .count();
        check(
            &CoarseFrame::reduce(&expected, CELL),
            &CoarseFrame::reduce(&actual, CELL),
            Tolerance::default(),
        )
        .unwrap_or_else(|err| panic!("the render's frame is not the harness's: {err}"));
        assert_eq!(differing, 0, "pixels that differ from the harness's frame");
        assert_eq!(rendered.debug_state, state);
    });
}
