//! The assembly pass (plans/c2-m1 step 11, ADR-0029 §3 to §5): the servo
//! of `tests/fixtures/step/` placed by each joint kind, flipped and not,
//! and nested two deep, against hand-computed poses; loops, missing
//! placements, limits and lost anchors as diagnostics; the outcome cached.

use std::collections::BTreeMap;
use std::f64::consts::FRAC_PI_2;
use std::path::Path;

use arrix_core::{
    AssemblyId, ComponentId, DVec3, FeatureId, Frame, Id, JointId, NameRoot, ParamId, PartId,
    PersistentName, QuantityKind, TopoKind,
};
use arrix_doc::{
    Anchor, Assembly, AssemblyOutcome, Command, Component, ComponentOf, Document, Evaluation,
    Evaluator, FeatureRecord, FeatureTypeId, ImportedFile, Joint, JointKind, JointOutcome, Param,
    Part, Pose, Registry, apply, expr::Expr,
};
use arrix_kernel::Kernel;

const MM: f64 = 1e-3;
const SERVO: PartId = PartId(Id(1));
const IMPORT: FeatureId = FeatureId(Id(2));
const ANGLE: ParamId = ParamId(Id(3));
const ASM: AssemblyId = AssemblyId(Id(20));
const MID: AssemblyId = AssemblyId(Id(21));
const OUTER: AssemblyId = AssemblyId(Id(22));
const BASE: ComponentId = ComponentId(Id(30));
const ARM: ComponentId = ComponentId(Id(31));
const SPARE: ComponentId = ComponentId(Id(32));
const HINGE: JointId = JointId(Id(40));
const SECOND: JointId = JointId(Id(41));

fn servo() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/step/servo.step");
    std::fs::read(path).expect("the servo fixture")
}

fn e(text: &str) -> Expr {
    Expr::parse(text).unwrap()
}

fn with(doc: &Document, command: Command) -> Document {
    apply(doc, &command)
        .unwrap_or_else(|e| panic!("{command:?}: {e}"))
        .document
}

/// The servo's faces an anchor names, found through the kernel by their
/// geometry: the shaft (radius 3 at (10, 10)), the bore (radius 2 at
/// (32, 10)) and the box's top (z = 36), and one vertex.
struct Names {
    shaft: PersistentName,
    bore: PersistentName,
    top: PersistentName,
    vertex: PersistentName,
}

fn names() -> Names {
    let mut k = Kernel::new();
    let body = k.import_step(IMPORT, &servo(), 1).unwrap();
    let faces: Vec<PersistentName> = k
        .names(body)
        .unwrap()
        .of_kind(TopoKind::Face)
        .cloned()
        .collect();
    let vertex = k
        .names(body)
        .unwrap()
        .of_kind(TopoKind::Vertex)
        .next()
        .unwrap()
        .clone();
    let (mut shaft, mut bore, mut top) = (None, None, None);
    for name in faces {
        if let Ok(f) = k.face_frame(body, &name) {
            if near(f.z_axis(), DVec3::Z) && (f.origin().z - 36.0 * MM).abs() < 1e-12 {
                top = Some(name);
            }
        } else if let Ok(f) = k.anchor_frame(body, &name, None) {
            let at = f.origin().truncate() / MM;
            if (at - arrix_core::DVec2::new(10.0, 10.0)).length() < 1e-9 {
                shaft = Some(name);
            } else if (at - arrix_core::DVec2::new(32.0, 10.0)).length() < 1e-9 {
                bore = Some(name);
            }
        }
    }
    Names {
        shaft: shaft.unwrap(),
        bore: bore.unwrap(),
        top: top.unwrap(),
        vertex,
    }
}

/// The servo imported as one part, and a parameter `angle`.
fn servo_doc(angle: &str) -> Document {
    let bytes = servo();
    let mut doc = with(
        &Document::default(),
        Command::AddParam {
            param: ANGLE,
            record: Param {
                name: "angle".into(),
                kind: QuantityKind::Angle,
                expr: e(angle),
            },
        },
    );
    doc = with(
        &doc,
        Command::AddPart {
            part: Part {
                id: SERVO,
                name: "Servo".into(),
                history: vec![],
                features: BTreeMap::new(),
                rollback: None,
            },
        },
    );
    let import = FeatureRecord {
        id: IMPORT,
        type_id: FeatureTypeId::new("core.import-step").unwrap(),
        type_version: 1,
        name: "Servo".into(),
        params: BTreeMap::new(),
        choices: BTreeMap::new(),
        inputs: BTreeMap::new(),
        suppressed: false,
        sketch: None,
        frozen: None,
        file: Some(ImportedFile {
            name: "servo.step".into(),
            blob: arrix_doc::BlobRef::of(&bytes, "step"),
        }),
    };
    with(
        &doc,
        Command::Group {
            label: "import servo.step".into(),
            commands: vec![
                Command::AddFile { bytes },
                Command::AddFeature {
                    part: SERVO,
                    at: 0,
                    record: import,
                },
            ],
        },
    )
}

fn component(id: ComponentId, name: &str, of: ComponentOf, pose: Option<Pose>) -> Component {
    Component {
        id,
        name: name.into(),
        of,
        pose,
    }
}

fn pose(position: [&str; 3], rpy: [&str; 3]) -> Pose {
    Pose {
        position: position.map(e),
        rpy: rpy.map(e),
    }
}

/// The base grounded at (1, 2, 3) mm turned a quarter about z.
fn base() -> Component {
    let at = pose(["1 mm", "2 mm", "3 mm"], ["0 deg", "0 deg", "90 deg"]);
    component(BASE, "base", ComponentOf::Part(SERVO), Some(at))
}

fn anchor(path: Vec<ComponentId>, axis: &PersistentName, origin: &PersistentName) -> Anchor {
    Anchor {
        path,
        axis: axis.clone(),
        origin: Some(origin.clone()),
    }
}

/// The arm's shaft in the base's bore, both on the top's plane.
fn hinge(n: &Names, id: JointId, kind: JointKind, value: &str) -> Joint {
    Joint {
        id,
        name: format!("j{}", id.0.0),
        kind,
        parent: anchor(vec![BASE], &n.bore, &n.top),
        child: anchor(vec![ARM], &n.shaft, &n.top),
        value: e(value),
        offset: e(if kind == JointKind::Prismatic {
            "0 mm"
        } else {
            "0 deg"
        }),
        limits: None,
        flip: false,
    }
}

fn assembly(id: AssemblyId, components: Vec<Component>, joints: Vec<Joint>) -> Assembly {
    Assembly {
        id,
        name: format!("a{}", id.0.0),
        components: components.into_iter().map(|c| (c.id, c)).collect(),
        joints: joints.into_iter().map(|j| (j.id, j)).collect(),
    }
}

fn with_assemblies(mut doc: Document, assemblies: Vec<Assembly>) -> Document {
    for a in assemblies {
        doc = with(&doc, Command::AddAssembly { assembly: a });
    }
    doc
}

fn eval(doc: &Document) -> Evaluation {
    Evaluator::new(Registry::with_core_types()).evaluate(doc)
}

fn near(a: DVec3, b: DVec3) -> bool {
    (a - b).length() < 1e-12
}

fn mm(x: f64, y: f64, z: f64) -> DVec3 {
    DVec3::new(x, y, z) * MM
}

#[track_caller]
fn assert_pose(f: Frame, origin: DVec3, x: DVec3, z: DVec3) {
    assert!(
        near(f.origin(), origin) && near(f.x_axis(), x) && near(f.z_axis(), z),
        "{f:?}, expected at {origin} with x {x} and z {z}"
    );
}

fn outcome(e: &Evaluation, id: AssemblyId) -> &AssemblyOutcome {
    &e.assemblies[&id]
}

fn pose_of(e: &Evaluation, id: AssemblyId, path: &[ComponentId]) -> Frame {
    outcome(e, id).pose(path).unwrap()
}

fn codes(o: &AssemblyOutcome, path: &[ComponentId]) -> Vec<String> {
    let c = o.components.iter().find(|c| c.path == path).unwrap();
    c.diagnostics.iter().map(|d| d.code.to_string()).collect()
}

fn joint_code(o: &AssemblyOutcome, id: JointId) -> String {
    match &o.joints[&id] {
        JointOutcome::Failed { diagnostic } => diagnostic.code.to_string(),
        JointOutcome::Ok { diagnostics, .. } => diagnostics
            .iter()
            .map(|d| d.code.to_string())
            .collect::<Vec<_>>()
            .join(","),
    }
}

/// The servo, its base grounded and its arm placed by `joint`.
fn hinged(angle: &str, joint: Joint) -> Document {
    let arm = component(ARM, "arm", ComponentOf::Part(SERVO), None);
    let a = assembly(ASM, vec![base(), arm], vec![joint]);
    with_assemblies(servo_doc(angle), vec![a])
}

#[test]
fn each_joint_kind_places_the_arm_at_its_hand_computed_pose() {
    let n = names();
    let arm = |angle: &str, joint: Joint| pose_of(&eval(&hinged(angle, joint)), ASM, &[ARM]);
    let revolute = hinge(&n, HINGE, JointKind::Revolute, "angle");
    let e = eval(&hinged("0 deg", revolute.clone()));
    // The base at its pose: a quarter turn about z, then (1, 2, 3) mm.
    assert_pose(
        pose_of(&e, ASM, &[BASE]),
        mm(1.0, 2.0, 3.0),
        DVec3::Y,
        DVec3::Z,
    );
    // At 0 the shaft sits in the bore: the arm is the base moved by the
    // bore's offset from the shaft, (22, 0, 0) mm in the base's frame.
    assert_pose(
        pose_of(&e, ASM, &[ARM]),
        mm(1.0, 24.0, 3.0),
        DVec3::Y,
        DVec3::Z,
    );
    // A quarter turn about the bore: (-10, -10, -36) turned to (10, -10,
    // -36), plus the bore's (32, 10, 36), is (42, 0, 0) in the base.
    let turned = eval(&hinged("90 deg", revolute.clone()));
    assert_pose(
        pose_of(&turned, ASM, &[ARM]),
        mm(1.0, 44.0, 3.0),
        -DVec3::X,
        DVec3::Z,
    );
    let JointOutcome::Ok {
        value,
        parent,
        child,
        diagnostics,
    } = &outcome(&turned, ASM).joints[&HINGE]
    else {
        panic!("{:?}", outcome(&turned, ASM).joints)
    };
    assert!((value - FRAC_PI_2).abs() < 1e-15 && diagnostics.is_empty());
    assert!(near(parent.origin(), child.origin()) && near(child.x_axis(), -DVec3::X));

    // A fixed joint ignores its value; a prismatic one slides along z.
    let fixed = hinge(&n, HINGE, JointKind::Fixed, "angle");
    assert_pose(arm("90 deg", fixed), mm(1.0, 24.0, 3.0), DVec3::Y, DVec3::Z);
    let prismatic = hinge(&n, HINGE, JointKind::Prismatic, "5 mm");
    assert_pose(
        arm("0 deg", prismatic),
        mm(1.0, 24.0, 8.0),
        DVec3::Y,
        DVec3::Z,
    );

    // Flipped, the shaft's frame turns half about its x first: (-10, -10,
    // -36) becomes (-10, 10, 36), so the arm's origin is (22, 20, 72) mm in
    // the base, upside down.
    let flipped = Joint {
        flip: true,
        ..revolute
    };
    assert_pose(
        arm("0 deg", flipped.clone()),
        mm(-19.0, 24.0, 75.0),
        DVec3::Y,
        -DVec3::Z,
    );
    // And a quarter turn on: (22, 0, 72) in the base.
    assert_pose(
        arm("90 deg", flipped),
        mm(1.0, 24.0, 75.0),
        -DVec3::X,
        -DVec3::Z,
    );
}

#[test]
fn nested_assemblies_compose_two_deep() {
    let n = names();
    const M1: ComponentId = ComponentId(Id(33));
    const O1: ComponentId = ComponentId(Id(34));
    const O2: ComponentId = ComponentId(Id(35));
    let lift = |c, of, z: &str| {
        let at = pose(["0 mm", "0 mm", z], ["0 deg", "0 deg", "0 deg"]);
        component(c, "lifted", ComponentOf::Assembly(of), Some(at))
    };
    let arm = component(ARM, "arm", ComponentOf::Part(SERVO), None);
    let inner = assembly(
        ASM,
        vec![base(), arm],
        vec![hinge(&n, HINGE, JointKind::Revolute, "angle")],
    );
    let mid = assembly(MID, vec![lift(M1, ASM, "10 mm")], vec![]);
    // A servo in the outer assembly, fixed in the innermost base's bore.
    let spare = component(O2, "spare", ComponentOf::Part(SERVO), None);
    let fixed = Joint {
        parent: anchor(vec![O1, M1, BASE], &n.bore, &n.top),
        child: anchor(vec![O2], &n.shaft, &n.top),
        ..hinge(&n, SECOND, JointKind::Fixed, "0 deg")
    };
    let outer = assembly(OUTER, vec![lift(O1, MID, "100 mm"), spare], vec![fixed]);
    let e = eval(&with_assemblies(
        servo_doc("0 deg"),
        vec![inner, mid, outer],
    ));
    assert_pose(
        pose_of(&e, MID, &[M1, BASE]),
        mm(1.0, 2.0, 13.0),
        DVec3::Y,
        DVec3::Z,
    );
    let deep = pose_of(&e, OUTER, &[O1, M1, ARM]);
    assert_pose(deep, mm(1.0, 24.0, 113.0), DVec3::Y, DVec3::Z);
    // The spare's shaft in the same bore puts it where the arm is.
    assert_pose(
        pose_of(&e, OUTER, &[O2]),
        mm(1.0, 24.0, 113.0),
        DVec3::Y,
        DVec3::Z,
    );
    let paths: Vec<_> = outcome(&e, OUTER)
        .components
        .iter()
        .map(|c| c.path.clone())
        .collect();
    assert_eq!(
        paths,
        [
            vec![O1],
            vec![O1, M1],
            vec![O1, M1, BASE],
            vec![O1, M1, ARM],
            vec![O2]
        ]
    );
}

#[test]
fn loops_and_missing_placements_are_diagnostics_and_the_rest_stands() {
    let n = names();
    let arm = component(ARM, "arm", ComponentOf::Part(SERVO), None);
    let spare = component(SPARE, "spare", ComponentOf::Part(SERVO), None);
    let onto_base = Joint {
        child: anchor(vec![BASE], &n.shaft, &n.top),
        parent: anchor(vec![ARM], &n.bore, &n.top),
        ..hinge(&n, JointId(Id(42)), JointKind::Fixed, "0 deg")
    };
    let a = assembly(
        ASM,
        vec![base(), arm.clone(), spare.clone()],
        vec![
            hinge(&n, HINGE, JointKind::Revolute, "0 deg"),
            hinge(&n, SECOND, JointKind::Revolute, "90 deg"),
            onto_base,
        ],
    );
    let e = eval(&with_assemblies(servo_doc("0 deg"), vec![a]));
    let o = outcome(&e, ASM);
    assert_eq!(joint_code(o, HINGE), "");
    assert_eq!(joint_code(o, SECOND), "assembly.loop");
    assert_eq!(joint_code(o, JointId(Id(42))), "assembly.loop");
    assert_pose(
        pose_of(&e, ASM, &[ARM]),
        mm(1.0, 24.0, 3.0),
        DVec3::Y,
        DVec3::Z,
    );
    assert_pose(
        pose_of(&e, ASM, &[BASE]),
        mm(1.0, 2.0, 3.0),
        DVec3::Y,
        DVec3::Z,
    );
    assert_eq!(codes(o, &[SPARE]), ["assembly.unplaced"]);
    assert_pose(pose_of(&e, ASM, &[SPARE]), DVec3::ZERO, DVec3::X, DVec3::Z);
    assert!(codes(o, &[ARM]).is_empty() && codes(o, &[BASE]).is_empty());

    // Two joints placing each other: the later is the loop, its child sits
    // at the origin and the earlier places the arm from there.
    let back = Joint {
        parent: anchor(vec![ARM], &n.bore, &n.top),
        child: anchor(vec![SPARE], &n.shaft, &n.top),
        ..hinge(&n, SECOND, JointKind::Fixed, "0 deg")
    };
    let forth = Joint {
        parent: anchor(vec![SPARE], &n.bore, &n.top),
        ..hinge(&n, HINGE, JointKind::Fixed, "0 deg")
    };
    let a = assembly(ASM, vec![arm, spare], vec![forth, back]);
    let e = eval(&with_assemblies(servo_doc("0 deg"), vec![a]));
    let o = outcome(&e, ASM);
    assert_eq!(joint_code(o, SECOND), "assembly.loop");
    assert_eq!(codes(o, &[SPARE]), ["assembly.unplaced"]);
    assert_pose(
        pose_of(&e, ASM, &[ARM]),
        mm(22.0, 0.0, 0.0),
        DVec3::X,
        DVec3::Z,
    );
}

#[test]
fn a_value_past_its_limits_is_applied_and_said() {
    let n = names();
    let joint = Joint {
        limits: Some([e("-45 deg"), e("45 deg")]),
        ..hinge(&n, HINGE, JointKind::Revolute, "angle")
    };
    let e = eval(&hinged("90 deg", joint));
    assert_eq!(joint_code(outcome(&e, ASM), HINGE), "assembly.limit");
    assert_pose(
        pose_of(&e, ASM, &[ARM]),
        mm(1.0, 44.0, 3.0),
        -DVec3::X,
        DVec3::Z,
    );
}

#[test]
fn a_lost_anchor_is_ref_lost_with_candidates_and_its_child_unplaced() {
    let n = names();
    let lost = PersistentName {
        kind: TopoKind::Face,
        root: NameRoot::Imported {
            feature: IMPORT,
            entity: 999_999,
            instance: 0,
        },
        chain: vec![],
    };
    let mut joint = hinge(&n, HINGE, JointKind::Revolute, "0 deg");
    joint.child.axis = lost;
    let e = eval(&hinged("0 deg", joint.clone()));
    let o = outcome(&e, ASM);
    let JointOutcome::Failed { diagnostic } = &o.joints[&HINGE] else {
        panic!("{:?}", o.joints)
    };
    assert_eq!(diagnostic.code.as_str(), "ref.lost", "{diagnostic:?}");
    assert!(!diagnostic.candidates.is_empty(), "{diagnostic:?}");
    assert_eq!(codes(o, &[ARM]), ["assembly.unplaced"]);
    assert_pose(pose_of(&e, ASM, &[ARM]), DVec3::ZERO, DVec3::X, DVec3::Z);
    assert_pose(
        pose_of(&e, ASM, &[BASE]),
        mm(1.0, 2.0, 3.0),
        DVec3::Y,
        DVec3::Z,
    );

    joint.child.axis = n.vertex.clone();
    let e = eval(&hinged("0 deg", joint.clone()));
    assert_eq!(joint_code(outcome(&e, ASM), HINGE), "input.wrong-kind");

    // A path that goes on past a part leads nowhere.
    joint.child = anchor(vec![ARM, BASE], &n.shaft, &n.top);
    let e = eval(&hinged("0 deg", joint));
    assert_eq!(joint_code(outcome(&e, ASM), HINGE), "ref.lost");
}

#[test]
fn the_outcome_is_cached_and_a_new_value_calls_no_kernel() {
    let n = names();
    let doc = hinged("0 deg", hinge(&n, HINGE, JointKind::Revolute, "angle"));
    let mut evaluator = Evaluator::new(Registry::with_core_types());
    let first = evaluator.evaluate(&doc);
    assert!(!outcome(&first, ASM).cached);
    let calls = evaluator.kernel_calls().len();
    let again = evaluator.evaluate(&doc);
    assert!(outcome(&again, ASM).cached);
    assert_eq!(
        outcome(&again, ASM).components,
        outcome(&first, ASM).components
    );
    let turned = with(
        &doc,
        Command::SetParam {
            param: ANGLE,
            expr: e("90 deg"),
        },
    );
    let e = evaluator.evaluate(&turned);
    assert!(!outcome(&e, ASM).cached);
    assert_pose(
        pose_of(&e, ASM, &[ARM]),
        mm(1.0, 44.0, 3.0),
        -DVec3::X,
        DVec3::Z,
    );
    assert_eq!(
        evaluator.kernel_calls().len(),
        calls,
        "the frames are cached"
    );
}
