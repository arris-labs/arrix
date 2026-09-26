//! A scenario's golden (ADR-0001): one text file,
//! `tests/snapshots/<name>.json`, holding the `debug_state()` and the coarse
//! frame. The full frame is always written to `target/snapshots/<name>.png`
//! (or to `$ARRIX_SNAPSHOT_DIR`), and is never committed.

use std::path::PathBuf;

use arrix_app::ArrixApp;
use egui_kittest::Harness;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::coarse::{self, CELL, CoarseFrame, Tolerance};
use crate::harness;

#[derive(Serialize, Deserialize)]
struct Golden {
    debug_state: Value,
    coarse_frame: CoarseFrame,
}

/// Where full frames go: `$ARRIX_SNAPSHOT_DIR` when set (the baseline script
/// renders another revision's frames there), else `target/snapshots/`.
pub fn frame_dir() -> PathBuf {
    let dir = std::env::var_os("ARRIX_SNAPSHOT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| harness::target_dir().join("snapshots"));
    std::fs::create_dir_all(&dir).expect("create the snapshot frame directory");
    dir
}

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/snapshots")
        .join(format!("{name}.json"))
}

/// Compares the current state against the golden `name` with the default
/// tolerance, or rewrites the golden under `UPDATE_SNAPSHOTS=1`.
pub fn check(h: &mut Harness<'_, ArrixApp>, name: &str) {
    check_with(h, name, Tolerance::default());
}

pub fn check_with(h: &mut Harness<'_, ArrixApp>, name: &str, tolerance: Tolerance) {
    let frame = harness::render(h);
    let png = frame_dir().join(format!("{name}.png"));
    frame.save(&png).expect("write the frame");
    if harness::env_flag("ARRIX_SNAPSHOT_DIFF") {
        write_diff(name, &frame);
    }
    // Through text and back, as the golden was: two equal texts then give
    // equal values, whatever the float parser does.
    let state = serde_json::to_string(&h.state().debug_state()).expect("serialise debug_state");
    let actual = Golden {
        debug_state: serde_json::from_str(&state).expect("parse debug_state"),
        coarse_frame: CoarseFrame::reduce(&frame, CELL),
    };
    let path = golden_path(name);
    if harness::env_flag("UPDATE_SNAPSHOTS") {
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("create tests/snapshots");
        let text = serde_json::to_string_pretty(&actual).expect("serialise the golden");
        std::fs::write(&path, text + "\n").expect("write the golden");
        println!("updated {}", path.display());
        return;
    }
    let text = std::fs::read_to_string(&path).unwrap_or_else(|err| {
        panic!(
            "no golden at {} ({err}). Look at {} first; UPDATE_SNAPSHOTS=1 writes it.",
            path.display(),
            png.display()
        )
    });
    let golden: Golden = serde_json::from_str(&text)
        .unwrap_or_else(|err| panic!("malformed golden {}: {err}", path.display()));
    let mut problems = Vec::new();
    if let Some(diff) = first_difference("", &golden.debug_state, &actual.debug_state) {
        problems.push(format!("debug_state differs at {diff}"));
    }
    match coarse::check(&golden.coarse_frame, &actual.coarse_frame, tolerance) {
        Ok(absorbed) if !absorbed.is_empty() => {
            println!("{name}: {} region(s) within the budget", absorbed.len());
        }
        Ok(_) => {}
        Err(err) => problems.push(format!("coarse frame: {err}")),
    }
    assert!(
        problems.is_empty(),
        "snapshot `{name}` differs from {}:\n{}\n\nThe frame is {}. `scripts/snapshot-baseline` \
         renders the before and diff images; refresh with UPDATE_SNAPSHOTS=1 only for an \
         intended change, after reading them.",
        path.display(),
        problems.join("\n"),
        png.display()
    );
}

/// Writes `<name>.diff.png` against `<name>.before.png` beside the frame,
/// when `scripts/snapshot-baseline` rendered one, and says how many pixels
/// differ. Straight to stderr, so the test runner's capture cannot hide it.
fn write_diff(name: &str, after: &image::RgbaImage) {
    use std::io::Write as _;
    let dir = frame_dir();
    let before = dir.join(format!("{name}.before.png"));
    let line = match image::open(&before) {
        Ok(before) => {
            let (diff, differing) = diff_image(&before.to_rgba8(), after);
            let path = dir.join(format!("{name}.diff.png"));
            diff.save(&path).expect("write the diff image");
            format!(
                "{name}: {differing} pixels differ from the baseline, {}",
                path.display()
            )
        }
        Err(err) => format!("{name}: no baseline frame at {} ({err})", before.display()),
    };
    let _ = writeln!(std::io::stderr(), "{line}");
}

/// `after` dimmed to a quarter, with every pixel whose RGB differs from
/// `before` (or that `before` does not have) in magenta; and their count.
fn diff_image(before: &image::RgbaImage, after: &image::RgbaImage) -> (image::RgbaImage, usize) {
    let mut differing = 0;
    let diff = image::RgbaImage::from_fn(after.width(), after.height(), |x, y| {
        let a = after.get_pixel(x, y).0;
        let b = before.get_pixel_checked(x, y).map(|p| p.0);
        if b.is_some_and(|b| b[..3] == a[..3]) {
            image::Rgba([a[0] / 4, a[1] / 4, a[2] / 4, 255])
        } else {
            differing += 1;
            image::Rgba([255, 0, 255, 255])
        }
    });
    (diff, differing)
}

/// The first place `actual` departs from `golden`, as a JSON pointer and
/// both values, or `None` when they are equal.
fn first_difference(at: &str, golden: &Value, actual: &Value) -> Option<String> {
    match (golden, actual) {
        (Value::Object(g), Value::Object(a)) => {
            for (key, gv) in g {
                let here = format!("{at}/{key}");
                match a.get(key) {
                    None => return Some(format!("{here}: golden {gv}, now absent")),
                    Some(av) => {
                        if let Some(d) = first_difference(&here, gv, av) {
                            return Some(d);
                        }
                    }
                }
            }
            a.iter()
                .find(|(key, _)| !g.contains_key(*key))
                .map(|(key, av)| format!("{at}/{key}: absent in the golden, now {av}"))
        }
        (Value::Array(g), Value::Array(a)) => {
            for (i, (gv, av)) in g.iter().zip(a).enumerate() {
                if let Some(d) = first_difference(&format!("{at}/{i}"), gv, av) {
                    return Some(d);
                }
            }
            (g.len() != a.len())
                .then(|| format!("{at}: golden has {} items, now {}", g.len(), a.len()))
        }
        _ => (golden != actual).then(|| format!("{at}: golden {golden}, now {actual}")),
    }
}

#[cfg(test)]
mod tests {
    use super::{diff_image, first_difference};
    use serde_json::json;

    #[test]
    fn the_diff_image_highlights_only_what_changed() {
        let grey = image::Rgba([80, 80, 80, 255]);
        let before = image::RgbaImage::from_pixel(8, 4, grey);
        let (diff, n) = diff_image(&before, &before);
        assert_eq!(n, 0);
        assert!(diff.pixels().all(|p| p.0 == [20, 20, 20, 255]));
        let mut after = before.clone();
        after.put_pixel(3, 1, image::Rgba([90, 80, 80, 255]));
        let (diff, n) = diff_image(&before, &after);
        assert_eq!(n, 1);
        assert_eq!(diff.get_pixel(3, 1).0, [255, 0, 255, 255]);
        // A larger frame: everything the baseline lacks differs.
        let (_, n) = diff_image(&before, &image::RgbaImage::from_pixel(8, 6, grey));
        assert_eq!(n, 16);
    }

    #[test]
    fn names_the_first_differing_path() {
        let g = json!({"layout": {"status": [0.0, 876.0]}, "shell": {"tabs": ["a", "b"]}});
        assert_eq!(first_difference("", &g, &g), None);
        let a = json!({"layout": {"status": [0.0, 870.0]}, "shell": {"tabs": ["a"]}});
        assert_eq!(
            first_difference("", &g, &a).as_deref(),
            Some("/layout/status/1: golden 876.0, now 870.0")
        );
        let a = json!({"layout": {"status": [0.0, 876.0]}, "shell": {"tabs": ["a"]}});
        assert_eq!(
            first_difference("", &g, &a).as_deref(),
            Some("/shell/tabs: golden has 2 items, now 1")
        );
    }
}
