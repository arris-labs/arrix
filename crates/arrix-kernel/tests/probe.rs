//! The kernel probe, in SI (plans/c1-m1-document step 2): what the
//! document and `gears.spur` will lean on, checked against hand-computed
//! values before anything is built on it.

use std::collections::BTreeSet;
use std::f64::consts::{PI, TAU};

use arrix_core::{
    CurveKey, DVec2, DVec3, FeatureId, Frame, Id, NameRoot, PersistentName, Profile, ProfileLoop,
    ProfileSegment, SweepPartName, TopoKind,
};
use arrix_kernel::{ARRIS_VERSION, CallIndex, Kernel, KernelCall, KernelError, KernelOp};

const MM: f64 = 1e-3;

fn key(n: u64) -> CurveKey {
    CurveKey(Id(n))
}

fn feature() -> FeatureId {
    FeatureId(Id(100))
}

fn name(kind: TopoKind, part: SweepPartName) -> PersistentName {
    PersistentName {
        kind,
        root: NameRoot::Sweep {
            feature: feature(),
            part,
        },
        chain: vec![],
    }
}

fn close(a: f64, b: f64, rel: f64) -> bool {
    (a - b).abs() <= rel * b.abs().max(f64::MIN_POSITIVE)
}

/// A 40 × 30 mm plate with a 4 mm bore at its centre, keys 1–4 the
/// sides from the origin anticlockwise, 5 the bore.
fn plate() -> Profile {
    let p = |x: f64, y: f64| DVec2::new(x * MM, y * MM);
    let line = |k, to| ProfileSegment::Line { key: key(k), to };
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
    Profile::new(Frame::WORLD_XY, outer, vec![bore]).unwrap()
}

#[test]
fn a_plate_with_a_bore_extrudes_to_its_hand_computed_body() {
    let mut k = Kernel::new();
    let body = k.extrude(feature(), &plate(), 10.0 * MM).unwrap();
    let names = k.names(body).unwrap();
    assert_eq!(
        names.count(TopoKind::Face),
        7,
        "two caps, four walls, the bore"
    );
    assert_eq!(names.count(TopoKind::Edge), 15, "the bore's seam once");
    assert_eq!(names.count(TopoKind::Vertex), 10);

    let mut faces = BTreeSet::from([
        name(TopoKind::Face, SweepPartName::StartCap),
        name(TopoKind::Face, SweepPartName::EndCap),
    ]);
    let mut edges = BTreeSet::new();
    let mut vertices = BTreeSet::new();
    for c in (1..=5).map(key) {
        faces.insert(name(TopoKind::Face, SweepPartName::Side(c)));
        for part in [
            SweepPartName::StartEdge(c),
            SweepPartName::EndEdge(c),
            SweepPartName::Rise(c),
        ] {
            edges.insert(name(TopoKind::Edge, part));
        }
        for part in [SweepPartName::StartVertex(c), SweepPartName::EndVertex(c)] {
            vertices.insert(name(TopoKind::Vertex, part));
        }
    }
    let got = |kind| names.of_kind(kind).cloned().collect::<BTreeSet<_>>();
    assert_eq!(got(TopoKind::Face), faces);
    assert_eq!(got(TopoKind::Edge), edges);
    assert_eq!(got(TopoKind::Vertex), vertices);

    let (w, h, t, r) = (0.04, 0.03, 0.01, 0.004);
    let props = k.mass_properties(body).unwrap();
    assert!(
        close(props.volume, (w * h - PI * r * r) * t, 1e-9),
        "{props:?}"
    );
    let area = 2.0 * (w * h - PI * r * r) + 2.0 * (w + h) * t + TAU * r * t;
    assert!(close(props.area, area, 1e-9), "{props:?}");
    assert!((props.centroid - DVec3::new(w / 2.0, h / 2.0, t / 2.0)).length() < 1e-12);

    let top = k
        .face_frame(body, &name(TopoKind::Face, SweepPartName::EndCap))
        .unwrap();
    assert!((top.z_axis() - DVec3::Z).length() < 1e-12);
    assert!((top.origin().z - t).abs() < 1e-12);
    let bottom = k
        .face_frame(body, &name(TopoKind::Face, SweepPartName::StartCap))
        .unwrap();
    assert!((bottom.z_axis() + DVec3::Z).length() < 1e-12, "outward");
    let wall = k
        .face_frame(body, &name(TopoKind::Face, SweepPartName::Side(key(1))))
        .unwrap();
    assert!(
        (wall.z_axis() + DVec3::Y).length() < 1e-12,
        "the wall on y = 0"
    );
}

/// A closed loop of 60 arcs between points of a 10 mm circle, bulging
/// out and in by turns: the shape class of a gear's outline.
fn scallops() -> (Profile, f64) {
    let (n, r0, bulge) = (60, 10.0 * MM, 0.5 * MM);
    let at = |angle: f64, radius: f64| DVec2::new(angle.cos(), angle.sin()) * radius;
    let step = TAU / n as f64;
    let segments = (0..n)
        .map(|i| {
            let out = if i % 2 == 0 { bulge } else { -bulge };
            ProfileSegment::Arc {
                key: key(1 + i as u64),
                to: at(step * (i + 1) as f64, r0),
                via: at(step * (i as f64 + 0.5), r0 + out),
            }
        })
        .collect();
    let outer = ProfileLoop::Path {
        start: at(0.0, r0),
        segments,
    };
    // By hand: the 60-gon, plus each outward circular segment and less
    // each inward one. Half chord `a`, sagitta `s`, arc radius
    // `(a² + s²) / 2s`, segment `r² (θ − sin θ) / 2` with `θ = 2 asin(a/r)`.
    let a = r0 * (step / 2.0).sin();
    let segment = |via_radius: f64| {
        let s = (via_radius - r0 * (step / 2.0).cos()).abs();
        let r = (a * a + s * s) / (2.0 * s);
        let theta = 2.0 * (a / r).asin();
        r * r * (theta - theta.sin()) / 2.0
    };
    let polygon = n as f64 * r0 * r0 * step.sin() / 2.0;
    let area = polygon + (n / 2) as f64 * (segment(r0 + bulge) - segment(r0 - bulge));
    (Profile::new(Frame::WORLD_XY, outer, vec![]).unwrap(), area)
}

#[test]
fn a_sixty_arc_outline_extrudes_to_its_hand_computed_volume() {
    let (profile, area) = scallops();
    let t = 5.0 * MM;
    let mut k = Kernel::new();
    let body = k.extrude(feature(), &profile, t).unwrap();
    let names = k.names(body).unwrap();
    assert_eq!(names.count(TopoKind::Face), 62);
    assert_eq!(names.count(TopoKind::Edge), 180);
    assert_eq!(names.count(TopoKind::Vertex), 120);
    for c in (1..=60).map(key) {
        assert!(names.contains(&name(TopoKind::Face, SweepPartName::Side(c))));
    }
    let props = k.mass_properties(body).unwrap();
    assert!(
        close(props.volume, area * t, 1e-9),
        "{} vs {}",
        props.volume,
        area * t
    );
}

#[test]
fn evaluating_twice_gives_identical_names_and_volumes() {
    let run = || {
        let mut k = Kernel::new();
        let mut out = Vec::new();
        for profile in [plate(), scallops().0] {
            let body = k.extrude(feature(), &profile, 7.0 * MM).unwrap();
            let names: Vec<_> = k.names(body).unwrap().iter().cloned().collect();
            let props = k.mass_properties(body).unwrap();
            out.push((names, props.volume.to_bits(), props.area.to_bits()));
        }
        (out, k.records().to_vec())
    };
    assert_eq!(run(), run());
}

#[test]
fn a_negative_distance_extrudes_against_the_normal() {
    let mut k = Kernel::new();
    let body = k.extrude(feature(), &plate(), -10.0 * MM).unwrap();
    let props = k.mass_properties(body).unwrap();
    assert!((props.centroid.z + 0.005).abs() < 1e-12, "{props:?}");
    let start = k
        .face_frame(body, &name(TopoKind::Face, SweepPartName::StartCap))
        .unwrap();
    assert!(
        (start.z_axis() - DVec3::Z).length() < 1e-12,
        "the profile face, outward"
    );
}

#[test]
fn a_zero_length_extrude_is_a_categorised_error_with_its_record_kept() {
    let mut k = Kernel::new();
    let err = k.extrude(feature(), &plate(), 0.0).unwrap_err();
    assert_eq!(err.code().as_str(), "kernel.degenerate.not-positive");
    assert_eq!(err.call(), Some(CallIndex(0)));
    let [record] = k.records() else {
        panic!("one record, got {:?}", k.records())
    };
    assert!(matches!(record.op, KernelOp::Extrude { distance, .. } if distance == 0.0));

    let err = k.extrude(feature(), &plate(), f64::NAN).unwrap_err();
    assert_eq!(err.code().as_str(), "kernel.degenerate.non-finite");
    assert_eq!(err.call(), Some(CallIndex(1)));
}

#[test]
fn a_query_that_fails_is_recorded_and_categorised() {
    let mut k = Kernel::new();
    let body = k.extrude(feature(), &plate(), 10.0 * MM).unwrap();
    let bore = name(TopoKind::Face, SweepPartName::Side(key(5)));
    let err = k.face_frame(body, &bore).unwrap_err();
    assert_eq!(err.code().as_str(), "kernel.degenerate.not-planar");
    assert_eq!(err.call(), Some(CallIndex(1)));

    let lost = name(TopoKind::Face, SweepPartName::Side(key(9)));
    assert_eq!(
        k.face_frame(body, &lost),
        Err(KernelError::UnknownName { name: lost })
    );
    let edge = name(TopoKind::Edge, SweepPartName::Rise(key(1)));
    assert!(matches!(
        k.face_frame(body, &edge),
        Err(KernelError::WrongKind {
            found: TopoKind::Edge,
            ..
        })
    ));
    assert_eq!(
        k.records().len(),
        2,
        "a name that resolves to nothing calls nothing"
    );
}

#[test]
fn records_name_their_operands_and_cross_as_json() {
    let mut k = Kernel::new();
    let body = k.extrude(feature(), &plate(), 10.0 * MM).unwrap();
    k.mass_properties(body).unwrap();
    k.face_frame(body, &name(TopoKind::Face, SweepPartName::EndCap))
        .unwrap();
    let records = k.records();
    assert_eq!(records.len(), 3);
    assert!(records[0].operands.is_empty());
    assert_eq!(records[1].operands, [CallIndex(0)]);
    assert_eq!(records[2].operands, [CallIndex(0)]);
    for r in records {
        assert_eq!(r.precision.default_tolerance, Kernel::DEFAULT_TOLERANCE);
        let json = serde_json::to_string(r).unwrap();
        assert_eq!(&serde_json::from_str::<KernelCall>(&json).unwrap(), r);
    }
}

#[test]
fn the_recorded_arris_version_is_the_locked_one() {
    let lock =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.lock")).unwrap();
    let locked = lock
        .split("[[package]]")
        .find(|p| p.contains("\nname = \"arris\"\n"))
        .and_then(|p| p.lines().find_map(|l| l.strip_prefix("version = ")))
        .unwrap();
    assert_eq!(locked, format!("\"{ARRIS_VERSION}\""));
}

#[test]
fn retain_releases_the_other_bodies_and_keeps_the_kept_ones_whole() {
    let mut k = Kernel::new();
    let kept = k.extrude(feature(), &plate(), 7.0 * MM).unwrap();
    let dropped = k.extrude(FeatureId(Id(101)), &plate(), 3.0 * MM).unwrap();
    let before = k.mass_properties(kept).unwrap();
    k.retain(&BTreeSet::from([kept]));
    assert_eq!(k.mass_properties(kept).unwrap(), before);
    assert_eq!(
        k.mass_properties(dropped),
        Err(KernelError::UnknownBody(dropped))
    );
    let again = k.extrude(FeatureId(Id(101)), &plate(), 3.0 * MM).unwrap();
    assert_ne!(again, dropped, "a released handle is never reused");
    assert_eq!(k.names(kept).unwrap().count(TopoKind::Face), 7);
}
