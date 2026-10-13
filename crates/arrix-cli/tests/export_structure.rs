//! `arrix export --assembly` as a user runs it (plans/c2-m3 step 10,
//! ADR-0032 §5): an assembly leaves as a STEP product tree in metres, and
//! `arrix import` reads the same tree back: the same parts and assembly by
//! name, the components at the same poses (1e-12 m), the same volumes and
//! the same colours.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn arrix(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_arrix"))
        .current_dir(workspace())
        .args(args)
        .output()
        .expect("run arrix")
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("export-structure")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.parent().unwrap()).unwrap();
    dir
}

fn ok(out: &Output) -> &Output {
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

fn eval(doc: &Path) -> Value {
    let out = arrix(&["eval".as_ref(), doc.as_os_str()]);
    serde_json::from_slice(&ok(&out).stdout).unwrap()
}

/// Exports `assembly` of `doc` to a file, imports it into a new directory,
/// and returns the file's text and the directory.
fn round_trip(doc: &Path, assembly: &str, name: &str) -> (String, PathBuf) {
    let file = scratch(name).with_extension("step");
    ok(&arrix(&[
        "export".as_ref(),
        doc.as_os_str(),
        "--format".as_ref(),
        "step".as_ref(),
        "--assembly".as_ref(),
        assembly.as_ref(),
        "-o".as_ref(),
        file.as_os_str(),
    ]));
    let back = scratch(name);
    ok(&arrix(&[
        "import".as_ref(),
        file.as_os_str(),
        "-o".as_ref(),
        back.as_os_str(),
    ]));
    (std::fs::read_to_string(file).unwrap(), back)
}

fn numbers(v: &Value) -> Vec<f64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_f64().unwrap())
        .collect()
}

/// A component's pose as twelve numbers: origin, x axis, z axis.
fn pose(c: &Value) -> Vec<f64> {
    let p = &c["pose"];
    ["origin", "x_axis", "z_axis"]
        .iter()
        .flat_map(|k| numbers(&p[k]))
        .collect()
}

/// The volumes of the final bodies, ascending.
fn volumes(line: &Value) -> Vec<f64> {
    let mut v: Vec<f64> = line["mass"]["bodies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["volume"].as_f64().unwrap())
        .collect();
    v.sort_by(f64::total_cmp);
    v
}

/// The poses of an evaluation's one assembly, ascending by origin so the
/// two documents' (different) component ids do not matter.
fn poses(line: &Value) -> Vec<Vec<f64>> {
    let assemblies = line["assemblies"].as_array().unwrap();
    assert_eq!(assemblies.len(), 1);
    let mut p: Vec<Vec<f64>> = assemblies[0]["components"]
        .as_array()
        .unwrap()
        .iter()
        .map(pose)
        .collect();
    p.sort_by(|a, b| {
        a.iter()
            .zip(b)
            .map(|(x, y)| x.total_cmp(y))
            .find(|o| o.is_ne())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    p
}

fn assert_close(a: &[Vec<f64>], b: &[Vec<f64>], tolerance: f64) {
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(b) {
        for (p, q) in x.iter().zip(y) {
            assert!((p - q).abs() <= tolerance, "{x:?} vs {y:?}");
        }
    }
}

#[test]
fn an_assembly_round_trips_with_its_structure_in_metres() {
    let source = workspace().join("tests/docs/assembly");
    let (text, back) = round_trip(&source, "Servo on bracket", "assembly");
    assert!(text.contains("SI_UNIT($,.METRE.)"), "declares metres");
    assert_eq!(text.matches("NEXT_ASSEMBLY_USAGE_OCCURRENCE(").count(), 2);
    for name in ["'Servo on bracket'", "'Bracket'", "'Servo'"] {
        assert!(text.contains(&format!("PRODUCT({name},")), "{name}");
    }

    let was = eval(&source);
    let now = eval(&back);
    assert_eq!(now["status"], "ok");
    assert_eq!(now["assemblies"][0]["name"], "Servo on bracket");
    let mut parts: Vec<String> = std::fs::read_dir(back.join("parts"))
        .unwrap()
        .map(|f| {
            let json: Value =
                serde_json::from_slice(&std::fs::read(f.unwrap().path()).unwrap()).unwrap();
            json["name"].as_str().unwrap().to_owned()
        })
        .collect();
    parts.sort();
    assert_eq!(parts, ["Bracket", "Servo"]);

    // Poses to rounding, volumes to rounding relative.
    assert_close(&poses(&was), &poses(&now), 1e-12);
    let (a, b) = (volumes(&was), volumes(&now));
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(&b) {
        assert!((x - y).abs() <= 1e-12 * x.abs(), "{x} vs {y}");
    }
}

/// A copy of the assembly document whose bracket has a red material.
fn painted() -> PathBuf {
    let copy = scratch("painted-source");
    fn cp(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let path = entry.unwrap().path();
            let target = to.join(path.file_name().unwrap());
            if path.is_dir() {
                cp(&path, &target);
            } else {
                std::fs::copy(&path, &target).unwrap();
            }
        }
    }
    cp(&workspace().join("tests/docs/assembly"), &copy);
    let id = "0000000000001";
    std::fs::create_dir_all(copy.join("materials")).unwrap();
    std::fs::write(
        copy.join(format!("materials/{id}.json")),
        format!(
            "{{\n  \"appearance\": [1.0, 0.25, 0.5],\n  \"density\": \"2700 kg/m3\",\n  \"id\": \"{id}\",\n  \"name\": \"red\"\n}}\n"
        ),
    )
    .unwrap();
    let part = copy.join("parts/BC6X598GYWPZN.json");
    let mut json: Value = serde_json::from_slice(&std::fs::read(&part).unwrap()).unwrap();
    json["material"] = Value::String(id.into());
    std::fs::write(part, serde_json::to_vec_pretty(&json).unwrap()).unwrap();
    copy
}

#[test]
fn a_materials_appearance_is_the_colour_the_file_carries_back() {
    let source = painted();
    assert_eq!(eval(&source)["status"], "ok");
    let (_, back) = round_trip(&source, "Servo on bracket", "painted");
    let now = eval(&back);
    let materials = now["materials"].as_array().unwrap();
    assert_eq!(materials.len(), 1, "{materials:?}");
    let colour = numbers(&materials[0]["appearance"]);
    for (got, want) in colour.iter().zip([1.0, 0.25, 0.5]) {
        assert!((got - want).abs() < 1e-9, "{colour:?}");
    }
}

#[test]
fn an_unknown_assembly_and_a_non_step_format_are_refused() {
    let source = workspace().join("tests/docs/assembly");
    let file = scratch("refused").with_extension("step");
    let run = |assembly: &str, format: &str| {
        arrix(&[
            "export".as_ref(),
            source.as_os_str(),
            "--format".as_ref(),
            format.as_ref(),
            "--assembly".as_ref(),
            assembly.as_ref(),
            "-o".as_ref(),
            file.as_os_str(),
        ])
    };
    let none = run("nothing", "step");
    assert_eq!(none.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&none.stderr).contains("no such assembly"));
    let stl = run("Servo on bracket", "stl");
    assert_eq!(stl.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&stl.stderr).contains("--format step"));
}
