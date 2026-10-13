//! Bodies as exchange files (plans/c1-m4 step 4): STL and OBJ closed with
//! their volume within the refined chord, the same bytes on a second
//! export, a frozen body exporting like a live one, and STEP refused until
//! Arris's writer can declare metres (Arris ask A12).

use std::collections::BTreeMap;

use arrix_core::{
    CurveKey, DVec2, FeatureId, Frame, Id, NameRoot, PersistentName, Profile, ProfileLoop,
    ProfileSegment, SweepPartName, TopoKind,
};
use arrix_kernel::{Kernel, KernelBody, KernelOp};

const MM: f64 = 1e-3;
const PLATE: FeatureId = FeatureId(Id(1));
const ROUND: FeatureId = FeatureId(Id(2));

fn key(n: u64) -> CurveKey {
    CurveKey(Id(n))
}

/// A 40 × 30 mm plate 8 mm thick with a 10 mm hole, its top outer edge
/// along the first side rounded: planes, a cylinder and a blend.
fn bracket(k: &mut Kernel) -> KernelBody {
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
    let hole = ProfileLoop::Circle {
        key: key(10),
        center: p(20.0, 15.0),
        radius: 5.0 * MM,
    };
    let profile = Profile::new(Frame::WORLD_XY, outer, vec![hole]).unwrap();
    let plate = k.extrude(PLATE, 0, &profile, 8.0 * MM).unwrap();
    let edge = PersistentName {
        kind: TopoKind::Edge,
        root: NameRoot::Sweep {
            feature: PLATE,
            index: 0,
            part: SweepPartName::EndEdge(key(1)),
        },
        chain: vec![],
    };
    k.fillet(ROUND, 0, plate, &[edge], 2.0 * MM).unwrap()
}

type Triangle = [[f64; 3]; 3];

/// Whether every edge of `triangles` is in exactly two of them, and the
/// volume they enclose.
fn closed_and_volume(triangles: &[Triangle]) -> (bool, f64) {
    let bits = |p: [f64; 3]| p.map(f64::to_bits);
    let mut uses: BTreeMap<([u64; 3], [u64; 3]), u32> = BTreeMap::new();
    let mut volume = 0.0;
    for t in triangles {
        for i in 0..3 {
            let (a, b) = (bits(t[i]), bits(t[(i + 1) % 3]));
            *uses.entry((a.min(b), a.max(b))).or_default() += 1;
        }
        let [a, b, c] = *t;
        let cross = [
            b[1] * c[2] - b[2] * c[1],
            b[2] * c[0] - b[0] * c[2],
            b[0] * c[1] - b[1] * c[0],
        ];
        volume += (a[0] * cross[0] + a[1] * cross[1] + a[2] * cross[2]) / 6.0;
    }
    (uses.values().all(|&n| n == 2), volume)
}

fn stl_triangles(bytes: &[u8]) -> Vec<Triangle> {
    let count = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
    assert_eq!(bytes.len(), 84 + 50 * count);
    let f = |at: usize| f64::from(f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()));
    (0..count)
        .map(|i| {
            let base = 84 + 50 * i + 12;
            std::array::from_fn(|v| std::array::from_fn(|c| f(base + 12 * v + 4 * c)))
        })
        .collect()
}

fn obj_triangles(text: &str) -> Vec<Triangle> {
    let mut points = Vec::new();
    let mut triangles = Vec::new();
    for line in text.lines() {
        let mut words = line.split_whitespace();
        match words.next() {
            Some("v") => points.push(words.map(|w| w.parse().unwrap()).collect::<Vec<f64>>()),
            Some("f") => {
                let at = |w: &str| {
                    let i: usize = w.split('/').next().unwrap().parse().unwrap();
                    let p = &points[i - 1];
                    [p[0], p[1], p[2]]
                };
                let corners: Vec<_> = words.map(at).collect();
                triangles.push([corners[0], corners[1], corners[2]]);
            }
            _ => {}
        }
    }
    triangles
}

/// How far the refined mesh may be from the body's volume: its chord on
/// every square metre of surface.
fn volume_tolerance(k: &mut Kernel, body: KernelBody) -> f64 {
    let props = k.mass_properties(body).unwrap();
    let tessellated = k.tessellate(body, arrix_core::MeshDetail::Refined).unwrap();
    tessellated.chord * props.area
}

#[test]
fn step_declares_metres_and_reads_back_bit_exact() {
    let mut k = Kernel::new();
    let body = bracket(&mut k);
    let want = k.mass_properties(body).unwrap();
    let text = k.export_step(&[body]).unwrap();
    assert!(
        text.contains("SI_UNIT($,.METRE."),
        "the file declares metres"
    );
    let call = k.records().last().unwrap();
    assert!(matches!(
        call.op,
        KernelOp::Export { format } if format == arrix_core::ExportFormat::Step
    ));
    assert_eq!(call.operands.len(), 1);

    let back = k.import_step(FeatureId(Id(3)), text.as_bytes(), 1).unwrap();
    let got = k.mass_properties(back).unwrap();
    // A file read in the unit it declares returns every coordinate as it
    // was written, so the properties agree to rounding of their sums.
    let near = |a: f64, b: f64| (a - b).abs() <= 1e-12 * a.abs().max(b.abs());
    assert!(
        near(got.volume, want.volume),
        "{} {}",
        got.volume,
        want.volume
    );
    assert!(near(got.area, want.area), "{} {}", got.area, want.area);
    for i in 0..3 {
        assert!((got.centroid[i] - want.centroid[i]).abs() < 1e-15);
    }
    assert_eq!(k.export_step(&[body]).unwrap(), text, "deterministic");
}

#[test]
fn stl_and_obj_are_closed_with_their_volume_within_the_chord() {
    let mut k = Kernel::new();
    let body = bracket(&mut k);
    let want = k.mass_properties(body).unwrap().volume;
    let tolerance = volume_tolerance(&mut k, body);

    let stl = k.export_stl(&[body]).unwrap();
    assert_eq!(k.export_stl(&[body]).unwrap(), stl, "deterministic");
    let (closed, volume) = closed_and_volume(&stl_triangles(&stl));
    assert!(closed, "every STL edge is in two triangles");
    assert!(
        (volume - want).abs() <= tolerance,
        "{volume} against {want} ± {tolerance}"
    );

    let obj = k.export_obj(&[body]).unwrap();
    assert_eq!(k.export_obj(&[body]).unwrap(), obj, "deterministic");
    let (closed, volume) = closed_and_volume(&obj_triangles(&obj));
    assert!(closed, "every OBJ edge is in two triangles");
    assert!(
        (volume - want).abs() <= tolerance,
        "{volume} against {want} ± {tolerance}"
    );
}

#[test]
fn several_bodies_are_one_file_in_the_order_given() {
    let mut k = Kernel::new();
    let one = bracket(&mut k);
    let two = bracket(&mut k);
    let single = obj_triangles(&k.export_obj(&[one]).unwrap()).len();
    let both = obj_triangles(&k.export_obj(&[one, two]).unwrap()).len();
    assert_eq!(both, 2 * single);
    let call = k.records().last().unwrap();
    assert_eq!(call.operands.len(), 2, "both bodies are operands");
}

#[test]
fn a_frozen_body_exports_like_a_live_one() {
    let mut live = Kernel::new();
    let body = bracket(&mut live);
    let written = live.to_bytes(body).unwrap();
    let mut thawed = Kernel::new();
    let frozen = thawed.from_bytes(&written.bytes, &written.names).unwrap();
    let stl = thawed.export_stl(&[frozen]).unwrap();
    let (closed, volume) = closed_and_volume(&stl_triangles(&stl));
    let want = live.mass_properties(body).unwrap().volume;
    assert!(closed);
    assert!((volume - want).abs() <= volume_tolerance(&mut live, body));
}

#[test]
fn an_stl_of_nothing_is_a_header_and_a_zero() {
    let mut k = Kernel::new();
    let empty = k.export_stl(&[]).unwrap();
    assert_eq!(empty.len(), 84, "an STL of nothing is a header and a zero");
}
