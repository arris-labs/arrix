//! `arrix export --exporter` and `describe`'s exporters (plans/c3-m2 step
//! 5): the core's formats by id, a refused id naming those there are, and
//! the flags that go with a plugin's exporter. A plugin's own exporter is
//! exercised end to end by the accept (step 11); `export.rs`'s unit tests
//! hold the writing of several files.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("export-exporter");
    std::fs::create_dir_all(&dir).expect("create the scratch directory");
    dir.join(name)
}

fn arrix(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_arrix"))
        .current_dir(workspace())
        .args(args)
        .output()
        .expect("run arrix")
}

#[test]
fn a_core_format_by_id_is_the_format_flag() {
    for ext in ["step", "stl", "obj"] {
        let (by_id, by_format) = (
            scratch(&format!("id.{ext}")),
            scratch(&format!("fmt.{ext}")),
        );
        let id = format!("core.{ext}");
        for (flag, value, out) in [
            ("--exporter", id.as_str(), &by_id),
            ("--format", ext, &by_format),
        ] {
            let run = arrix(&[
                "export",
                "tests/docs/slice",
                flag,
                value,
                "-o",
                out.to_str().unwrap(),
            ]);
            assert_eq!(
                run.status.code(),
                Some(0),
                "{}",
                String::from_utf8_lossy(&run.stderr)
            );
        }
        assert_eq!(
            std::fs::read(&by_id).unwrap(),
            std::fs::read(&by_format).unwrap(),
            "core.{ext} is --format {ext}"
        );
    }
}

#[test]
fn an_exporter_nobody_has_is_refused_naming_those_there_are() {
    let out = scratch("none.txt");
    let run = arrix(&[
        "export",
        "tests/docs/slice",
        "--exporter",
        "nobody.none",
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_eq!(run.status.code(), Some(2));
    let err = String::from_utf8_lossy(&run.stderr);
    assert!(
        err.contains("nobody.none") && err.contains("core.stl"),
        "{err}"
    );
    assert!(!out.exists() || std::fs::read(&out).unwrap().is_empty());
}

#[test]
fn the_flags_of_the_two_ways_do_not_mix() {
    let out = scratch("mix.txt");
    let out = out.to_str().unwrap();
    for extra in [
        &["--format", "stl", "--exporter", "core.stl"][..],
        &["--format", "stl", "--option", "a=1"],
        &["--exporter", "core.stl", "--option", "a=1"],
        &["--exporter", "core.stl", "--part", "p"],
    ] {
        let run = arrix(&[&["export", "tests/docs/slice"], extra, &["-o", out]].concat());
        assert_ne!(run.status.code(), Some(0), "{extra:?}");
    }
    let run = arrix(&["export", "tests/docs/slice", "-o", out]);
    assert_ne!(
        run.status.code(),
        Some(0),
        "one of --format and --exporter is needed"
    );
}

#[test]
fn describe_lists_the_core_formats_and_compiled_in_exporters() {
    let run = arrix(&["describe", "--json"]);
    assert_eq!(run.status.code(), Some(0));
    let d: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    let ids: Vec<&str> = d["exporters"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["core.step", "core.stl", "core.obj", "robotics.urdf"]);
    let text = arrix(&["describe"]);
    assert!(String::from_utf8_lossy(&text.stdout).contains("exporter core.stl"));
}
