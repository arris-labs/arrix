//! The north star's document (plans/c3-m3 step 14), C3's accept: C1's
//! bracket (the holes patterned, the gear from `gears`) with the servo of
//! C2 mated to its gear by a revolute joint, the horn on the servo by a
//! second one, and a material on every part. It holds no screw: C3's accept
//! places them with `fasteners.place`. Built on `tests/docs/bracket` by the
//! commands the UI issues, as `c2_accept` builds its document; it writes
//! `tests/docs/north-star/` (`UPDATE_SNAPSHOTS=1`).

mod common;

use std::collections::BTreeMap;

use arrix_core::{
    AssemblyId, ComponentId, DVec3, FeatureId, Frame, IdMinter, JointId, MaterialId, MeshDetail,
    PartId, PersistentName,
};
use arrix_doc::{
    Anchor, Assembly, AuthorId, BlobRef, Command, Component, ComponentOf, Document, Evaluator,
    FeatureOutcome, ImportedFile, Joint, JointKind, JointOutcome, Material, MemorySource, Part,
    Pose, Rgb, Session, expr::Expr, open,
};
use common::build::record;
use common::{Client, arrix_eval, golden, read_tree, scenario_directory, workspace};

const MM: f64 = 1e-3;
/// The bracket's plate, and the gear fused on its top (`tests/docs/bracket`).
const BRACKET: &str = "8E4ZYYF80N5EM";
const GEAR: &str = "AFXQH9YAYRG2M";

fn e(text: &str) -> Expr {
    Expr::parse(text).unwrap()
}

fn step_file(name: &str) -> Vec<u8> {
    std::fs::read(workspace().join("tests/fixtures/step").join(name)).unwrap()
}

fn bracket_document() -> Document {
    let mut files = BTreeMap::new();
    read_tree(&workspace().join("tests/docs/bracket"), "", &mut files);
    open(&MemorySource(files)).expect("the bracket opens")
}

struct Ids {
    author: AuthorId,
    bracket: PartId,
    servo: PartId,
    servo_import: FeatureId,
    horn: PartId,
    horn_import: FeatureId,
    assembly: AssemblyId,
    ground: ComponentId,
    servo_component: ComponentId,
    horn_component: ComponentId,
    hinge: JointId,
    spin: JointId,
    aluminium: MaterialId,
    abs: MaterialId,
}

impl Ids {
    fn new() -> Self {
        let mut m = IdMinter::new(2034);
        Ids {
            author: AuthorId(m.next_id()),
            bracket: BRACKET.parse().unwrap(),
            servo: m.mint(),
            servo_import: m.mint(),
            horn: m.mint(),
            horn_import: m.mint(),
            assembly: m.mint(),
            ground: m.mint(),
            servo_component: m.mint(),
            horn_component: m.mint(),
            hinge: m.mint(),
            spin: m.mint(),
            aluminium: m.mint(),
            abs: m.mint(),
        }
    }
}

/// A part of one imported file, in one gesture.
fn import(c: &mut Client, part: PartId, feature: FeatureId, name: &str, file: &str) {
    let bytes = step_file(file);
    let mut import = record(feature, "core.import-step", name);
    import.type_version = 2;
    import.file = Some(ImportedFile {
        name: file.into(),
        blob: BlobRef::of(&bytes, "step"),
    });
    let record_part = Part {
        id: part,
        name: name.into(),
        history: vec![],
        features: BTreeMap::new(),
        rollback: None,
        material: None,
        body_materials: Default::default(),
    };
    c.submit(Command::Group {
        label: format!("Import {file}"),
        commands: vec![
            Command::AddFile { bytes },
            Command::AddPart { part: record_part },
            Command::AddFeature {
                part,
                at: 0,
                record: import,
            },
        ],
    });
}

/// Every face of the part's final body: its name, size and low corner in
/// mm, and the nearest and farthest its vertices lie from the z axis.
struct FaceBox {
    name: PersistentName,
    size: DVec3,
    low: DVec3,
    radii: (f64, f64),
}

fn faces(doc: &Document, part: PartId) -> Vec<FaceBox> {
    let mut evaluator = Evaluator::new(common::registry());
    evaluator.evaluate(doc);
    let finals = evaluator.final_bodies().to_vec();
    let body = finals.iter().find(|b| b.part == part).expect("a body");
    let mesh = evaluator.mesh(body, MeshDetail::Refined).unwrap();
    mesh.faces
        .iter()
        .map(|face| {
            let mut lo = DVec3::splat(f64::MAX);
            let mut hi = DVec3::splat(f64::MIN);
            let mut radii = (f64::MAX, 0.0f64);
            for t in &mesh.triangles[face.range()] {
                for &v in t {
                    let p = mesh.world(mesh.positions[v as usize]);
                    lo = lo.min(p);
                    hi = hi.max(p);
                    let r = p.truncate().length() / MM;
                    radii = (radii.0.min(r), radii.1.max(r));
                }
            }
            FaceBox {
                name: face.name.clone(),
                size: (hi - lo) / MM,
                low: lo / MM,
                radii,
            }
        })
        .collect()
}

/// The one face of that size (and, when given, height).
fn find(faces: &[FaceBox], span: [f64; 3], z: Option<f64>) -> PersistentName {
    let near = |a: f64, b: f64| (a - b).abs() < 0.05;
    let mut hits = faces.iter().filter(|f| {
        near(f.size.x, span[0])
            && near(f.size.y, span[1])
            && near(f.size.z, span[2])
            && z.is_none_or(|z| near(f.low.z, z))
    });
    let hit = hits.next().expect("the face is there").name.clone();
    assert!(hits.next().is_none(), "one face of that shape");
    hit
}

fn anchor(path: ComponentId, axis: PersistentName, origin: PersistentName) -> Anchor {
    Anchor {
        path: vec![path],
        axis,
        origin: Some(origin),
    }
}

fn scenario() -> (Client, Ids) {
    let i = Ids::new();
    let mut c = Client::of_document(i.author, bracket_document(), common::registry());
    import(&mut c, i.servo, i.servo_import, "Servo", "servo.step");
    import(&mut c, i.horn, i.horn_import, "Horn", "horn.step");

    // The gear's axis is that of a tooth tip (a cylinder 11 mm from it),
    // and its top is the gear's end cap; the servo's shaft seats on it as
    // the horn's shaft seats in the servo's bore.
    let doc = c.replica.document();
    let gear = faces(doc, i.bracket);
    let tip = gear
        .iter()
        .filter(|f| (f.size.z - 8.0).abs() < 0.05 && f.radii.0 > 10.99)
        .map(|f| f.name.clone())
        .min()
        .expect("a tooth tip");
    let gear_top: PersistentName = format!("face:sweep.{GEAR}.end-cap").parse().unwrap();
    let servo = faces(doc, i.servo);
    let horn = faces(doc, i.horn);
    let hinge = Joint {
        id: i.hinge,
        name: "hinge".into(),
        kind: JointKind::Revolute,
        parent: anchor(i.ground, tip, gear_top),
        child: anchor(
            i.servo_component,
            find(&servo, [6.0, 6.0, 6.0], None),
            find(&servo, [40.0, 20.0, 0.0], Some(36.0)),
        ),
        value: e("0 deg"),
        offset: e("0 deg"),
        limits: Some([e("-90 deg"), e("90 deg")]),
        flip: true,
    };
    let spin = Joint {
        id: i.spin,
        name: "spin".into(),
        kind: JointKind::Revolute,
        parent: anchor(
            i.servo_component,
            find(&servo, [4.0, 4.0, 36.0], None),
            find(&servo, [40.0, 20.0, 0.0], Some(36.0)),
        ),
        child: anchor(
            i.horn_component,
            find(&horn, [6.0, 6.0, 6.0], None),
            find(&horn, [30.0, 10.0, 0.0], Some(4.0)),
        ),
        value: e("30 deg"),
        offset: e("0 deg"),
        limits: Some([e("-180 deg"), e("180 deg")]),
        flip: false,
    };
    let component = |id, name: &str, part, pose| Component {
        id,
        name: name.into(),
        of: ComponentOf::Part(part),
        pose,
    };
    let assembly = Assembly {
        id: i.assembly,
        name: "Servo on bracket".into(),
        components: [
            (
                i.ground,
                component(
                    i.ground,
                    "bracket",
                    i.bracket,
                    Some(Pose {
                        position: ["0 mm", "0 mm", "0 mm"].map(e),
                        rpy: ["0 deg", "0 deg", "0 deg"].map(e),
                    }),
                ),
            ),
            (
                i.servo_component,
                component(i.servo_component, "servo", i.servo, None),
            ),
            (
                i.horn_component,
                component(i.horn_component, "horn", i.horn, None),
            ),
        ]
        .into(),
        joints: [(i.hinge, hinge), (i.spin, spin)].into(),
    };
    c.submit(Command::AddAssembly { assembly });

    // A material on every part: aluminium for the bracket, ABS for the
    // servo and the horn.
    let material = |id, name: &str, density: &str, colour| Command::AddMaterial {
        material: Material {
            id,
            name: name.into(),
            density: Some(e(density)),
            appearance: Rgb(colour),
        },
    };
    let mut group = vec![
        material(i.aluminium, "aluminium", "2700 kg/m3", [0.75, 0.77, 0.80]),
        material(i.abs, "abs", "1.05 g/cm3", [0.20, 0.25, 0.60]),
    ];
    for (part, material) in [(i.bracket, i.aluminium), (i.servo, i.abs), (i.horn, i.abs)] {
        group.push(Command::AssignMaterial {
            part,
            body: None,
            material: Some(material),
        });
    }
    c.submit(Command::Group {
        label: "Materials".into(),
        commands: group,
    });
    (c, i)
}

fn pose(c: &Client, i: &Ids, component: ComponentId) -> Frame {
    let (_, solved) = c.replica.assembly(i.assembly).expect("solved");
    solved.pose(&[component]).unwrap()
}

#[test]
fn the_north_star_is_what_the_commands_save() {
    let (c, _) = scenario();
    scenario_directory("north-star", &c.session.save());
}

#[test]
fn the_north_star_evaluates_clean_and_holds_together() {
    let (c, i) = scenario();
    for feature in [i.servo_import, i.horn_import] {
        assert!(
            matches!(c.outcome(feature), FeatureOutcome::Ok { .. }),
            "{feature}: {:?}",
            c.outcome(feature)
        );
    }
    let doc = c.replica.document();
    for (id, part) in doc.parts() {
        assert!(part.material.is_some(), "{id} has a material");
        for feature in part.features.keys() {
            assert!(
                matches!(c.outcome(*feature), FeatureOutcome::Ok { .. }),
                "{feature}: {:?}",
                c.outcome(*feature)
            );
        }
    }
    let (_, solved) = c.replica.assembly(i.assembly).unwrap();
    assert!(
        solved.components.iter().all(|p| p.diagnostics.is_empty()),
        "{solved:?}"
    );
    for (joint, outcome) in &solved.joints {
        assert!(
            matches!(outcome, JointOutcome::Ok { .. }),
            "{joint}: {outcome:?}"
        );
    }
    // No screw: the accept places them.
    assert_eq!(doc.parts().len(), 3);

    // The servo stands upside down on the gear's top (z = 13 mm), its
    // shaft down the gear's axis: the shaft's base is the servo's top, 36
    // mm above its origin, which is (-10, 10) mm off the axis. The horn
    // is seated in the servo's bore, turned 30 degrees.
    let near = |a: DVec3, b: DVec3| (a - b).length() < 1e-9;
    let mm = |x: f64, y: f64, z: f64| DVec3::new(x, y, z) * MM;
    let servo = pose(&c, &i, i.servo_component);
    assert!(near(servo.origin(), mm(-10.0, 10.0, 49.0)), "{servo:?}");
    assert!(near(servo.z_axis(), -DVec3::Z), "{servo:?}");
    let horn = pose(&c, &i, i.horn_component);
    assert!(near(horn.z_axis(), -DVec3::Z), "{horn:?}");
    let turn = 30f64.to_radians();
    assert!(
        near(horn.x_axis(), DVec3::new(turn.cos(), -turn.sin(), 0.0)),
        "{horn:?}"
    );
    assert!((horn.origin().z - 0.017).abs() < 1e-9, "{horn:?}");
}

#[test]
fn arrix_eval_prints_the_north_stars_line() {
    let (c, _) = scenario();
    let (code, line) = arrix_eval("north-star", &c.session.save(), &[]);
    assert_eq!(code, Some(0));
    golden("eval_north_star.jsonl", &line);
}
