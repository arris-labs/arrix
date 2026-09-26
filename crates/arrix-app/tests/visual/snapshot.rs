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
    use super::first_difference;
    use serde_json::json;

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
