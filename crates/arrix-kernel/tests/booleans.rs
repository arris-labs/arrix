//! The boolean probe, in SI (plans/c1-m1-slice step 2): `fuse` and `cut`
//! through the choke point, with counts and volumes by hand and names
//! through the booleans' provenance. The gear's case needs the plugin's
//! outline, so it lives beside the plugin's other kernel checks
//! (`crates/arrix-cli/tests/gears.rs`).

use std::f64::consts::PI;

use arrix_core::{
    CurveKey, DVec2, DVec3, FeatureId, Frame, Id, NameRoot, NameStep, PersistentName, Profile,
    ProfileLoop, ProfileSegment, SweepPartName, TopoKind,
};
use arrix_kernel::{BodyHash, Kernel, KernelBody, KernelError, KernelOp, NameEntry, NameTable};

const MM: f64 = 1e-3;

const PLATE: FeatureId = FeatureId(Id(1));
const BOSS: FeatureId = FeatureId(Id(2));
const JOIN: FeatureId = FeatureId(Id(3));
const POCKET: FeatureId = FeatureId(Id(4));
const CUT: FeatureId = FeatureId(Id(5));

fn key(n: u64) -> CurveKey {
    CurveKey(Id(n))
}

fn plane_at(z: f64) -> Frame {
    Frame::new(DVec3::new(0.0, 0.0, z), DVec3::X, DVec3::Z).unwrap()
}

/// An axis-aligned rectangle from `(x0, y0)` to `(x1, y1)` in mm on
/// `plane`, keyed `first..first + 4` anticlockwise from its low edge.
fn rectangle(plane: Frame, (x0, y0): (f64, f64), (x1, y1): (f64, f64), first: u64) -> Profile {
    let p = |x: f64, y: f64| DVec2::new(x * MM, y * MM);
    let line = |k, to| ProfileSegment::Line { key: key(k), to };
    let outer = ProfileLoop::Path {
        start: p(x0, y0),
        segments: vec![
            line(first, p(x1, y0)),
            line(first + 1, p(x1, y1)),
            line(first + 2, p(x0, y1)),
            line(first + 3, p(x0, y0)),
        ],
    };
    Profile::new(plane, outer, vec![]).unwrap()
}

/// A `w` × 30 × 5 mm plate on the XY plane, sides keyed 1–4.
fn plate(k: &mut Kernel, w: f64) -> KernelBody {
    let profile = rectangle(Frame::WORLD_XY, (0.0, 0.0), (w, 30.0), 1);
    k.extrude(PLATE, 0, &profile, 5.0 * MM).unwrap()
}

/// A 4 mm boss, 10 mm tall, standing on the plate's top at (20, 15) mm.
fn boss(k: &mut Kernel) -> KernelBody {
    let circle = ProfileLoop::Circle {
        key: key(11),
        center: DVec2::new(20.0 * MM, 15.0 * MM),
        radius: 4.0 * MM,
    };
    let profile = Profile::new(plane_at(5.0 * MM), circle, vec![]).unwrap();
    k.extrude(BOSS, 0, &profile, 10.0 * MM).unwrap()
}

/// A 10 × 6 mm pocket through the plate, from 1 mm below it to 1 mm above.
fn pocket(k: &mut Kernel) -> KernelBody {
    let profile = rectangle(plane_at(-MM), (5.0, 5.0), (15.0, 11.0), 21);
    k.extrude(POCKET, 0, &profile, 7.0 * MM).unwrap()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * b.abs()
}

fn sweep(kind: TopoKind, feature: FeatureId, part: SweepPartName) -> PersistentName {
    PersistentName {
        kind,
        root: NameRoot::Sweep {
            index: 0,
            feature,
            part,
        },
        chain: vec![],
    }
}

fn all_names(k: &Kernel, body: KernelBody) -> Vec<String> {
    k.names(body)
        .unwrap()
        .iter()
        .map(|n| n.to_string())
        .collect()
}

/// The plate, the boss fused onto it, and the pocket cut through it.
fn plate_boss_pocket(k: &mut Kernel) -> (KernelBody, KernelBody) {
    let (p, b) = (plate(k, 40.0), boss(k));
    let joined = k.fuse(JOIN, 0, p, b).unwrap();
    let tool = pocket(k);
    let cut = k.cut(CUT, 0, joined, tool).unwrap();
    (joined, cut)
}

#[test]
fn a_boss_fused_and_a_pocket_cut_give_their_hand_computed_bodies() {
    let mut k = Kernel::new();
    let (joined, cut) = plate_boss_pocket(&mut k);
    let (plate_v, boss_v, pocket_v) = (
        0.04 * 0.03 * 0.005,
        PI * 0.004f64.powi(2) * 0.01,
        0.01 * 0.006 * 0.005,
    );

    let names = k.names(joined).unwrap().clone();
    // The plate's six, less nothing (its top keeps a hole), and the boss's
    // wall and top: its bottom cap lay on the top face, opposed, and went.
    assert_eq!(
        [TopoKind::Face, TopoKind::Edge, TopoKind::Vertex].map(|t| names.count(t)),
        [8, 15, 10],
        "{:#?}",
        all_names(&k, joined)
    );
    assert!(close(
        k.mass_properties(joined).unwrap().volume,
        plate_v + boss_v
    ));

    let names = k.names(cut).unwrap().clone();
    // Four walls, eight edges and eight vertices more, the pocket's
    // edges on the top and the bottom and its four rises.
    assert_eq!(
        [TopoKind::Face, TopoKind::Edge, TopoKind::Vertex].map(|t| names.count(t)),
        [12, 27, 18],
        "{:#?}",
        all_names(&k, cut)
    );
    let v = k.mass_properties(cut).unwrap().volume;
    assert!(close(v, plate_v + boss_v - pocket_v), "{v}");

    // What neither boolean split keeps its extrude's name (ADR-0010): the
    // top face, holed twice, is one piece of itself each time, piece 0.
    let side = sweep(TopoKind::Face, PLATE, SweepPartName::Side(key(2)));
    let top = sweep(TopoKind::Face, PLATE, SweepPartName::EndCap);
    let boss_top = sweep(TopoKind::Face, BOSS, SweepPartName::EndCap);
    for name in [&side, &boss_top, &top] {
        assert!(names.contains(name), "{name} in {:#?}", all_names(&k, cut));
    }
    // Every entity has one name, every name one entity, and the frames
    // still come through them.
    assert_eq!(names.iter().count(), 12 + 27 + 18);
    let frame = k.face_frame(cut, &top).unwrap();
    assert!((frame.z_axis() - DVec3::Z).length() < 1e-12);
}

#[test]
fn the_booleans_are_recorded_with_their_operands() {
    let mut k = Kernel::new();
    let (joined, _) = plate_boss_pocket(&mut k);
    let records = k.records();
    // plate, boss, fuse, pocket, cut.
    assert_eq!(records.len(), 5);
    assert_eq!(
        records[2].op,
        KernelOp::Fuse {
            feature: JOIN,
            boolean: 0
        }
    );
    let (fuse_operands, cut_operands) = (records[2].operands.clone(), records[4].operands.clone());
    assert_eq!(
        records[4].op,
        KernelOp::Cut {
            feature: CUT,
            boolean: 0
        }
    );
    assert_eq!(fuse_operands.len(), 2);
    assert_ne!(fuse_operands[0], fuse_operands[1]);
    // The operands are bodies by content: the fuse's result, the pocket.
    assert_eq!(cut_operands[0], hash_of(&mut k, joined));
    assert_ne!(cut_operands[0], fuse_operands[0]);
    assert_ne!(cut_operands[1], cut_operands[0]);
}

fn hash_of(k: &mut Kernel, body: KernelBody) -> BodyHash {
    BodyHash::of(&k.to_bytes(body).unwrap().bytes)
}

#[test]
fn the_operands_stay_valid_after_a_boolean() {
    let mut k = Kernel::new();
    let p = plate(&mut k, 40.0);
    let before = k.mass_properties(p).unwrap();
    let b = boss(&mut k);
    k.fuse(JOIN, 0, p, b).unwrap();
    assert_eq!(k.mass_properties(p).unwrap(), before);
    assert_eq!(k.names(p).unwrap().count(TopoKind::Face), 6);
}

#[test]
fn a_boolean_twice_from_fresh_state_gives_identical_names_and_volumes() {
    let run = || {
        let mut k = Kernel::new();
        let (joined, cut) = plate_boss_pocket(&mut k);
        let v = |k: &mut Kernel, b| k.mass_properties(b).unwrap().volume.to_bits();
        (
            all_names(&k, joined),
            all_names(&k, cut),
            v(&mut k, joined),
            v(&mut k, cut),
            k.records().to_vec(),
        )
    };
    assert_eq!(run(), run());
}

/// A 4 mm channel 2 mm deep across a `w` mm plate at x = 18 mm, which
/// splits the top face in two, then a 3 mm square hole through the left
/// piece alone.
fn channel_and_hole(k: &mut Kernel, w: f64) -> KernelBody {
    let p = plate(k, w);
    let profile = rectangle(plane_at(3.0 * MM), (18.0, -1.0), (22.0, 31.0), 31);
    let tool = k.extrude(POCKET, 0, &profile, 3.0 * MM).unwrap();
    let channelled = k.cut(CUT, 0, p, tool).unwrap();
    let profile = rectangle(plane_at(-MM), (8.0, 8.0), (11.0, 11.0), 41);
    let hole = k.extrude(FeatureId(Id(6)), 0, &profile, 7.0 * MM).unwrap();
    k.cut(FeatureId(Id(7)), 0, channelled, hole).unwrap()
}

#[test]
fn a_split_face_s_pieces_keep_their_index_across_a_width_edit() {
    // At 30 mm the left piece (18 mm) is the larger, at 50 mm the right
    // (28 mm): the index follows what bounds a piece, not its size. Piece 0
    // keeps the top face's name, piece 1 is `mod.<cut>.1` (ADR-0010); the
    // holed piece is told by the rim edges generated from it.
    let left_piece = |w| {
        let mut k = Kernel::new();
        let body = channel_and_hole(&mut k, w);
        let top = sweep(TopoKind::Face, PLATE, SweepPartName::EndCap);
        let faces: Vec<_> = k
            .names(body)
            .unwrap()
            .of_kind(TopoKind::Face)
            .cloned()
            .collect();
        let mut holed = Vec::new();
        let edges: Vec<_> = k
            .names(body)
            .unwrap()
            .of_kind(TopoKind::Edge)
            .cloned()
            .collect();
        for name in &edges {
            if let Some(NameStep::Generated { feature, from, .. }) = name.chain.last()
                && *feature == FeatureId(Id(7))
            {
                holed.extend(from.iter().filter(|f| f.root == top.root).cloned());
            }
        }
        let mut pieces = Vec::new();
        for name in &faces {
            if name.root != top.root {
                continue;
            }
            let split = match &name.chain[..] {
                [] => 0,
                [NameStep::Modified { feature, split, .. }] => {
                    assert_eq!(*feature, CUT);
                    *split
                }
                _ => panic!("a top piece is piece 0 or a cut's piece: {name}"),
            };
            pieces.push((split, holed.contains(name)));
        }
        pieces.sort_by_key(|p| p.0);
        pieces
    };
    let narrow = left_piece(30.0);
    assert_eq!(narrow.len(), 2, "{narrow:?}");
    assert_eq!(narrow.iter().map(|p| p.0).collect::<Vec<_>>(), [0, 1]);
    assert_eq!(narrow.iter().filter(|(_, left)| *left).count(), 1);
    assert_eq!(narrow, left_piece(50.0), "{narrow:?}");
}

/// The cut body, round-tripped into a kernel that has made other things.
fn round_trip() -> (Kernel, KernelBody, Kernel, KernelBody) {
    let mut k = Kernel::new();
    let (_, cut) = plate_boss_pocket(&mut k);
    let written = k.to_bytes(cut).unwrap();
    let mut other = Kernel::new();
    plate(&mut other, 7.0); // so its ids are not the writer's
    let read = other.from_bytes(&written.bytes, &written.names).unwrap();
    (k, cut, other, read)
}

#[test]
fn a_body_round_trips_through_bytes_with_its_names_and_volume() {
    let (mut k, cut, mut other, read) = round_trip();
    assert_eq!(all_names(&k, cut), all_names(&other, read));
    let (a, b) = (
        k.mass_properties(cut).unwrap(),
        other.mass_properties(read).unwrap(),
    );
    assert_eq!(a.volume.to_bits(), b.volume.to_bits());
    assert_eq!(a.area.to_bits(), b.area.to_bits());
    // Each name still leads to the same face: the planar ones' frames match.
    let faces: Vec<_> = k
        .names(cut)
        .unwrap()
        .of_kind(TopoKind::Face)
        .cloned()
        .collect();
    let mut planar = 0;
    for face in faces {
        let (x, y) = (k.face_frame(cut, &face), other.face_frame(read, &face));
        assert_eq!(x.is_ok(), y.is_ok(), "{face}");
        if let (Ok(x), Ok(y)) = (x, y) {
            assert_eq!(x, y, "{face}");
            planar += 1;
        }
    }
    assert!(planar >= 8, "the plate's faces are planar, {planar}");
}

#[test]
fn a_body_written_twice_or_in_another_kernel_gives_the_same_bytes() {
    let (mut k, cut, mut other, read) = round_trip();
    let first = k.to_bytes(cut).unwrap();
    assert_eq!(first, k.to_bytes(cut).unwrap());
    assert_eq!(first, other.to_bytes(read).unwrap());
    // Pinned: a change here is Arris's wire or its geometry having changed,
    // and is taken on purpose with an Arris bump, never by accident.
    assert_eq!(
        BodyHash::of(&first.bytes).as_str(),
        "875c52e75ea7eed5f75a6295f06082d6910117a55e537427e65fbb1463b09d6d",
        "{} bytes",
        first.bytes.len()
    );
}

#[test]
fn a_table_that_does_not_cover_the_body_one_to_one_is_refused() {
    let mut k = Kernel::new();
    let body = plate(&mut k, 40.0);
    let written = k.to_bytes(body).unwrap();
    let refused = |table: NameTable| {
        let err = Kernel::new()
            .from_bytes(&written.bytes, &table)
            .unwrap_err();
        assert_eq!(err.code().as_str(), "kernel.naming", "{err}");
        assert!(matches!(err, KernelError::Naming { .. }));
    };
    let entries = written.names.0.clone();
    assert_eq!(entries.len(), 6 + 12 + 8);
    // One entity short.
    refused(NameTable(entries[1..].to_vec()));
    // One entry past the body.
    let mut extra = entries.clone();
    extra.push(NameEntry {
        index: 999,
        ..entries[0].clone()
    });
    refused(NameTable(extra));
    // Two names for one entity, and so another with none.
    let mut doubled = entries.clone();
    doubled[1].index = doubled[0].index;
    doubled[1].name.kind = doubled[0].name.kind;
    refused(NameTable(doubled));
    // The same name twice.
    let mut twice = entries.clone();
    twice[1] = twice[0].clone();
    refused(NameTable(twice));
    // And the table as written is taken.
    Kernel::new()
        .from_bytes(&written.bytes, &written.names)
        .unwrap();
}

#[test]
fn bytes_that_are_not_a_body_fail_as_a_record_with_a_category() {
    let mut k = Kernel::new();
    let err = k
        .from_bytes(b"not a body", &NameTable::default())
        .unwrap_err();
    assert_eq!(err.code().as_str(), "kernel.body.magic");
    assert_eq!(err.call().map(|c| c.0), Some(0));
    assert!(matches!(k.records()[0].op, KernelOp::FromBytes { .. }));
    assert!(k.records()[0].operands.is_empty());
}
