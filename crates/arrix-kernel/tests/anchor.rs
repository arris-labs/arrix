//! The anchor-frame probe (plans/c2-m1 step 6, ADR-0029 §3): each axis
//! kind on the imported servo and on swept bodies, with and without an
//! origin face, against hand-computed frames; the kinds an anchor does not
//! take refused by name.

use std::collections::BTreeMap;
use std::path::PathBuf;

use arrix_core::{
    CurveKey, DVec2, DVec3, FeatureId, Frame, Id, NameRoot, PersistentName, Profile, ProfileLoop,
    ProfileSegment, SweepPartName, TopoKind,
};
use arrix_kernel::{Kernel, KernelBody, KernelOp};

const MM: f64 = 1e-3;
const IMPORT: FeatureId = FeatureId(Id(7));
const SWEEP: FeatureId = FeatureId(Id(8));

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/step")
        .join(name);
    std::fs::read_to_string(path).unwrap()
}

/// A STEP file's instances, `#id` to the text after `=`.
fn instances(text: &str) -> BTreeMap<u64, String> {
    text.lines()
        .filter_map(|l| {
            let (id, body) = l.strip_prefix('#')?.split_once(" = ")?;
            Some((id.parse().ok()?, body.trim_end_matches(';').to_owned()))
        })
        .collect()
}

/// The references `#n` in an instance's text, in order.
fn refs(body: &str) -> Vec<u64> {
    body.split('#')
        .skip(1)
        .filter_map(|s| {
            let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
            digits.parse().ok()
        })
        .collect()
}

fn imported(kind: TopoKind, entity: u64) -> PersistentName {
    PersistentName {
        kind,
        root: NameRoot::Imported {
            feature: IMPORT,
            entity,
            instance: 0,
        },
        chain: vec![],
    }
}

/// The servo, read, with its file's instances.
struct Servo {
    k: Kernel,
    body: KernelBody,
    step: BTreeMap<u64, String>,
}

impl Servo {
    fn new() -> Self {
        let text = fixture("servo.step");
        let mut k = Kernel::new();
        let body = k.import_step(IMPORT, text.as_bytes(), 1).unwrap();
        Self {
            k,
            body,
            step: instances(&text),
        }
    }

    /// The face whose surface is the first instance its `ADVANCED_FACE`
    /// names that satisfies `is`, given the surface's and its placement's
    /// text.
    fn face(&self, is: impl Fn(&str, &[f64]) -> bool) -> PersistentName {
        let found: Vec<u64> = self
            .step
            .iter()
            .filter(|(_, body)| body.starts_with("ADVANCED_FACE("))
            .filter(|(_, body)| {
                let surface = &self.step[refs(body).last().unwrap()];
                is(surface, &self.placement(surface))
            })
            .map(|(&id, _)| id)
            .collect();
        let [id] = found[..] else {
            panic!("{} faces match", found.len())
        };
        imported(TopoKind::Face, id)
    }

    /// The edge whose `EDGE_CURVE`'s 3D curve satisfies `is`.
    fn edge(&self, is: impl Fn(&str, &[f64]) -> bool) -> PersistentName {
        let found: Vec<u64> = self
            .step
            .iter()
            .filter(|(_, body)| body.starts_with("EDGE_CURVE("))
            .filter(|(_, body)| {
                let mut curve = &self.step[&refs(body)[2]];
                // The writer holds the 3D curve in a surface curve.
                if curve.starts_with("SURFACE_CURVE(") || curve.starts_with("SEAM_CURVE(") {
                    curve = &self.step[&refs(curve)[0]];
                }
                is(curve, &self.placement(curve))
            })
            .map(|(&id, _)| id)
            .collect();
        let [id] = found[..] else {
            panic!("{} edges match", found.len())
        };
        imported(TopoKind::Edge, id)
    }

    /// The numbers of an entity placed by an `AXIS2_PLACEMENT_3D`: its
    /// location (0–2), its axis (3–5), then the entity's own (a radius).
    /// Empty for any other entity.
    fn placement(&self, entity: &str) -> Vec<f64> {
        let Some(placement) = refs(entity).first().map(|r| &self.step[r]) else {
            return Vec::new();
        };
        if !placement.starts_with("AXIS2_PLACEMENT_3D(") {
            return Vec::new();
        }
        let r = refs(placement);
        let mut out = numbers(&self.step[&r[0]]);
        out.extend(numbers(&self.step[&r[1]]));
        out.extend(numbers(entity));
        out
    }

    fn frame(&mut self, axis: &PersistentName, origin: Option<&PersistentName>) -> Frame {
        self.k.anchor_frame(self.body, axis, origin).unwrap()
    }
}

/// The plain numbers in an instance's text, `#` references left out.
fn numbers(text: &str) -> Vec<f64> {
    text.split(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-' | 'E' | 'e' | '#')))
        .filter(|s| !s.starts_with('#') && s.chars().any(|c| c.is_ascii_digit()))
        .filter_map(|s| s.parse().ok())
        .collect()
}

fn mm(x: f64, y: f64, z: f64) -> DVec3 {
    DVec3::new(x, y, z) * MM
}

fn near(a: DVec3, b: DVec3) -> bool {
    (a - b).length() < 1e-9
}

/// A cylinder of radius `r` mm placed at `(x, y)` mm.
fn cylinder(r: f64, x: f64, y: f64) -> impl Fn(&str, &[f64]) -> bool {
    move |s: &str, n: &[f64]| {
        s.starts_with("CYLINDRICAL_SURFACE(")
            && n.len() == 7
            && (n[0] - x).abs() < 1e-9
            && (n[1] - y).abs() < 1e-9
            && (n[6] - r).abs() < 1e-9
    }
}

/// The plane square to world axis `axis` (0 for x, 2 for z) at `at` mm.
fn plane_at(axis: usize, at: f64) -> impl Fn(&str, &[f64]) -> bool {
    move |s: &str, n: &[f64]| {
        s.starts_with("PLANE(")
            && n.len() == 6
            && (n[3 + axis].abs() - 1.0).abs() < 1e-9
            && (n[axis] - at).abs() < 1e-9
    }
}

#[test]
fn a_cylindrical_face_gives_its_axis_at_its_lowest_extent() {
    let mut servo = Servo::new();
    let shaft = servo.face(cylinder(3.0, 10.0, 10.0));
    let f = servo.frame(&shaft, None);
    assert!(near(f.z_axis().abs(), DVec3::Z), "{f:?}");
    // The shaft runs from the box's top, 36, to 42: its lowest extent along
    // z is the end z points away from.
    let low = if f.z_axis().z > 0.0 { 36.0 } else { 42.0 };
    assert!(near(f.origin(), mm(10.0, 10.0, low)), "{f:?}");
    assert!(
        near(f.x_axis(), DVec3::X),
        "world X is least aligned with z"
    );

    let bore = servo.face(cylinder(2.0, 32.0, 10.0));
    let f = servo.frame(&bore, None);
    let low = if f.z_axis().z > 0.0 { 0.0 } else { 36.0 };
    assert!(near(f.origin(), mm(32.0, 10.0, low)), "{f:?}");
}

#[test]
fn an_origin_face_moves_the_point_onto_its_plane_and_gives_x() {
    let mut servo = Servo::new();
    let shaft = servo.face(cylinder(3.0, 10.0, 10.0));
    let top = servo.face(plane_at(2, 36.0));
    let top_frame = servo.k.face_frame(servo.body, &top).unwrap();
    let f = servo.frame(&shaft, Some(&top));
    assert!(near(f.origin(), mm(10.0, 10.0, 36.0)), "{f:?}");
    let u = top_frame.x_axis();
    let expected = (u - f.z_axis() * u.dot(f.z_axis())).normalize();
    assert!(near(f.x_axis(), expected), "{f:?} against the top's u {u}");

    // The shaft's far end, as an origin: a plane at 42.
    let end = servo.face(plane_at(2, 42.0));
    let f = servo.frame(&shaft, Some(&end));
    assert!(near(f.origin(), mm(10.0, 10.0, 42.0)), "{f:?}");
}

#[test]
fn a_planar_face_gives_its_outward_normal_at_its_frame_origin() {
    let mut servo = Servo::new();
    let top = servo.face(plane_at(2, 36.0));
    let f = servo.frame(&top, None);
    assert!(near(f.z_axis(), DVec3::Z), "outward from the top: {f:?}");
    assert!((f.origin().z - 36.0 * MM).abs() < 1e-12, "{f:?}");
    assert!(near(f.x_axis(), DVec3::X));

    // The side at x = 0 faces −X; Y and Z tie for least aligned, Y first.
    let side = servo.face(plane_at(0, 0.0));
    let f = servo.frame(&side, None);
    assert!(near(f.z_axis(), -DVec3::X), "{f:?}");
    assert!(near(f.x_axis(), DVec3::Y), "{f:?}");
}

#[test]
fn a_circular_edge_gives_its_normal_at_its_centre() {
    let mut servo = Servo::new();
    // The shaft's top rim: a circle of radius 3 at z = 42.
    let rim = servo.edge(|s, n| {
        s.starts_with("CIRCLE(") && n.len() == 7 && (n[2] - 42.0).abs() < 1e-9 && n[6] == 3.0
    });
    let f = servo.frame(&rim, None);
    assert!(near(f.z_axis().abs(), DVec3::Z), "{f:?}");
    assert!(near(f.origin(), mm(10.0, 10.0, 42.0)), "{f:?}");
}

#[test]
fn a_straight_edge_gives_its_line_at_its_start() {
    let mut servo = Servo::new();
    let names: Vec<_> = servo
        .k
        .names(servo.body)
        .unwrap()
        .of_kind(TopoKind::Edge)
        .cloned()
        .collect();
    // The box's vertical edge at the origin: a line along z through (0, 0).
    let edge = names
        .into_iter()
        .find(|e| {
            servo.k.edge_axis(servo.body, e).is_ok_and(|a| {
                a.direction.z.abs() > 0.999 && a.origin.x.abs() < 1e-12 && a.origin.y.abs() < 1e-12
            })
        })
        .unwrap();
    let f = servo.frame(&edge, None);
    assert!(near(f.z_axis().abs(), DVec3::Z), "{f:?}");
    // Its start along its line is the end z points away from.
    let start = if f.z_axis().z > 0.0 { 0.0 } else { 36.0 };
    assert!(near(f.origin(), mm(0.0, 0.0, start)), "{f:?}");
}

#[test]
fn the_kinds_an_anchor_does_not_take_are_refused_by_name_and_recorded() {
    let mut servo = Servo::new();
    let shaft = servo.face(cylinder(3.0, 10.0, 10.0));
    let side = servo.face(plane_at(0, 0.0));

    let parallel = servo
        .k
        .anchor_frame(servo.body, &shaft, Some(&side))
        .unwrap_err();
    assert_eq!(parallel.code().as_str(), "kernel.invalid-input");
    assert!(parallel.to_string().contains("parallel"), "{parallel}");
    assert!(matches!(
        servo.k.record(parallel.call().unwrap()).unwrap().op,
        KernelOp::AnchorFrame { .. }
    ));

    let curved_origin = servo
        .k
        .anchor_frame(servo.body, &side, Some(&shaft))
        .unwrap_err();
    assert_eq!(curved_origin.code().as_str(), "kernel.invalid-input");
    assert!(curved_origin.to_string().contains(&shaft.to_string()));

    let vertex = servo
        .k
        .names(servo.body)
        .unwrap()
        .of_kind(TopoKind::Vertex)
        .next()
        .unwrap()
        .clone();
    let wrong = servo.k.anchor_frame(servo.body, &vertex, None).unwrap_err();
    assert_eq!(wrong.code().as_str(), "kernel.wrong-kind");
}

/// A sphere and a B-spline cylinder (Open CASCADE's STEP of a cylinder
/// converted to B-splines, every face a NURBS) are no anchor axis.
#[test]
fn a_sphere_or_a_b_spline_face_is_kernel_invalid_input_naming_it() {
    for (file, kind) in [
        ("half-ball.stp", "Sphere"),
        ("nurbs-cylinder.step", "Nurbs"),
    ] {
        let mut k = Kernel::new();
        let body = k.import_step(IMPORT, fixture(file).as_bytes(), 1).unwrap();
        let faces: Vec<_> = k
            .names(body)
            .unwrap()
            .of_kind(TopoKind::Face)
            .cloned()
            .collect();
        let refused: Vec<_> = faces
            .iter()
            .filter_map(|f| k.anchor_frame(body, f, None).err().map(|e| (f, e)))
            .collect();
        assert!(!refused.is_empty(), "{file}");
        for (face, err) in refused {
            assert_eq!(err.code().as_str(), "kernel.invalid-input", "{file}");
            let message = err.to_string();
            assert!(
                message.contains(&face.to_string()) && message.contains(kind),
                "{message}"
            );
        }
    }
}

fn key(n: u64) -> CurveKey {
    CurveKey(Id(n))
}

fn sweep(kind: TopoKind, part: SweepPartName) -> PersistentName {
    PersistentName {
        kind,
        root: NameRoot::Sweep {
            feature: SWEEP,
            index: 0,
            part,
        },
        chain: vec![],
    }
}

/// A 40 × 30 × 10 mm plate with a bore of radius 4 at (20, 15), keys 1–4
/// its sides, 5 the bore.
fn plate(k: &mut Kernel) -> KernelBody {
    let p = |x: f64, y: f64| DVec2::new(x * MM, y * MM);
    let line = |n, to| ProfileSegment::Line { key: key(n), to };
    let outer = ProfileLoop::Path {
        start: p(0.0, 0.0),
        segments: vec![
            line(1, p(40.0, 0.0)),
            line(2, p(40.0, 30.0)),
            line(3, p(0.0, 30.0)),
            line(4, p(0.0, 0.0)),
        ],
    };
    let bore = ProfileLoop::Circle {
        key: key(5),
        center: p(20.0, 15.0),
        radius: 4.0 * MM,
    };
    let profile = Profile::new(Frame::WORLD_XY, outer, vec![bore]).unwrap();
    k.extrude(SWEEP, 0, &profile, 10.0 * MM).unwrap()
}

#[test]
fn an_extruded_bore_anchors_on_its_axis_and_its_cap() {
    let mut k = Kernel::new();
    let body = plate(&mut k);
    let bore = sweep(TopoKind::Face, SweepPartName::Side(key(5)));
    let f = k.anchor_frame(body, &bore, None).unwrap();
    assert!(near(f.z_axis().abs(), DVec3::Z), "{f:?}");
    let low = if f.z_axis().z > 0.0 { 0.0 } else { 10.0 };
    assert!(near(f.origin(), mm(20.0, 15.0, low)), "{f:?}");

    // On the end cap, whose plane is z = 10 mm.
    let cap = sweep(TopoKind::Face, SweepPartName::EndCap);
    let f = k.anchor_frame(body, &bore, Some(&cap)).unwrap();
    assert!(near(f.origin(), mm(20.0, 15.0, 10.0)), "{f:?}");

    // The bore's rim on the end cap: a circular edge at its centre.
    let rim = sweep(TopoKind::Edge, SweepPartName::EndEdge(key(5)));
    let f = k.anchor_frame(body, &rim, None).unwrap();
    assert!(near(f.origin(), mm(20.0, 15.0, 10.0)), "{f:?}");
    assert!(near(f.z_axis().abs(), DVec3::Z), "{f:?}");
}

/// A ring revolved a full turn about z: its outer wall is a cylinder about
/// the world axis, whatever the revolve made of it.
#[test]
fn a_revolved_wall_anchors_on_the_revolve_s_axis() {
    let p = |x: f64, y: f64| DVec2::new(x * MM, y * MM);
    let line = |n, to| ProfileSegment::Line { key: key(n), to };
    // In the XZ plane (u along x, v along z): r from 6 to 9, z from 2 to 5.
    let plane = Frame::new(DVec3::ZERO, DVec3::X, -DVec3::Y).unwrap();
    let outer = ProfileLoop::Path {
        start: p(6.0, 2.0),
        segments: vec![
            line(1, p(9.0, 2.0)),
            line(2, p(9.0, 5.0)),
            line(3, p(6.0, 5.0)),
            line(4, p(6.0, 2.0)),
        ],
    };
    let profile = Profile::new(plane, outer, vec![]).unwrap();
    let mut k = Kernel::new();
    let body = k
        .revolve(
            SWEEP,
            0,
            &profile,
            (DVec3::ZERO, DVec3::Z),
            std::f64::consts::TAU,
        )
        .unwrap();
    let wall = PersistentName {
        kind: TopoKind::Face,
        root: NameRoot::Revolve {
            feature: SWEEP,
            index: 0,
            part: SweepPartName::Side(key(2)),
        },
        chain: vec![],
    };
    let f = k.anchor_frame(body, &wall, None).unwrap();
    assert!(near(f.z_axis().abs(), DVec3::Z), "{f:?}");
    let low = if f.z_axis().z > 0.0 { 2.0 } else { 5.0 };
    assert!(near(f.origin(), mm(0.0, 0.0, low)), "{f:?}");
}
