//! `robotics.urdf` held to two independent readers (plans/c3-m2 step 10):
//! `scripts/urdf-read` ran urdfdom's `check_urdf` and yourdfpy on the
//! arm's URDF (`c3-m2.arm.txt`) at two joint configurations and wrote
//! `c3-m2.urdf.txt`. Here ArriX's own forward kinematics (`place` at the
//! same values) and mass pass on `tests/docs/arm` are held to that reading:
//! every part where the readers put its geometry, its centre of mass and
//! its inertia, every joint's frame and axis, within 1e-9 m and rad. The
//! readers know nothing of ArriX, so a disagreement is a frame convention
//! `robotics` gets wrong; the fix goes there, and the golden is written
//! again by the script, never by hand.

mod common;

use std::collections::BTreeMap;

use arrix_core::{DMat3, DVec3, Frame, JointId};
use arrix_doc::{ComponentOf, Evaluator, JointKind, JointOutcome, MemorySource, open, place};
use common::{read_tree, registry, workspace};

const READING: &str = include_str!("golden/c3-m2.urdf.txt");
const ARM: &str = include_str!("golden/c3-m2.arm.txt");
const TOL: f64 = 1e-9;

/// A line's `key=value…` fields; a value runs to the next key.
fn fields(rest: &str) -> BTreeMap<&str, Vec<&str>> {
    let mut out: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut key = "";
    for token in rest.split_whitespace() {
        if let Some((k, v)) = token.split_once('=') {
            key = k;
            out.insert(k, vec![v]);
        } else {
            out.get_mut(key).expect("a value follows a key").push(token);
        }
    }
    out
}

/// A field's numbers; `None` for `-`.
fn numbers(f: &BTreeMap<&str, Vec<&str>>, key: &str) -> Option<Vec<f64>> {
    let v = &f[key];
    (v != &["-"]).then(|| v.iter().map(|n| n.parse().unwrap()).collect())
}

/// One configuration's reading, by link and by joint name.
struct At {
    q: BTreeMap<String, f64>,
    poses: BTreeMap<String, BTreeMap<String, Option<Vec<f64>>>>,
    frames: BTreeMap<String, (DVec3, DVec3)>,
}

/// A joint as yourdfpy read it.
struct JointRead {
    kind: String,
    parent: String,
    child: String,
    limits: Option<Vec<f64>>,
}

struct Reading {
    blake3: String,
    tree: BTreeMap<String, String>,
    masses: BTreeMap<String, Option<f64>>,
    joints: BTreeMap<String, JointRead>,
    ats: Vec<At>,
}

fn reading() -> Reading {
    let mut r = Reading {
        blake3: String::new(),
        tree: BTreeMap::new(),
        masses: BTreeMap::new(),
        joints: BTreeMap::new(),
        ats: Vec::new(),
    };
    for line in READING.lines().filter(|l| !l.starts_with('#')) {
        let (kind, rest) = line.split_once(' ').unwrap();
        let (name, rest) = rest.split_once(' ').unwrap_or((rest, ""));
        let f = fields(rest);
        match kind {
            "file" => r.blake3 = f["blake3"][0].into(),
            "tree" => {
                r.tree.insert(name.into(), f["parent"][0].into());
            }
            "link" => {
                r.masses
                    .insert(name.into(), numbers(&f, "mass").map(|m| m[0]));
            }
            "joint" => {
                let s = |k: &str| f[k][0].to_string();
                r.joints.insert(
                    name.into(),
                    JointRead {
                        kind: s("type"),
                        parent: s("parent"),
                        child: s("child"),
                        limits: numbers(&f, "limit"),
                    },
                );
            }
            "at" => {
                let q = fields(line.strip_prefix("at ").unwrap())
                    .into_iter()
                    .map(|(j, v)| (j.to_string(), v[0].parse().unwrap()))
                    .collect();
                r.ats.push(At {
                    q,
                    poses: BTreeMap::new(),
                    frames: BTreeMap::new(),
                });
            }
            "pose" => {
                let p = ["link", "visual", "com", "inertia"]
                    .map(|k| (k.to_string(), numbers(&f, k)))
                    .into();
                r.ats.last_mut().unwrap().poses.insert(name.into(), p);
            }
            "frame" => {
                let v = |k| DVec3::from_slice(&numbers(&f, k).unwrap());
                let at = r.ats.last_mut().unwrap();
                at.frames.insert(name.into(), (v("origin"), v("axis")));
            }
            other => panic!("an unknown line kind {other}"),
        }
    }
    r
}

/// A 3x4 row-major matrix as its rotation and origin.
fn matrix(m: &[f64]) -> (DMat3, DVec3) {
    let col = |c: usize| DVec3::new(m[c], m[4 + c], m[8 + c]);
    (DMat3::from_cols(col(0), col(1), col(2)), col(3))
}

fn rotation(f: &Frame) -> DMat3 {
    DMat3::from_cols(f.x_axis(), f.y_axis(), f.z_axis())
}

/// `local` (in `old`'s frame) where it is once `old` has moved to `new`.
fn carried(old: &Frame, new: &Frame, point: DVec3) -> DVec3 {
    new.origin() + rotation(new) * (rotation(old).transpose() * (point - old.origin()))
}

#[track_caller]
fn near(what: &str, a: DVec3, b: DVec3) {
    assert!(
        (a - b).length() < TOL,
        "{what}: ArriX {a} vs the readers' {b}"
    );
}

/// As `robotics` names a link: unsafe characters become `_`.
fn link_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[test]
fn the_readers_read_the_urdf_the_arm_golden_holds() {
    let urdf = ARM
        .split_once("\nmeshes (bytes, blake3):\n")
        .expect("the arm golden lists its meshes")
        .0;
    assert_eq!(
        reading().blake3,
        blake3::hash(urdf.as_bytes()).to_hex().as_str(),
        "c3-m2.arm.txt changed: write c3-m2.urdf.txt again with scripts/urdf-read"
    );
}

#[test]
fn urdfdom_and_yourdfpy_agree_on_the_tree() {
    let r = reading();
    assert_eq!(r.tree.len(), r.masses.len(), "a link per tree line");
    let edges: BTreeMap<String, String> = r
        .joints
        .values()
        .map(|j| (j.child.clone(), j.parent.clone()))
        .collect();
    let tree: BTreeMap<String, String> = r
        .tree
        .iter()
        .filter(|(_, p)| *p != "-")
        .map(|(c, p)| (c.clone(), p.clone()))
        .collect();
    assert_eq!(tree, edges);
}

#[test]
fn arrix_places_and_weighs_the_arm_as_the_readers_do() {
    let mut files = BTreeMap::new();
    read_tree(&workspace().join("tests/docs/arm"), "", &mut files);
    let doc = open(&MemorySource(files)).expect("the arm opens");
    let evaluation = Evaluator::new(registry()).evaluate(&doc);
    let (&id, assembly) = doc.assemblies().iter().next().unwrap();
    let outcome = &evaluation.assemblies[&id];
    let name_of = |path: &[arrix_core::ComponentId]| {
        assert_eq!(path.len(), 1, "the arm is one level deep");
        link_name(&assembly.components[&path[0]].name)
    };
    let r = reading();

    // Joints: kind, parent and child links, limits in SI.
    let mut anchors: BTreeMap<String, (JointId, Frame, String)> = BTreeMap::new();
    for joint in assembly.joints.values() {
        let JointRead {
            kind,
            parent,
            child,
            limits,
        } = &r.joints[&joint.name];
        let want = match (joint.kind, &joint.limits) {
            (JointKind::Revolute, Some(_)) => "revolute",
            (JointKind::Revolute, None) => "continuous",
            (JointKind::Prismatic, _) => "prismatic",
            (JointKind::Fixed, _) => "fixed",
        };
        assert_eq!(kind, want, "{}", joint.name);
        assert_eq!(parent, &name_of(&joint.parent.path));
        assert_eq!(child, &name_of(&joint.child.path));
        if let Some(l) = &joint.limits {
            let si = l.clone().map(|e| e.evaluate(&|_| None).unwrap());
            let read = limits.as_ref().expect("limits are written");
            for (a, b) in si.iter().zip(read) {
                assert!((a - b).abs() < TOL, "{}: {si:?} vs {read:?}", joint.name);
            }
        }
        let JointOutcome::Ok { parent: at, .. } = &outcome.joints[&joint.id] else {
            panic!("{} solves", joint.name)
        };
        anchors.insert(
            joint.name.clone(),
            (joint.id, *at, name_of(&joint.parent.path)),
        );
    }

    assert_eq!(r.ats.len(), 2, "two configurations");
    let solved: BTreeMap<String, Frame> = outcome
        .components
        .iter()
        .map(|c| (name_of(&c.path), c.pose))
        .collect();
    for at in &r.ats {
        let overrides: Vec<(JointId, f64)> = at.q.iter().map(|(j, &q)| (anchors[j].0, q)).collect();
        let placed = place(&doc, id, outcome, &evaluation.assemblies, &overrides).unwrap();
        let mut seen = 0;
        for c in &placed {
            let ComponentOf::Part(part) = assembly.components[&c.path[0]].of else {
                continue;
            };
            let link = name_of(&c.path);
            let read = &at.poses[&link];
            let what = |w: &str| format!("{link} {w} at {:?}", at.q);
            seen += 1;

            // The part's geometry is where the readers put the visual.
            let (rot, origin) = matrix(read["visual"].as_ref().unwrap());
            near(&what("origin"), c.pose.origin(), origin);
            near(&what("x"), c.pose.x_axis(), rot.x_axis);
            near(&what("z"), c.pose.z_axis(), rot.z_axis);

            // Mass, centre of mass and inertia, in base_link.
            let m = evaluation.masses.parts[&part].mass.as_ref().unwrap();
            let mass = r.masses[&link].unwrap();
            assert!((m.mass - mass).abs() <= TOL * mass, "{link}: mass");
            let com = c.pose.origin() + rotation(&c.pose) * DVec3::from_array(m.centroid);
            near(
                &what("centre of mass"),
                com,
                DVec3::from_slice(read["com"].as_ref().unwrap()),
            );
            let r3 = rotation(&c.pose);
            let local = DMat3::from_cols_array_2d(&m.inertia).transpose();
            let ours = r3 * local * r3.transpose();
            let theirs = DMat3::from_cols_slice(read["inertia"].as_ref().unwrap()).transpose();
            let scale = ours
                .to_cols_array()
                .iter()
                .fold(0.0f64, |a, v| a.max(v.abs()));
            for (a, b) in ours.to_cols_array().iter().zip(theirs.to_cols_array()) {
                assert!((a - b).abs() <= TOL * scale, "{link} inertia at {:?}", at.q);
            }
        }
        assert_eq!(seen + 1, at.poses.len(), "a link per part, and base_link");

        // A joint's frame is its parent anchor, carried by the parent part.
        let now: BTreeMap<String, Frame> =
            placed.iter().map(|c| (name_of(&c.path), c.pose)).collect();
        let moving = r.joints.values().filter(|j| j.kind != "fixed").count();
        assert_eq!(at.frames.len(), moving, "a frame per moving joint");
        for (name, (_, anchor, parent)) in &anchors {
            let Some((origin, axis)) = at.frames.get(name) else {
                assert_eq!(r.joints[name].kind, "fixed", "{name} has a frame");
                continue;
            };
            let (old, new) = (&solved[parent], &now[parent]);
            let here = carried(old, new, anchor.origin());
            let z = rotation(new) * (rotation(old).transpose() * anchor.z_axis());
            near(&format!("{name} origin at {:?}", at.q), here, *origin);
            near(&format!("{name} axis at {:?}", at.q), z, *axis);
        }
    }
}
