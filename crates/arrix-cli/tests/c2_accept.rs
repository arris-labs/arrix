//! C2's accept (docs/ROADMAP.md §C2, plans/c2-m3 step 11): the bracket and
//! servo of M1, a horn on a second revolute joint, a screw used twice, and
//! one vendor part (NIST's FTC 11), a material on each. Built by commands
//! on `tests/docs/assembly`; it writes `tests/docs/assembly-exchange/`.
//!
//! `arrix export --assembly` writes it as a STEP product tree in metres and
//! `arrix import` reads that back: the same parts and components by name
//! (but for the component names, which STEP does not carry), the screw one
//! part used twice, the poses to 1e-12 m, the volumes to 1e-12 relative,
//! the masses the densities times those volumes, and the colours.

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Output;

use arrix_core::{
    AssemblyId, ComponentId, DVec3, FeatureId, IdMinter, JointId, MaterialId, MeshDetail, PartId,
    PersistentName,
};
use arrix_doc::{
    Anchor, AuthorId, BlobRef, Command, Component, ComponentOf, Document, Evaluator,
    FeatureOutcome, ImportedFile, Joint, JointKind, Material, MemorySource, Part, Pose, Rgb,
    Session, expr::Expr, open,
};
use common::build::record;
use common::{Client, arrix_eval, golden, read_tree, scenario_directory, workspace};
use serde_json::Value;

const MM: f64 = 1e-3;

fn e(text: &str) -> Expr {
    Expr::parse(text).unwrap()
}

fn step_file(name: &str) -> Vec<u8> {
    std::fs::read(workspace().join("tests/fixtures/step").join(name)).unwrap()
}

/// The M1 document, whose ids the accept builds on.
fn base() -> Document {
    let mut files = BTreeMap::new();
    read_tree(&workspace().join("tests/docs/assembly"), "", &mut files);
    open(&MemorySource(files)).unwrap()
}

struct Ids {
    author: AuthorId,
    assembly: AssemblyId,
    bracket: PartId,
    servo: PartId,
    arm: ComponentId,
    horn: PartId,
    horn_import: FeatureId,
    screw: PartId,
    screw_import: FeatureId,
    vendor: PartId,
    vendor_import: FeatureId,
    horn_component: ComponentId,
    left: ComponentId,
    right: ComponentId,
    vendor_component: ComponentId,
    spin: JointId,
    materials: [MaterialId; 4],
}

fn ids(doc: &Document) -> Ids {
    let part = |name: &str| {
        doc.parts()
            .values()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("part {name}"))
            .id
    };
    let assembly = doc.assemblies().values().next().unwrap();
    let component = |name: &str| {
        assembly
            .components
            .values()
            .find(|c| c.name == name)
            .unwrap()
            .id
    };
    let mut m = IdMinter::new(2031);
    Ids {
        author: AuthorId(m.next_id()),
        assembly: assembly.id,
        bracket: part("Bracket"),
        servo: part("Servo"),
        arm: component("servo"),
        horn: m.mint(),
        horn_import: m.mint(),
        screw: m.mint(),
        screw_import: m.mint(),
        vendor: m.mint(),
        vendor_import: m.mint(),
        horn_component: m.mint(),
        left: m.mint(),
        right: m.mint(),
        vendor_component: m.mint(),
        spin: m.mint(),
        materials: [m.mint(), m.mint(), m.mint(), m.mint()],
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
    let id = part;
    let part = Part {
        id,
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
            Command::AddPart { part },
            Command::AddFeature {
                part: id,
                at: 0,
                record: import,
            },
        ],
    });
}

/// Every face of the part's final body: its name, size and low corner, in mm.
fn faces(doc: &Document, part: PartId) -> Vec<(PersistentName, DVec3, DVec3)> {
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
            for t in &mesh.triangles[face.range()] {
                for &v in t {
                    let p = mesh.world(mesh.positions[v as usize]);
                    lo = lo.min(p);
                    hi = hi.max(p);
                }
            }
            (face.name.clone(), (hi - lo) / MM, lo / MM)
        })
        .collect()
}

/// The one face of that size (and, when given, height).
fn find(
    faces: &[(PersistentName, DVec3, DVec3)],
    span: [f64; 3],
    z: Option<f64>,
) -> PersistentName {
    let near = |a: f64, b: f64| (a - b).abs() < 0.05;
    let mut hits = faces.iter().filter(|(_, s, lo)| {
        near(s.x, span[0])
            && near(s.y, span[1])
            && near(s.z, span[2])
            && z.is_none_or(|z| near(lo.z, z))
    });
    let hit = hits.next().expect("the face is there").0.clone();
    assert!(hits.next().is_none(), "one face of that shape");
    hit
}

fn pose(x: &str, y: &str, z: &str, yaw: &str) -> Pose {
    Pose {
        position: [x, y, z].map(e),
        rpy: ["0 deg", "0 deg", yaw].map(e),
    }
}

fn scenario() -> (Client, Ids) {
    let doc = base();
    let i = ids(&doc);
    let mut c = Client::of_document(i.author, doc, common::registry());
    import(&mut c, i.horn, i.horn_import, "Horn", "horn.step");
    import(&mut c, i.screw, i.screw_import, "Screw", "screw.step");
    import(
        &mut c,
        i.vendor,
        i.vendor_import,
        "Vendor",
        "nist-ftc-11.stp",
    );

    // The horn's shaft seats in the servo's bore, as the servo's seats in
    // the bracket's.
    let servo = faces(c.replica.document(), i.servo);
    let horn = faces(c.replica.document(), i.horn);
    let anchor = |path, axis, origin| Anchor {
        path: vec![path],
        axis,
        origin: Some(origin),
    };
    let spin = Joint {
        id: i.spin,
        name: "spin".into(),
        kind: JointKind::Revolute,
        parent: anchor(
            i.arm,
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
        limits: None,
        flip: false,
    };
    let component = |id, name: &str, of, pose| Component {
        id,
        name: name.into(),
        of,
        pose,
    };
    let screw = |id, name: &str, y: &str| {
        component(
            id,
            name,
            ComponentOf::Part(i.screw),
            Some(pose("-15 mm", y, "5 mm", "0 deg")),
        )
    };
    let adds = [
        component(i.horn_component, "horn", ComponentOf::Part(i.horn), None),
        screw(i.left, "left screw", "10 mm"),
        screw(i.right, "right screw", "-10 mm"),
        component(
            i.vendor_component,
            "vendor",
            ComponentOf::Part(i.vendor),
            Some(pose("60 mm", "0 mm", "0 mm", "90 deg")),
        ),
    ];
    let mut commands: Vec<Command> = adds
        .into_iter()
        .map(|component| Command::AddComponent {
            assembly: i.assembly,
            component: Box::new(component),
        })
        .collect();
    commands.push(Command::AddJoint {
        assembly: i.assembly,
        joint: Box::new(spin),
    });
    c.submit(Command::Group {
        label: "Add the horn, screws and vendor part".into(),
        commands,
    });

    // A material on each part: aluminium, ABS (servo and horn), steel, brass.
    let materials = [
        ("aluminium", "2700 kg/m3", [0.75, 0.77, 0.80]),
        ("abs", "1.05 g/cm3", [0.20, 0.25, 0.60]),
        ("steel", "7850 kg/m3", [0.45, 0.45, 0.50]),
        ("brass", "8.5 g/cm3", [0.80, 0.65, 0.20]),
    ];
    let mut group = Vec::new();
    for (id, (name, density, colour)) in i.materials.iter().zip(materials) {
        group.push(Command::AddMaterial {
            material: Material {
                id: *id,
                name: name.into(),
                density: Some(e(density)),
                appearance: Rgb(colour),
            },
        });
    }
    let [aluminium, abs, steel, brass] = i.materials;
    for (part, material) in [
        (i.bracket, aluminium),
        (i.servo, abs),
        (i.horn, abs),
        (i.screw, steel),
        (i.vendor, brass),
    ] {
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

fn run(args: &[&std::ffi::OsStr]) -> Output {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_arrix"))
        .current_dir(workspace())
        .args(args)
        .output()
        .expect("run arrix");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

fn eval(doc: &Path) -> Value {
    serde_json::from_slice(&run(&["eval".as_ref(), doc.as_os_str()]).stdout).unwrap()
}

fn numbers(v: &Value) -> Vec<f64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_f64().unwrap())
        .collect()
}

/// The components' poses of the document's one assembly, as twelve numbers
/// each, in a fixed order (the two documents' component ids differ).
fn poses(line: &Value) -> Vec<Vec<f64>> {
    let assemblies = line["assemblies"].as_array().unwrap();
    assert_eq!(assemblies.len(), 1);
    let mut all: Vec<Vec<f64>> = assemblies[0]["components"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            ["origin", "x_axis", "z_axis"]
                .iter()
                .flat_map(|k| numbers(&c["pose"][k]))
                .collect()
        })
        .collect();
    // Quantised, so a rounding difference cannot reorder the two lists.
    all.sort_by_key(|p| {
        p.iter()
            .map(|x| (x * 1e6).round() as i64)
            .collect::<Vec<_>>()
    });
    all
}

/// The final bodies' volumes, ascending.
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

/// A saved document's parts: name to id.
fn part_ids(dir: &Path) -> BTreeMap<String, String> {
    std::fs::read_dir(dir.join("parts"))
        .unwrap()
        .map(|f| {
            let json: Value =
                serde_json::from_slice(&std::fs::read(f.unwrap().path()).unwrap()).unwrap();
            (
                json["name"].as_str().unwrap().to_owned(),
                json["id"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn part_names(dir: &Path) -> Vec<String> {
    part_ids(dir).into_keys().collect()
}

/// The accept document exported as `exchange.step` and imported again.
fn exchange(source: &Path) -> (PathBuf, PathBuf) {
    let file = scratch("exchange").with_extension("step");
    run(&[
        "export".as_ref(),
        source.as_os_str(),
        "--format".as_ref(),
        "step".as_ref(),
        "--assembly".as_ref(),
        "Servo on bracket".as_ref(),
        "-o".as_ref(),
        file.as_os_str(),
    ]);
    let back = scratch("exchange");
    run(&[
        "import".as_ref(),
        file.as_os_str(),
        "-o".as_ref(),
        back.as_os_str(),
    ]);
    (file, back)
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("c2-accept")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.parent().unwrap()).unwrap();
    dir
}

#[test]
fn the_accept_document_is_built_and_evaluates_clean() {
    let (c, i) = scenario();
    for feature in [i.horn_import, i.screw_import, i.vendor_import] {
        assert!(
            matches!(c.outcome(feature), FeatureOutcome::Ok { .. }),
            "{feature}: {:?}",
            c.outcome(feature)
        );
    }
    let (_, solved) = c.replica.assembly(i.assembly).unwrap();
    assert!(
        solved.components.iter().all(|p| p.diagnostics.is_empty()),
        "{solved:?}"
    );
    let saved = c.session.save();
    scenario_directory("assembly-exchange", &saved);
    let (code, line) = arrix_eval("assembly-exchange", &saved, &[]);
    assert_eq!(code, Some(0));
    golden("eval_c2_accept.jsonl", &line);
}

#[test]
fn the_assembly_leaves_as_step_in_metres_and_reads_back_the_same() {
    let (c, _) = scenario();
    scenario_directory("assembly-exchange", &c.session.save());
    let source = workspace().join("tests/docs/assembly-exchange");

    let (file, back) = exchange(&source);
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("SI_UNIT($,.METRE.)"));
    // Five parts, six occurrences: the screw is one product placed twice.
    assert_eq!(text.matches("NEXT_ASSEMBLY_USAGE_OCCURRENCE(").count(), 6);
    assert_eq!(text.matches("MANIFOLD_SOLID_BREP(").count(), 5);

    // Structure and names.
    assert_eq!(
        part_names(&back),
        ["Bracket", "Horn", "Screw", "Servo", "Vendor"]
    );
    let was = eval(&source);
    let now = eval(&back);
    assert_eq!(now["status"], "ok");
    assert_eq!(now["assemblies"][0]["name"], "Servo on bracket");
    assert_eq!(
        now["assemblies"][0]["components"].as_array().unwrap().len(),
        6
    );

    // Poses to 1e-12 m, volumes to 1e-12 relative.
    let (a, b) = (poses(&was), poses(&now));
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(&b) {
        for (p, q) in x.iter().zip(y) {
            assert!((p - q).abs() <= 1e-12, "{x:?} vs {y:?}");
        }
    }
    let (a, b) = (volumes(&was), volumes(&now));
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(&b) {
        assert!((x - y).abs() <= 1e-12 * x.abs(), "{x} vs {y}");
    }

    // The mass is the densities times the volumes the file carries back,
    // the screw counted twice.
    let by_name = part_ids(&back);
    let density = [
        ("Bracket", 2700.0, 1.0),
        ("Servo", 1050.0, 1.0),
        ("Horn", 1050.0, 1.0),
        ("Screw", 7850.0, 2.0),
        ("Vendor", 8500.0, 1.0),
    ];
    let mut want = 0.0;
    for (name, rho, uses) in density {
        let part = now["mass"]["parts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == by_name[name])
            .unwrap();
        want += rho * uses * part["volume"].as_f64().unwrap();
    }
    let mass = was["mass"]["assemblies"][0]["mass"]["mass"]
        .as_f64()
        .unwrap();
    assert!((mass - want).abs() <= 1e-9 * want, "{mass} vs {want}");
    assert_eq!(was["mass"]["assemblies"][0]["partial"], false);

    // Colours: a material per distinct appearance.
    let mut colours: Vec<Vec<f64>> = now["materials"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| numbers(&m["appearance"]))
        .collect();
    colours.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut given: Vec<Vec<f64>> = was["materials"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| numbers(&m["appearance"]))
        .collect();
    given.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(colours.len(), given.len());
    for (x, y) in colours.iter().zip(&given) {
        for (p, q) in x.iter().zip(y) {
            assert!((p - q).abs() < 1e-9, "{x:?} vs {y:?}");
        }
    }
}

// Open CASCADE's reading, frozen by `scripts/occt-read` (never run in CI).

/// A transform as a 3x4 row-major matrix.
type Matrix = [f64; 12];

const IDENTITY: Matrix = [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0.];

fn compose(outer: &Matrix, inner: &Matrix) -> Matrix {
    let mut out = [0.0; 12];
    for r in 0..3 {
        for c in 0..4 {
            out[r * 4 + c] = (0..3)
                .map(|k| outer[r * 4 + k] * inner[k * 4 + c])
                .sum::<f64>()
                + if c == 3 { outer[r * 4 + 3] } else { 0.0 };
        }
    }
    out
}

fn apply(m: &Matrix, p: [f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|r| (0..3).map(|k| m[r * 4 + k] * p[k]).sum::<f64>() + m[r * 4 + 3])
}

/// One occurrence as either reader tells it.
#[derive(Debug)]
struct Occurrence {
    depth: usize,
    name: String,
    placement: Option<Matrix>,
    colour: Option<[f64; 3]>,
}

/// One leaf part as either reader tells it: names from the root, its
/// volume and its centroid at its composed placement.
#[derive(Debug)]
struct Instance {
    path: String,
    volume: f64,
    centroid: [f64; 3],
}

fn floats<const N: usize>(text: &str) -> Option<[f64; N]> {
    if text == "-" {
        return None;
    }
    let v: Vec<f64> = text.split(' ').map(|n| n.parse().unwrap()).collect();
    Some(v.try_into().unwrap())
}

/// The golden's section for `file`.
fn occt(file: &str) -> (Vec<Occurrence>, Vec<Instance>) {
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/c2-accept.occt.txt"),
    )
    .unwrap();
    let mut lines = text
        .lines()
        .skip_while(|l| *l != format!("file {file}"))
        .skip(1)
        .take_while(|l| !l.starts_with("file "));
    let (mut occurrences, mut instances) = (Vec::new(), Vec::new());
    for line in lines.by_ref() {
        if let Some(rest) = line.strip_prefix("occ ") {
            let (depth, rest) = rest.split_once(' ').unwrap();
            let (name, rest) = rest.rsplit_once(" placement=").unwrap();
            let (placement, colour) = rest.split_once(" colour=").unwrap();
            occurrences.push(Occurrence {
                depth: depth.parse().unwrap(),
                name: name.into(),
                placement: floats(placement),
                colour: floats(colour),
            });
        } else if let Some(rest) = line.strip_prefix("inst ") {
            let (path, rest) = rest.rsplit_once(" volume=").unwrap();
            let (volume, centroid) = rest.split_once(" centroid=").unwrap();
            instances.push(Instance {
                path: path.into(),
                volume: volume.parse().unwrap(),
                centroid: floats(centroid).unwrap(),
            });
        }
    }
    assert!(!occurrences.is_empty(), "no section for {file}");
    (occurrences, instances)
}

/// ArriX's reading of `file`: the tree of `read_step_tree`, and the
/// volume and centroid of each part from the document `arrix import`
/// makes of it (`back`), at the placement each path composes.
fn arrix(file: &Path, back: &Path) -> (Vec<Occurrence>, Vec<Instance>) {
    let tree = arrix_doc::read_step_tree(&std::fs::read(file).unwrap()).unwrap();
    let line = eval(back);
    let ids = part_ids(back);
    let part = |name: &str| -> (f64, [f64; 3]) {
        // A file of one part names its part for the file, not the product.
        let id = ids
            .get(name)
            .or_else(|| ids.values().next().filter(|_| ids.len() == 1));
        let id = id.unwrap_or_else(|| panic!("no part {name}"));
        let volume = line["mass"]["parts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == *id)
            .unwrap()["volume"]
            .as_f64()
            .unwrap();
        let bodies: Vec<&Value> = line["mass"]["bodies"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|b| b["part"] == *id)
            .collect();
        assert_eq!(bodies.len(), 1, "{name}: one body");
        let centroid: Vec<f64> = numbers(&bodies[0]["centroid"]);
        (volume, [centroid[0], centroid[1], centroid[2]])
    };
    fn walk(
        node: &arrix_doc::StepNode,
        depth: usize,
        world: &Matrix,
        path: &mut Vec<String>,
        part: &dyn Fn(&str) -> (f64, [f64; 3]),
        out: &mut (Vec<Occurrence>, Vec<Instance>),
    ) {
        let placement = (depth > 0).then(|| {
            let p = node.placement.as_ref().unwrap();
            let y = DVec3::from_array(p.z_axis).cross(DVec3::from_array(p.x_axis));
            let (x, z, o) = (p.x_axis, p.z_axis, p.origin);
            [0, 1, 2].map(|r| [x[r], y[r], z[r], o[r]])
        });
        let placement = placement.map(|rows| -> Matrix { rows.concat().try_into().unwrap() });
        out.0.push(Occurrence {
            depth,
            name: node.name.clone(),
            placement,
            colour: node.colour,
        });
        let world = placement.map_or(*world, |m| compose(world, &m));
        path.push(node.name.clone());
        if node.children.is_empty() {
            let (volume, centroid) = part(&node.name);
            out.1.push(Instance {
                path: path.join("/"),
                volume,
                centroid: apply(&world, centroid),
            });
        }
        for child in &node.children {
            walk(child, depth + 1, &world, path, part, out);
        }
        path.pop();
    }
    let mut out = (Vec::new(), Vec::new());
    for root in &tree.roots {
        walk(root, 0, &IDENTITY, &mut Vec::new(), &part, &mut out);
    }
    out
}

#[track_caller]
fn near(a: &[f64], b: &[f64], tolerance: f64, what: &str) {
    assert_eq!(a.len(), b.len(), "{what}");
    for (x, y) in a.iter().zip(b) {
        assert!((x - y).abs() <= tolerance, "{what}: {a:?} vs {b:?}");
    }
}

/// Structure and names exact, placements to 1e-9 m, volumes to 1e-6
/// relative, centroids to 1e-9 m, colours to 1e-6 (Open CASCADE hands
/// them back through sRGB).
#[track_caller]
fn held_to_open_cascade(file: &str, mine: (Vec<Occurrence>, Vec<Instance>)) {
    let (mut want_o, mut want_i) = occt(file);
    let (mut got_o, mut got_i) = mine;
    // The readers list siblings in their own orders: sort by what is exact,
    // then the placement's translation, rounded.
    let key = |o: &Occurrence| {
        let t = o.placement.map_or([0; 3], |m| {
            [m[3], m[7], m[11]].map(|v| (v * 1e6).round() as i64)
        });
        (o.depth, o.name.clone(), t)
    };
    want_o.sort_by_key(key);
    got_o.sort_by_key(key);
    assert_eq!(got_o.len(), want_o.len(), "occurrences");
    for (g, w) in got_o.iter().zip(&want_o) {
        assert_eq!((g.depth, &g.name), (w.depth, &w.name));
        let what = format!("{} at depth {}", g.name, g.depth);
        assert_eq!(g.placement.is_some(), w.placement.is_some(), "{what}");
        if let (Some(a), Some(b)) = (g.placement, w.placement) {
            near(&a, &b, 1e-9, &format!("{what}: placement"));
        }
        assert_eq!(g.colour.is_some(), w.colour.is_some(), "{what}: colour");
        if let (Some(a), Some(b)) = (g.colour, w.colour) {
            near(&a, &b, 1e-6, &format!("{what}: colour"));
        }
    }
    let key = |i: &Instance| (i.path.clone(), (i.centroid[1] * 1e6).round() as i64);
    want_i.sort_by_key(key);
    got_i.sort_by_key(key);
    assert_eq!(got_i.len(), want_i.len(), "instances");
    for (g, w) in got_i.iter().zip(&want_i) {
        assert_eq!(g.path, w.path);
        assert!(
            (g.volume - w.volume).abs() <= 1e-6 * w.volume,
            "{}: volume {} vs {}",
            g.path,
            g.volume,
            w.volume
        );
        near(
            &g.centroid,
            &w.centroid,
            1e-9,
            &format!("{}: centroid", g.path),
        );
    }
}

#[test]
fn arrix_reads_the_exported_assembly_as_open_cascade_does() {
    let (c, _) = scenario();
    scenario_directory("assembly-exchange", &c.session.save());
    let (file, back) = exchange(&workspace().join("tests/docs/assembly-exchange"));
    held_to_open_cascade("exchange", arrix(&file, &back));
}

#[test]
fn arrix_reads_the_vendor_part_as_open_cascade_does() {
    let file = workspace().join("tests/fixtures/step/nist-ftc-11.stp");
    let back = scratch("vendor");
    run(&[
        "import".as_ref(),
        file.as_os_str(),
        "-o".as_ref(),
        back.as_os_str(),
    ]);
    held_to_open_cascade("nist-ftc-11", arrix(&file, &back));
}
