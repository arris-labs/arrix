//! C2 M1's accept (docs/ROADMAP.md §C2, plans/c2-m1): a bracket (the slice's
//! plate with the gear joined, then a bore cut downstream of it) grounded, a
//! servo imported from `tests/fixtures/step/`, and a revolute joint from the
//! servo's shaft face to the bracket's bore face, its value a parameter.
//! Built by commands alone through `LocalSession` with `gears` compiled in;
//! it writes `tests/docs/assembly/`, the document `arrix eval` and `arrix
//! serve` are checked on.

mod common;

use std::collections::BTreeMap;

use arrix_core::Ref;
use arrix_core::{
    AssemblyId, ComponentId, DVec3, FeatureId, Frame, IdMinter, JointId, MeshDetail, ParamId,
    PartId, PersistentName, QuantityKind, TopoKind,
};
use arrix_doc::{
    Anchor, Assembly, AssemblyOutcome, AuthorId, BlobRef, Command, Component, ComponentOf,
    Document, Evaluator, FeatureOutcome, ImportedFile, Joint, JointKind, JointOutcome, Param, Part,
    Pose, Request, Session, expr::Expr, open, save,
};
use arrix_sketch::{Constraint, Draft, Point, SketchEdit};
use common::build::{extrude, record, rectangle_sketch, slot, with};
use common::serve_client::Serve;
use common::{Client, arrix_eval, golden, scenario_directory, workspace};

const MM: f64 = 1e-3;

/// Every id the scenario's commands create, from the test's fixed seed.
struct Ids {
    author: AuthorId,
    teeth: ParamId,
    m: ParamId,
    w: ParamId,
    t: ParamId,
    angle: ParamId,
    bracket: PartId,
    plate_sketch: FeatureId,
    plate: FeatureId,
    base: FeatureId,
    gear: FeatureId,
    join: FeatureId,
    bore_sketch: FeatureId,
    bore: FeatureId,
    servo: PartId,
    import: FeatureId,
    assembly: AssemblyId,
    ground: ComponentId,
    arm: ComponentId,
    hinge: JointId,
}

impl Ids {
    fn new() -> Self {
        let mut m = IdMinter::new(2027);
        Ids {
            author: AuthorId(m.next_id()),
            teeth: m.mint(),
            m: m.mint(),
            w: m.mint(),
            t: m.mint(),
            angle: m.mint(),
            bracket: m.mint(),
            plate_sketch: m.mint(),
            plate: m.mint(),
            base: m.mint(),
            gear: m.mint(),
            join: m.mint(),
            bore_sketch: m.mint(),
            bore: m.mint(),
            servo: m.mint(),
            import: m.mint(),
            assembly: m.mint(),
            ground: m.mint(),
            arm: m.mint(),
            hinge: m.mint(),
        }
    }

    fn plate_top(&self) -> PersistentName {
        PersistentName {
            kind: TopoKind::Face,
            root: arrix_core::NameRoot::Sweep {
                index: 0,
                feature: self.plate,
                part: arrix_core::SweepPartName::EndCap,
            },
            chain: vec![],
        }
    }

    /// The plate's top face once the join has holed it.
    fn joined_top(&self) -> PersistentName {
        format!("face:sweep.{}.end-cap", self.plate)
            .parse()
            .unwrap()
    }
}

fn servo_step() -> Vec<u8> {
    std::fs::read(workspace().join("tests/fixtures/step/servo.step")).expect("the servo fixture")
}

fn e(text: &str) -> Expr {
    Expr::parse(text).unwrap()
}

/// The bracket: the plate (`w` × 30 × `t` mm), the gear fused on its top,
/// and a 4 mm bore through the plate, its centre `w - 26 mm` along x from
/// the sketch's origin, so the bore follows the plate's width.
fn build_bracket(c: &mut Client, i: &Ids) {
    let params = [
        (i.teeth, "teeth", QuantityKind::Count, "20"),
        (i.m, "m", QuantityKind::Length, "1 mm"),
        (i.w, "w", QuantityKind::Length, "40 mm"),
        (i.t, "t", QuantityKind::Length, "5 mm"),
        (i.angle, "angle", QuantityKind::Angle, "0 deg"),
    ];
    for (param, name, kind, expr) in params {
        let record = Param {
            name: name.into(),
            kind,
            expr: e(expr),
        };
        c.submit(Command::AddParam { param, record });
    }
    let part = Part {
        id: i.bracket,
        name: "Bracket".into(),
        history: vec![],
        features: BTreeMap::new(),
        rollback: None,
        material: None,
        body_materials: Default::default(),
    };
    c.submit(Command::AddPart { part });
    let key = rectangle_sketch(
        c,
        (i.plate_sketch, 0),
        i.bracket,
        None,
        (-20.0, -15.0, 40.0, 30.0),
        (Some("w"), None),
    );
    let mut plate = extrude(i.plate, "Plate", i.plate_sketch, key, "new");
    plate.params.insert("distance".into(), e("t"));
    let base = with(
        record(i.base, "core.datum-plane", "Plate top"),
        &[("offset", "0 mm")],
        &[("plane", Ref::Topo(i.plate_top()))],
    );
    let gear = with(
        record(i.gear, "gears.spur", "Spur gear"),
        &[("teeth", "teeth"), ("module", "m"), ("width", "8 mm")],
        &[("plane", slot(i.base, "plane"))],
    );
    let mut join = with(
        record(i.join, "core.boolean", "Join"),
        &[],
        &[("target", slot(i.plate, "body"))],
    );
    join.type_version = 2;
    join.inputs
        .insert("tools".into(), vec![slot(i.gear, "body")].into());
    join.choices.insert("op".into(), "fuse".into());
    for (at, record) in [(1, plate), (2, base), (3, gear), (4, join)] {
        c.submit(Command::AddFeature {
            part: i.bracket,
            at,
            record,
        });
    }

    // The bore downstream of the join, on the joined top face.
    let mut sketch = record(i.bore_sketch, "core.sketch", "Bore sketch");
    sketch.sketch = Some(Box::default());
    sketch
        .inputs
        .insert("plane".into(), Ref::Topo(i.joined_top()).into());
    c.submit(Command::AddFeature {
        part: i.bracket,
        at: 5,
        record: sketch,
    });
    let mut draft = Draft::seeded(9);
    let origin = draft.add_point(Point::new(0.0, 0.0));
    let centre = draft.add_point(Point::new(14.0 * MM, 0.0));
    draft.point_mut(origin).unwrap().fixed = true;
    let across = draft.add_constraint(Constraint::HorizontalDistance {
        a: origin,
        b: centre,
        value: 14.0 * MM,
    });
    draft.set_constraint_expr(across, Some("w - 26 mm".into()));
    draft.add_constraint(Constraint::VerticalDistance {
        a: origin,
        b: centre,
        value: 0.0,
    });
    assert!(draft.solve().converged);
    let stored = {
        let (_, _, r) = c.replica.document().feature(i.bore_sketch).unwrap();
        r.sketch.as_deref().unwrap().clone()
    };
    let edit = SketchEdit::diff(&stored, &draft);
    c.submit(Command::SketchEdit {
        feature: i.bore_sketch,
        edit,
    });
    let mut bore = with(
        record(i.bore, "core.hole", "Bore"),
        &[("diameter", "4 mm")],
        &[("target", slot(i.plate, "body"))],
    );
    bore.choices.insert("extent".into(), "through".into());
    bore.inputs.insert(
        "points".into(),
        vec![Ref::Sketch {
            feature: i.bore_sketch,
            entity: centre,
        }]
        .into(),
    );
    c.submit(Command::AddFeature {
        part: i.bracket,
        at: 6,
        record: bore,
    });
}

/// The servo, imported in one gesture: the file, its part, its feature.
fn import_servo(c: &mut Client, i: &Ids) {
    let bytes = servo_step();
    let mut import = record(i.import, "core.import-step", "Servo");
    import.file = Some(ImportedFile {
        name: "servo.step".into(),
        blob: BlobRef::of(&bytes, "step"),
    });
    let part = Part {
        id: i.servo,
        name: "Servo".into(),
        history: vec![],
        features: BTreeMap::new(),
        rollback: None,
        material: None,
        body_materials: Default::default(),
    };
    c.submit(Command::Group {
        label: "Import servo.step".into(),
        commands: vec![
            Command::AddFile { bytes },
            Command::AddPart { part },
            Command::AddFeature {
                part: i.servo,
                at: 0,
                record: import,
            },
        ],
    });
}

/// The faces the joint is anchored to, found by their geometry in the final
/// bodies' meshes.
struct Names {
    bore: PersistentName,
    bracket_top: PersistentName,
    shaft: PersistentName,
    servo_top: PersistentName,
}

fn locate(doc: &Document, i: &Ids) -> Names {
    let mut evaluator = Evaluator::new(common::registry());
    evaluator.evaluate(doc);
    let finals = evaluator.final_bodies().to_vec();
    let mut boxes = |part: PartId| {
        let body = finals
            .iter()
            .find(|b| b.part == part)
            .expect("a final body");
        let mesh = evaluator.mesh(body, MeshDetail::Refined).unwrap();
        mesh.faces
            .iter()
            .map(|face| {
                let mut lo = DVec3::splat(f64::MAX);
                let mut hi = DVec3::splat(f64::MIN);
                for t in &mesh.triangles[face.range()] {
                    for &v in t {
                        let p = mesh.world(mesh.positions[v as usize]);
                        lo = lo.min(p);
                        hi = hi.max(p);
                    }
                }
                (face.name.clone(), (hi - lo) / MM, lo / MM)
            })
            .collect::<Vec<_>>()
    };
    let near = |a: f64, b: f64| (a - b).abs() < 0.05;
    let bracket = boxes(i.bracket);
    let servo = boxes(i.servo);
    let find = |faces: &[(PersistentName, DVec3, DVec3)], span: [f64; 3], z: Option<f64>| {
        let mut hits = faces.iter().filter(|(_, s, lo)| {
            near(s.x, span[0])
                && near(s.y, span[1])
                && near(s.z, span[2])
                && z.is_none_or(|z| near(lo.z, z))
        });
        let hit = hits.next().expect("the face is there").0.clone();
        assert!(hits.next().is_none(), "one face of that shape");
        hit
    };
    Names {
        bore: find(&bracket, [4.0, 4.0, 5.0], None),
        bracket_top: i.joined_top(),
        shaft: find(&servo, [6.0, 6.0, 6.0], None),
        servo_top: find(&servo, [40.0, 20.0, 0.0], Some(36.0)),
    }
}

fn anchor(path: ComponentId, axis: &PersistentName, origin: &PersistentName) -> Anchor {
    Anchor {
        path: vec![path],
        axis: axis.clone(),
        origin: Some(origin.clone()),
    }
}

/// The assembly: the bracket grounded at the origin, the servo placed by a
/// revolute joint from its shaft to the bracket's bore, its value `angle`.
fn assemble(c: &mut Client, i: &Ids, n: &Names) {
    let ground = Component {
        id: i.ground,
        name: "bracket".into(),
        of: ComponentOf::Part(i.bracket),
        pose: Some(Pose {
            position: ["0 mm", "0 mm", "0 mm"].map(e),
            rpy: ["0 deg", "0 deg", "0 deg"].map(e),
        }),
    };
    let arm = Component {
        id: i.arm,
        name: "servo".into(),
        of: ComponentOf::Part(i.servo),
        pose: None,
    };
    let hinge = Joint {
        id: i.hinge,
        name: "hinge".into(),
        kind: JointKind::Revolute,
        parent: anchor(i.ground, &n.bore, &n.bracket_top),
        child: anchor(i.arm, &n.shaft, &n.servo_top),
        value: e("angle"),
        offset: e("0 deg"),
        limits: None,
        flip: false,
    };
    let assembly = Assembly {
        id: i.assembly,
        name: "Servo on bracket".into(),
        components: [(i.ground, ground), (i.arm, arm)].into(),
        joints: [(i.hinge, hinge)].into(),
    };
    c.submit(Command::AddAssembly { assembly });
}

fn scenario() -> (Client, Ids, Names) {
    let i = Ids::new();
    let mut c = Client::new(i.author);
    build_bracket(&mut c, &i);
    import_servo(&mut c, &i);
    let n = locate(c.replica.document(), &i);
    assemble(&mut c, &i, &n);
    (c, i, n)
}

fn solved<'a>(c: &'a Client, i: &Ids) -> &'a AssemblyOutcome {
    let (g, o) = c
        .replica
        .assembly(i.assembly)
        .expect("the assembly is solved");
    assert_eq!(g, c.replica.generation(), "of the latest generation");
    o
}

fn pose(c: &Client, i: &Ids, component: ComponentId) -> Frame {
    solved(c, i).pose(&[component]).unwrap()
}

fn mm(x: f64, y: f64, z: f64) -> DVec3 {
    DVec3::new(x, y, z) * MM
}

#[track_caller]
fn assert_pose(f: Frame, origin: DVec3, x: DVec3, z: DVec3) {
    let near = |a: DVec3, b: DVec3| (a - b).length() < 1e-9;
    assert!(
        near(f.origin(), origin) && near(f.x_axis(), x) && near(f.z_axis(), z),
        "{f:?}, expected at {origin} with x {x} and z {z}"
    );
}

/// Every feature of the bracket evaluated.
fn all_ok(c: &Client, i: &Ids) {
    for f in [
        i.plate_sketch,
        i.plate,
        i.base,
        i.gear,
        i.join,
        i.bore_sketch,
        i.bore,
        i.import,
    ] {
        assert!(
            matches!(c.outcome(f), FeatureOutcome::Ok { .. }),
            "{f}: {:?}",
            c.outcome(f)
        );
    }
}

#[test]
fn the_servo_follows_its_joint_and_the_plates_width() {
    let (mut c, i, _) = scenario();
    let built = c.replica.generation();
    let scenario = c.saves[built.0 as usize].clone();
    all_ok(&c, &i);

    // The bore is at (14, 0) mm on the plate's top, z = 5, its axis the
    // hole's inward normal (-z); the shaft is at (10, 10) mm on the servo's
    // top, z = 36, its axis +z. At 0 the shaft sits in the bore, so the
    // servo is the bracket's frame turned half about x, its (-10, -10, -36)
    // mm offset to (-10, +10, +36), plus (14, 0, 5): (4, 10, 41) mm, upside
    // down, shaft down the bore.
    assert_pose(pose(&c, &i, i.ground), DVec3::ZERO, DVec3::X, DVec3::Z);
    assert_pose(
        pose(&c, &i, i.arm),
        mm(4.0, 10.0, 41.0),
        DVec3::X,
        -DVec3::Z,
    );

    // A quarter turn about the bore: the offset (-10, -10, -36) turns to
    // (10, -10, -36), then to (10, 10, 36) in the bracket, plus (14, 0, 5).
    c.set(i.angle, "90 deg");
    assert_pose(
        pose(&c, &i, i.arm),
        mm(24.0, 10.0, 41.0),
        -DVec3::Y,
        -DVec3::Z,
    );
    c.set(i.angle, "0 deg");

    // The width: the bore is `w - 26 mm` along x, so +10 mm of plate moves
    // the bore, and the servo with it, +10 mm.
    c.set(i.w, "50 mm");
    all_ok(&c, &i);
    assert_pose(
        pose(&c, &i, i.arm),
        mm(14.0, 10.0, 41.0),
        DVec3::X,
        -DVec3::Z,
    );
    c.set(i.w, "40 mm");
    assert_pose(
        pose(&c, &i, i.arm),
        mm(4.0, 10.0, 41.0),
        DVec3::X,
        -DVec3::Z,
    );

    // Every generation announced the assembly (then its masses, before its Finished).
    let mut announced = 0;
    for triple in c.heard.windows(3) {
        if let [
            arrix_doc::SessionEvent::Assembled { generation: a, .. },
            arrix_doc::SessionEvent::Massed { generation: m, .. },
            arrix_doc::SessionEvent::Finished { generation: f, .. },
        ] = triple
        {
            assert_eq!(a, m);
            assert_eq!(a, f);
            announced += 1;
        }
    }
    assert_eq!(announced, 5, "built, turned, back, widened, narrowed");

    // Saves are deterministic and round-trip, as a directory and as an
    // `.arrx`; undo walks the saved bytes back.
    let last = c.replica.generation();
    assert_eq!(save(c.replica.document()).0, c.saves[last.0 as usize].0);
    let reopened: Document = open(&c.saves[last.0 as usize]).unwrap();
    assert_eq!(save(&reopened).0, c.saves[last.0 as usize].0);
    let zip = arrix_doc::save_zip(&reopened).unwrap();
    assert_eq!(zip, arrix_doc::save_zip(&reopened).unwrap());
    let from_zip = arrix_doc::open_zip(&zip).unwrap();
    assert_eq!(arrix_doc::save_zip(&from_zip).unwrap(), zip);
    assert_eq!(arrix_doc::from_zip(&zip).unwrap().0, save(&reopened).0);
    for back in (built.0..last.0).rev() {
        c.request(Request::Undo { author: i.author });
        assert!(
            save(c.replica.document()).0 == c.saves[back as usize].0,
            "undone to generation {back}"
        );
    }

    scenario_directory("assembly", &scenario);
}

#[test]
fn a_deleted_bore_leaves_the_joint_lost_with_candidates_and_the_rest_standing() {
    let (mut c, i, _) = scenario();
    let before = c.outcome(i.join).clone();
    c.submit(Command::DeleteFeature { feature: i.bore });

    let o = solved(&c, &i);
    let JointOutcome::Failed { diagnostic } = &o.joints[&i.hinge] else {
        panic!("{:?}", o.joints)
    };
    assert_eq!(diagnostic.code.as_str(), "ref.lost", "{diagnostic:?}");
    assert!(!diagnostic.candidates.is_empty(), "{diagnostic:?}");
    let arm = o.components.iter().find(|p| p.path == [i.arm]).unwrap();
    let codes: Vec<&str> = arm.diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, ["assembly.unplaced"]);
    assert_pose(pose(&c, &i, i.arm), DVec3::ZERO, DVec3::X, DVec3::Z);
    assert_pose(pose(&c, &i, i.ground), DVec3::ZERO, DVec3::X, DVec3::Z);

    // Every other result stands.
    for f in [i.plate, i.gear, i.join, i.import] {
        assert!(matches!(c.outcome(f), FeatureOutcome::Ok { .. }), "{f}");
    }
    assert_eq!(c.outcome(i.join), &before);
}

fn assemblies(stdout: &str) -> serde_json::Value {
    let line: serde_json::Value = serde_json::from_str(stdout.lines().next().unwrap()).unwrap();
    line["assemblies"].clone()
}

#[test]
fn arrix_eval_without_the_gears_plugin_gives_the_same_poses() {
    let (c, _, _) = scenario();
    let saved = c.session.save();

    let (code, live) = arrix_eval("assembly-live", &saved, &[]);
    assert_eq!(code, Some(0));
    golden("eval_assembly_live.jsonl", &live);

    // The gear frozen and saying why; the poses unchanged (criterion 3).
    let (code, without) = arrix_eval("assembly-without", &saved, &["--without-plugin", "gears"]);
    assert_eq!(code, Some(0));
    golden("eval_assembly_without_gears.jsonl", &without);
    assert!(
        without.contains(r#""category":"plugin.frozen.missing""#),
        "{without}"
    );
    assert_eq!(assemblies(&without), assemblies(&live));
    let poses = assemblies(&live);
    let arm = &poses[0]["components"][1]["pose"]["origin"];
    assert_eq!(arm, &serde_json::json!([0.004, 0.01, 0.041]));
}

#[test]
fn serve_announces_the_same_outcome_as_an_assembled_event() {
    let (c, _, _) = scenario();
    let built = c.saves.last().unwrap();
    let (_, live) = arrix_eval("assembly-serve", built, &[]);
    let line = assemblies(&live);

    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("eval-scratch")
        .join("assembly-serve");
    let mut s = Serve::spawn_with(&[dir.to_str().unwrap()]);
    let hello = s.call("initialize", serde_json::json!({"protocol": 1}));
    assert_eq!(hello["result"]["protocol"], 1, "{hello}");
    let event = s.hear_event(|e| e["kind"] == "assembled" && e["generation"] == 0);
    assert_eq!(event["assembly"], line[0]["id"]);
    assert_eq!(event["outcome"]["components"], line[0]["components"]);
    assert_eq!(event["outcome"]["joints"], line[0]["joints"]);
}
