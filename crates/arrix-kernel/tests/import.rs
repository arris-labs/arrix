//! The STEP import probe (plans/c2-m1 step 5): `Kernel::import_step` on
//! `tests/fixtures/step/servo.step`, whose faces, edges and vertices are
//! named from the file's own entities (`Role::File`), the same on every
//! read, in metres, a refused solid a recorded `kernel.import.<kind>`,
//! and the record replayed from its fixture recipe.

mod arris_fixture;

use std::collections::BTreeMap;
use std::f64::consts::PI;
use std::path::PathBuf;

use arris::Control;
use arris::io::step::{self, LengthUnit, ReadOptions};
use arris::math::{Axis, Point3, Vec3};
use arris::topo::Model;
use arrix_core::{FeatureId, Id, NameRoot, PersistentName, TopoKind};
use arrix_kernel::{FixtureRecipe, Kernel, KernelOp};

const IMPORT: FeatureId = FeatureId(Id(7));

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/step")
}

/// The servo, in millimetres: a 40 × 20 × 36 body, a shaft of radius 3
/// standing 6 on its top at (10, 10), and a bore of radius 2 through it
/// at (32, 10). Arris's own STEP of it, which declares millimetres.
fn servo_step() -> String {
    let mut m = Model::default();
    let none = &Control::NONE;
    let (body, _) = arris::ops::primitive_box(&mut m, [0.0; 3], [40.0, 20.0, 36.0], none).unwrap();
    let shaft_axis = Axis::new(Point3::new(10.0, 10.0, 36.0), Vec3::z()).unwrap();
    let (shaft, _) = arris::ops::primitive_cylinder(&mut m, shaft_axis, 3.0, 6.0, none).unwrap();
    let (body, _) = arris::ops::fuse(&mut m, body, shaft, none).unwrap();
    let bore_axis = Axis::new(Point3::new(32.0, 10.0, -1.0), Vec3::z()).unwrap();
    let (bore, _) = arris::ops::primitive_cylinder(&mut m, bore_axis, 2.0, 38.0, none).unwrap();
    let (body, _) = arris::ops::cut(&mut m, body, bore, none).unwrap();
    step::write(&m, &[body]).unwrap()
}

/// The servo's volume in cubic millimetres.
const SERVO_MM3: f64 = 40.0 * 20.0 * 36.0 + PI * 9.0 * 6.0 - PI * 4.0 * 36.0;

fn servo_bytes() -> Vec<u8> {
    std::fs::read(fixtures().join("servo.step")).unwrap()
}

/// The fixture is Arris's own STEP of [`servo_step`]'s recipe, as its
/// README says; `UPDATE_SNAPSHOTS=1` writes it again.
#[test]
fn the_servo_fixture_is_its_recipe_s_step() {
    let text = servo_step();
    let path = fixtures().join("servo.step");
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&path, &text).unwrap();
        return;
    }
    let golden = std::fs::read_to_string(&path).unwrap();
    assert!(
        text == golden,
        "{}: Arris's writer changed it; rerun with UPDATE_SNAPSHOTS=1 and read the diff",
        path.display()
    );
}

#[test]
fn every_entity_of_the_servo_is_named_from_its_file_entity() {
    let mut k = Kernel::new();
    let body = k.import_step(IMPORT, &servo_bytes(), 1).unwrap();
    let names = k.names(body).unwrap();
    assert_eq!(
        names.count(TopoKind::Face),
        9,
        "six of the box, the shaft's side and top, the bore"
    );
    for name in names.iter() {
        let NameRoot::Imported {
            feature, instance, ..
        } = name.root
        else {
            panic!("{name} is not an import root");
        };
        assert_eq!((feature, instance), (IMPORT, 0), "{name}");
        assert!(name.chain.is_empty(), "{name}: one entity per file entity");
    }

    // A second read, in another kernel, gives the same names.
    let mut again = Kernel::new();
    let other = again.import_step(IMPORT, &servo_bytes(), 1).unwrap();
    let a: Vec<_> = names.iter().collect();
    let b: Vec<_> = again.names(other).unwrap().iter().collect();
    assert_eq!(a, b);
}

#[test]
fn the_servo_reads_in_metres() {
    let mut k = Kernel::new();
    let body = k.import_step(IMPORT, &servo_bytes(), 1).unwrap();
    let metres = k.mass_properties(body).unwrap().volume;

    // The same file read as Arris reads by default, in millimetres.
    let mut m = Model::default();
    let text = String::from_utf8(servo_bytes()).unwrap();
    let options = ReadOptions {
        length_unit: LengthUnit::Millimetre,
    };
    let read = step::read(&mut m, &text, &options, &Control::NONE).unwrap();
    let mm = read.solids[0].result.as_ref().unwrap().body;
    let millimetres = arris::ops::measure::mass_properties(&m, mm, &Control::NONE)
        .unwrap()
        .volume;

    assert!(
        (millimetres - SERVO_MM3).abs() < 1e-6 * SERVO_MM3,
        "{millimetres}"
    );
    assert!(
        (metres - millimetres / 1e9).abs() < 1e-9 * metres,
        "{metres} m³ against {millimetres} mm³"
    );
}

#[test]
fn the_record_names_the_file_by_hash_and_keeps_its_bytes_for_a_recipe() {
    let bytes = servo_bytes();
    let mut k = Kernel::new();
    k.import_step(IMPORT, &bytes, 1).unwrap();
    let [record] = k.records() else {
        panic!("one call")
    };
    let KernelOp::ImportStep {
        feature,
        file,
        solid,
        entity,
        ..
    } = &record.op
    else {
        panic!("{:?}", record.op)
    };
    assert_eq!((*feature, *solid), (IMPORT, 1));
    assert_eq!(file.as_str(), blake3::hash(&bytes).to_hex().as_str());
    assert_eq!(entity.map(|e| e.instance), Some(0));

    let recipe = FixtureRecipe::measured(k.records(), arrix_kernel::CallIndex(0), "kept").unwrap();
    let [(file_name, file_bytes)] = recipe.files().collect::<Vec<_>>()[..] else {
        panic!("one file beside the recipe")
    };
    assert_eq!(file_bytes, bytes.as_slice());
    let parsed = arris_fixture::parse(&recipe.to_json());
    let [
        arris_fixture::Step::Read {
            file, sha256, id, ..
        },
    ] = &parsed.steps[..]
    else {
        panic!("{:?}", parsed.steps)
    };
    assert_eq!(file, file_name);
    assert_eq!(sha256, &sha256_hex(&bytes));
    assert_eq!(Some(*id), entity.map(|e| e.id));
    // The step op reads millimetres, so the recipe is in millimetres.
    assert_eq!(parsed.precision.default_tolerance, 1e-3);
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A downstream call on the imported body replays in millimetres too: the
/// fillet's radius and edge points scaled with the file.
#[test]
fn a_fillet_on_an_imported_edge_exports_in_millimetres() {
    let mut k = Kernel::new();
    let body = k.import_step(IMPORT, &servo_bytes(), 1).unwrap();
    // A straight edge of the box: one whose axis runs along z at x = 0, y = 0.
    let edge = straight_edge_at(&mut k, body, (0.0, 0.0));
    let blended = k.fillet(FeatureId(Id(8)), 0, body, &[edge], 2e-3).unwrap();
    k.mass_properties(blended).unwrap();
    let recipe = FixtureRecipe::measured(k.records(), call_of_last_fillet(&k), "kept").unwrap();
    let parsed = arris_fixture::parse(&recipe.to_json());
    let ops: Vec<_> = parsed.steps.iter().map(|s| s.op()).collect();
    assert_eq!(ops, ["step", "fillet"]);
    let arris_fixture::Step::Fillet { radius, edges, .. } = &parsed.steps[1] else {
        unreachable!()
    };
    assert!((radius.value() - 2.0).abs() < 1e-12, "{radius:?}");
    let [at] = &edges[..] else { panic!() };
    let at = arris_fixture::point(at);
    assert!(at[0].abs() < 1e-9 && at[1].abs() < 1e-9, "{at:?}");
    assert!(
        (at[2] - 18.0).abs() < 1e-9,
        "the edge's middle, in mm: {at:?}"
    );
}

/// The straight edge of `body` along z through `(x, y)` metres.
fn straight_edge_at(
    k: &mut Kernel,
    body: arrix_kernel::KernelBody,
    (x, y): (f64, f64),
) -> PersistentName {
    let edges: Vec<_> = k
        .names(body)
        .unwrap()
        .of_kind(TopoKind::Edge)
        .cloned()
        .collect();
    edges
        .into_iter()
        .find(|e| {
            k.edge_axis(body, e).is_ok_and(|a| {
                a.direction.z.abs() > 0.999
                    && (a.origin.x - x).abs() < 1e-9
                    && (a.origin.y - y).abs() < 1e-9
            })
        })
        .expect("the box's edge along z at the origin")
}

fn call_of_last_fillet(k: &Kernel) -> arrix_kernel::CallIndex {
    let i = k
        .records()
        .iter()
        .rposition(|r| matches!(r.op, KernelOp::Fillet { .. }))
        .unwrap();
    arrix_kernel::CallIndex(u32::try_from(i).unwrap())
}

#[test]
fn a_solid_past_the_file_s_count_is_import_no_solid_with_its_record() {
    let mut k = Kernel::new();
    let err = k.import_step(IMPORT, &servo_bytes(), 2).unwrap_err();
    assert_eq!(err.code().as_str(), "import.no-solid");
    assert!(
        err.to_string().contains("solid 2") && err.to_string().contains("1 solid"),
        "{err}"
    );
    assert_eq!(err.call().map(|c| c.0), Some(0));
}

#[test]
fn text_that_is_no_exchange_structure_is_kernel_import_parse() {
    let mut k = Kernel::new();
    let err = k.import_step(IMPORT, b"not a step file", 1).unwrap_err();
    assert_eq!(err.code().as_str(), "kernel.import.parse");
    assert!(err.call().is_some());
}

/// The servo's text with its shaft's cylinder made an offset surface,
/// which Arris refuses by name.
#[test]
fn a_refused_solid_is_kernel_import_kind_naming_the_file_entity() {
    let text = String::from_utf8(servo_bytes()).unwrap();
    let (line, id) = text
        .lines()
        .find_map(|l| {
            let rest = l.strip_prefix('#')?;
            let (id, body) = rest.split_once(" = ")?;
            body.starts_with("CYLINDRICAL_SURFACE(")
                .then(|| (l.to_owned(), id.to_owned()))
        })
        .expect("a cylinder in the file");
    // An offset of the cylinder, at distance 0: a valid entity Arris refuses.
    let max = text
        .lines()
        .filter_map(|l| {
            l.strip_prefix('#')?
                .split_once(" = ")?
                .0
                .parse::<u64>()
                .ok()
        })
        .max()
        .unwrap();
    let new = max + 1;
    let renamed = line.replacen(&format!("#{id} = "), &format!("#{new} = "), 1);
    let offset = format!("#{id} = OFFSET_SURFACE('',#{new},0.,.F.);");
    let edited = text.replacen(&line, &format!("{renamed}\n{offset}"), 1);

    let mut k = Kernel::new();
    let err = k.import_step(IMPORT, edited.as_bytes(), 1).unwrap_err();
    assert_eq!(err.code().as_str(), "kernel.import.offset");
    assert!(err.to_string().contains(&format!("#{id}")), "{err}");
    // Its recipe is the read alone, the edited file beside it.
    let recipe = FixtureRecipe::of(k.records(), err.call().unwrap(), &err.code()).unwrap();
    let [(_, bytes)] = recipe.files().collect::<Vec<_>>()[..] else {
        panic!()
    };
    assert_eq!(bytes, edited.as_bytes());
}

/// The names, by their text, say which file entity each came from: every
/// face's entity is an `ADVANCED_FACE` of the file.
#[test]
fn a_face_s_name_is_its_advanced_face() {
    let text = String::from_utf8(servo_bytes()).unwrap();
    let faces: BTreeMap<u64, &str> = text
        .lines()
        .filter_map(|l| {
            let (id, body) = l.strip_prefix('#')?.split_once(" = ")?;
            Some((id.parse().ok()?, body))
        })
        .collect();
    let mut k = Kernel::new();
    let body = k.import_step(IMPORT, &servo_bytes(), 1).unwrap();
    for name in k.names(body).unwrap().of_kind(TopoKind::Face) {
        let NameRoot::Imported { entity, .. } = name.root else {
            unreachable!()
        };
        assert!(faces[&entity].starts_with("ADVANCED_FACE("), "{name}");
        let text = name.to_string();
        assert!(
            text.starts_with("face:import.") && text.ends_with(&format!(".{entity}")),
            "{text}"
        );
    }
}

/// Several entities of one kind from one file entity: the half ball's
/// meridian edge runs through the south pole, where the reader splits it,
/// so its second piece is `mod.<feature>.1` of it, on every read.
#[test]
fn a_split_file_edge_names_its_second_piece_with_a_mod_step() {
    let bytes = std::fs::read(fixtures().join("half-ball.stp")).unwrap();
    let read = |k: &mut Kernel| {
        let body = k.import_step(IMPORT, &bytes, 1).unwrap();
        let names: Vec<String> = k
            .names(body)
            .unwrap()
            .iter()
            .map(|n| n.to_string())
            .collect();
        names
    };
    let names = read(&mut Kernel::new());
    let split: Vec<_> = names.iter().filter(|n| n.contains("/mod.")).collect();
    assert_eq!(
        split.len(),
        2,
        "the meridian's piece and the pole's: {names:?}"
    );
    for piece in &split {
        assert!(
            piece.starts_with("edge:import.") && piece.ends_with(".1"),
            "{piece}"
        );
        let whole = piece.split_once("/mod.").unwrap().0;
        assert!(
            names.iter().any(|n| n == whole),
            "{whole} is the first piece"
        );
    }
    assert_eq!(read(&mut Kernel::new()), names);
}
