//! C3's accept (plans/c3-m3 step 16, SEED.md §7): the north star. `arrix
//! serve` with `fasteners` (Tier 2) and `robotics` (a Tier 1 component),
//! both loaded by `--plugin` from their own directories, opens
//! `tests/docs/north-star`; `fasteners.place` on the three counterbores'
//! edges seats a screw on each hole; `robotics.urdf` exports the URDF that
//! step 15's golden holds to urdfdom and yourdfpy; and the saved document,
//! opened with neither plugin, keeps every body, freezes each
//! `fasteners.socket-head` with its reason, and still moves every hole and
//! its screw together under an edit of the plate's thickness and one of the
//! pattern's spacing (a width edit moves neither: Findings 3).

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command as Process;

use arrix_core::{ComponentId, DVec3, Frame, JointId};
use arrix_doc::{
    Command, Document, Evaluator, FeatureEdit, FeatureOutcome, FrozenReason, JointOutcome,
    MemorySource, apply, expr::Expr, open,
};
use common::north_star::{ASSEMBLY, BRACKET, place, serve_with};
use common::tier1::build;
use common::{read_tree, registry, workspace};

const MM: f64 = 1e-3;
const THICKNESS: &str = "D1BPZWKQ716XG";
const WIDTH: &str = "4SZ1VMCFJ1YDG";
const PATTERN: &str = "E635NVKB74361";
/// The plate's top, where the screws' heads sit.
const TOP: f64 = 5.0 * MM;
const URDF: &str = "Servo_on_bracket.urdf";
const TOL: f64 = 1e-9;

fn scratch() -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join("c3-accept")
}

fn e(text: &str) -> Expr {
    text.parse().unwrap()
}

fn id<T: std::str::FromStr>(text: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    text.parse().unwrap()
}

#[track_caller]
fn near(a: DVec3, b: DVec3, what: &str) {
    assert!((a - b).length() < TOL, "{what}: {a} vs {b}");
}

/// Each screw's pose and its joint's parent frame (the hole edge's, in
/// the world), every joint resolved with no diagnostic.
fn screws(evaluator: &mut Evaluator, doc: &Document) -> Vec<(Frame, Frame)> {
    let evaluation = evaluator.evaluate(doc);
    let assembly = &doc.assemblies()[&id::<arrix_core::AssemblyId>(ASSEMBLY)];
    let outcome = &evaluation.assemblies[&assembly.id];
    let mut found: Vec<(ComponentId, JointId)> = assembly
        .joints
        .values()
        .filter(|j| j.child.path[0].to_string() != "FBZB73GQ7R5Z0")
        .filter(|j| j.child.path[0].to_string() != "FRDKPQPGN4XT2")
        .map(|j| (j.child.path[0], j.id))
        .collect();
    found.sort();
    found
        .into_iter()
        .map(|(c, j)| {
            let JointOutcome::Ok {
                parent,
                diagnostics,
                ..
            } = &outcome.joints[&j]
            else {
                panic!("{:?}", outcome.joints[&j])
            };
            assert!(diagnostics.is_empty(), "{diagnostics:?}");
            (outcome.pose(&[c]).unwrap(), *parent)
        })
        .collect()
}

/// How far an edit moves the hole of an index.
type Move = fn(usize) -> DVec3;

fn applied(doc: &Document, command: &Command) -> Document {
    apply(doc, command)
        .unwrap_or_else(|err| panic!("{command:?}: {err}"))
        .document
}

fn set(param: &str, expr: &str) -> Command {
    Command::SetParam {
        param: id(param),
        expr: e(expr),
    }
}

fn spacing(expr: &str) -> Command {
    Command::EditFeature {
        feature: id(PATTERN),
        edit: FeatureEdit {
            params: [("spacing".into(), Some(e(expr)))].into(),
            ..FeatureEdit::default()
        },
    }
}

#[test]
fn the_north_star_runs_with_both_plugins_and_survives_without_them() {
    let robotics = build::robotics();
    let fasteners = workspace().join("plugins/fasteners");
    let mut s = serve_with(&[robotics]);

    // Screws on the three counterbores, each on its hole.
    let assembled = place(&mut s);
    for joint in assembled["outcome"]["joints"].as_object().unwrap().values() {
        assert_eq!(joint["status"], "ok", "{joint}");
    }
    let heads: Vec<&serde_json::Value> = assembled["outcome"]["components"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| {
            !["FBZB73GQ7R5Z0", BRACKET, "FRDKPQPGN4XT2"].contains(&c["path"][0].as_str().unwrap())
        })
        .collect();
    assert_eq!(heads.len(), 3, "a screw per counterbore");
    for (n, screw) in heads.iter().enumerate() {
        let at = [0, 1, 2].map(|i| screw["pose"]["origin"][i].as_f64().unwrap());
        let want = [20.0 * MM, (-12.0 + 10.0 * n as f64) * MM, TOP];
        for (a, b) in at.iter().zip(want) {
            assert!((a - b).abs() < TOL, "screw {n}: {at:?} vs {want:?}");
        }
        let up = screw["pose"]["z_axis"][2].as_f64().unwrap();
        assert!((up - 1.0).abs() < TOL, "screw {n} points {up}");
    }

    // The saved document, then the URDF of it from a build with both plugins.
    let saved = scratch().join("north-star");
    common::north_star::save(&mut s, &saved);
    drop(s);
    let out = scratch().join("urdf");
    let _ = std::fs::remove_dir_all(&out);
    let run = Process::new(env!("CARGO_BIN_EXE_arrix"))
        .current_dir(workspace())
        .env("PYTHONPATH", workspace().join("python"))
        .args(["export", saved.to_str().unwrap()])
        .args(["--plugin", fasteners.to_str().unwrap()])
        .args(["--plugin", robotics.to_str().unwrap()])
        .args(["--exporter", "robotics.urdf", "--assembly", ASSEMBLY])
        .args(["-o", out.to_str().unwrap()])
        .output()
        .expect("run arrix");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let mut files = BTreeMap::new();
    read_tree(&out, "", &mut files);
    let reading = common::urdf::reading(include_str!("golden/c3-accept.urdf.txt"));
    assert_eq!(
        reading.blake3,
        blake3::hash(&files[URDF]).to_hex().as_str(),
        "the URDF is not the one the readers read (step 15's golden)"
    );

    // Opened with neither plugin: every body is there, each screw's
    // feature frozen with its reason.
    let mut tree = BTreeMap::new();
    read_tree(&saved, "", &mut tree);
    let doc = open(&MemorySource(tree)).expect("the saved document opens");
    let mut without = Evaluator::new(registry());
    let evaluation = without.evaluate(&doc);
    let mut frozen = 0;
    for part in doc.parts().values() {
        for feature in part.features.values() {
            if feature.type_id.to_string() != "fasteners.socket-head" {
                continue;
            }
            let Some(FeatureOutcome::Ok {
                frozen: Some(f), ..
            }) = evaluation.outcome(feature.id)
            else {
                panic!("{:?}", evaluation.outcome(feature.id))
            };
            assert_eq!(f.reason, FrozenReason::Missing);
            frozen += 1;
        }
    }
    assert_eq!(frozen, 1, "one screw size, one part");
    for part in doc.parts().values() {
        for feature in part.features.values() {
            assert!(
                evaluation
                    .outcome(feature.id)
                    .is_some_and(|o| { matches!(o, FeatureOutcome::Ok { .. }) }),
                "{:?}",
                evaluation.outcome(feature.id)
            );
        }
    }

    // Thickness moves every hole up and its screw with it; spacing moves
    // each copy by its index times the change; width moves neither.
    let before = screws(&mut without, &doc);
    assert_eq!(before.len(), 3);
    let moves: [(Command, Move); 3] = [
        (set(WIDTH, "70 mm"), |_| DVec3::ZERO),
        (set(THICKNESS, "6 mm"), |_| DVec3::Z * MM),
        (spacing("12 mm"), |n| DVec3::Y * 2.0 * MM * n as f64),
    ];
    for (edit, moved) in moves {
        let after = screws(&mut without, &applied(&doc, &edit));
        for (n, (b, a)) in before.iter().zip(&after).enumerate() {
            near(a.1.origin() - b.1.origin(), moved(n), "the hole");
            near(a.0.origin() - b.0.origin(), moved(n), "its screw");
        }
    }
}
