//! The kernel-level naming scenarios of plans/c1-m2-naming (step 11, the
//! acceptance): on a plate with a pattern of through holes, a fillet on one
//! plate edge and a boss, an edit is made, and a name taken before it names
//! the same entity after (ADR-0010, docs/DATA-MODEL.md §Persistent naming).
//! "The same entity" is read off the name table alone, never by geometry:
//! a name is in the table of the edited body, and the table it came from
//! and the edited one agree on every name the edit does not touch. Each
//! opposite case removes what a name refers to, and the name is absent.

use std::collections::BTreeSet;

use arrix_core::{
    CurveKey, DVec2, DVec3, FeatureId, Frame, Id, NameRoot, NameStep, PersistentName, Profile,
    ProfileLoop, ProfileSegment, SweepPartName, TopoKind,
};
use arrix_kernel::{Kernel, KernelBody, Motion};

const MM: f64 = 1e-3;
const PLATE: FeatureId = FeatureId(Id(1));
const BLEND: FeatureId = FeatureId(Id(2));
const PIN: FeatureId = FeatureId(Id(3));
const PATTERN: FeatureId = FeatureId(Id(4));
const CUT: FeatureId = FeatureId(Id(5));
const BOSS: FeatureId = FeatureId(Id(6));
const JOIN: FeatureId = FeatureId(Id(7));

fn key(n: u64) -> CurveKey {
    CurveKey(Id(n))
}

fn p(x: f64, y: f64) -> DVec2 {
    DVec2::new(x * MM, y * MM)
}

/// How the plate's top side (key 3) is drawn, between the kept sides 2 and 4.
#[derive(Clone, Copy, PartialEq)]
enum Top {
    Line,
    /// Redrawn as an arc bulging 3 mm outwards: the same curve, new geometry.
    Arc,
    /// Replaced by two lines (keys 5 and 6): the curve is gone.
    Replaced,
}

/// Whether the fillet is made, and where among the other features.
#[derive(Clone, Copy, PartialEq)]
enum Blend {
    None,
    /// Between the plate (and the boss) and the holes.
    Before,
    /// After the holes.
    After,
}

#[derive(Clone)]
struct Scene {
    /// The plate's width in mm; its height is 30.
    width: f64,
    top: Top,
    blend: Blend,
    /// A boss fused to the plate's top face before the fillet.
    boss: bool,
    /// The x of the seed pin, mm; hole `i` is `6 i` mm further.
    seed_x: f64,
    holes: u32,
}

impl Scene {
    fn new() -> Self {
        Self {
            width: 40.0,
            top: Top::Line,
            blend: Blend::Before,
            boss: false,
            seed_x: 4.0,
            holes: 3,
        }
    }
}

fn plate(k: &mut Kernel, s: &Scene) -> KernelBody {
    let line = |n, to| ProfileSegment::Line { key: key(n), to };
    let w = s.width;
    let mut segments = vec![line(1, p(w, 0.0)), line(2, p(w, 30.0))];
    match s.top {
        Top::Line => segments.push(line(3, p(0.0, 30.0))),
        Top::Arc => segments.push(ProfileSegment::Arc {
            key: key(3),
            to: p(0.0, 30.0),
            via: p(w / 2.0, 33.0),
        }),
        Top::Replaced => {
            segments.push(line(5, p(w / 2.0, 30.0)));
            segments.push(line(6, p(0.0, 30.0)));
        }
    }
    segments.push(line(4, p(0.0, 0.0)));
    let outer = ProfileLoop::Path {
        start: p(0.0, 0.0),
        segments,
    };
    let profile = Profile::new(Frame::WORLD_XY, outer, vec![]).unwrap();
    k.extrude(PLATE, 0, &profile, 5.0 * MM).unwrap()
}

/// A 6 mm boss on the plate's top face, clear of the holes and the fillet.
fn boss(k: &mut Kernel) -> KernelBody {
    let circle = ProfileLoop::Circle {
        key: key(21),
        center: p(20.0, 6.0),
        radius: 3.0 * MM,
    };
    let plane = Frame::new(DVec3::new(0.0, 0.0, 5.0 * MM), DVec3::X, DVec3::Y).unwrap();
    let profile = Profile::new(plane, circle, vec![]).unwrap();
    k.extrude(BOSS, 0, &profile, 4.0 * MM).unwrap()
}

/// The pattern's tool: 1 mm pins from 1 mm below the plate to 1 mm above
/// its top, `holes` copies of the seed 6 mm apart, fused.
fn tool(k: &mut Kernel, s: &Scene) -> Option<KernelBody> {
    let circle = ProfileLoop::Circle {
        key: key(11),
        center: p(s.seed_x, 15.0),
        radius: MM,
    };
    let plane = Frame::new(DVec3::new(0.0, 0.0, -MM), DVec3::X, DVec3::Z).unwrap();
    let profile = Profile::new(plane, circle, vec![]).unwrap();
    let seed = k.extrude(PIN, 0, &profile, 7.0 * MM).unwrap();
    let step = |i: u32| Motion::translation(DVec3::new(6.0 * MM * f64::from(i), 0.0, 0.0));
    let mut tool = None;
    for i in 1..=s.holes {
        let copy = k.copy(PATTERN, i, seed, step(i)).unwrap();
        tool = Some(match tool {
            None => copy,
            Some(so_far) => k.fuse(PATTERN, so_far, copy).unwrap(),
        });
    }
    tool
}

fn build(s: &Scene) -> (Kernel, KernelBody) {
    let mut k = Kernel::new();
    let mut body = plate(&mut k, s);
    if s.boss {
        let b = boss(&mut k);
        body = k.fuse(JOIN, body, b).unwrap();
    }
    if s.blend == Blend::Before {
        body = k.fillet(BLEND, body, &[top_edge(2)], 2.0 * MM).unwrap();
    }
    if let Some(t) = tool(&mut k, s) {
        body = k.cut(CUT, body, t).unwrap();
    }
    if s.blend == Blend::After {
        body = k.fillet(BLEND, body, &[top_edge(2)], 2.0 * MM).unwrap();
    }
    (k, body)
}

fn table(s: &Scene) -> BTreeSet<PersistentName> {
    let (k, body) = build(s);
    k.names(body).unwrap().iter().cloned().collect()
}

fn plate_name(kind: TopoKind, part: SweepPartName) -> PersistentName {
    PersistentName {
        kind,
        root: NameRoot::Sweep {
            feature: PLATE,
            index: 0,
            part,
        },
        chain: vec![],
    }
}

/// The plate's top edge along side `n`.
fn top_edge(n: u64) -> PersistentName {
    plate_name(TopoKind::Edge, SweepPartName::EndEdge(key(n)))
}

fn top_face() -> PersistentName {
    plate_name(TopoKind::Face, SweepPartName::EndCap)
}

fn side(n: u64) -> PersistentName {
    plate_name(TopoKind::Face, SweepPartName::Side(key(n)))
}

/// What the fillet made from the plate's top edge along side 2.
fn blend_face() -> PersistentName {
    PersistentName {
        kind: TopoKind::Face,
        chain: vec![NameStep::Generated {
            feature: BLEND,
            from: vec![top_edge(2)],
        }],
        root: top_edge(2).root,
    }
}

fn inst(i: u32) -> NameStep {
    NameStep::Instance {
        feature: PATTERN,
        index: i,
    }
}

/// Hole `i`'s wall, found by the one cut-made face whose origin is the
/// pin's side face in copy `i`.
fn wall(names: &BTreeSet<PersistentName>, i: u32) -> Option<PersistentName> {
    let mut found = names.iter().filter(|n| {
        n.kind == TopoKind::Face
            && matches!(
                n.chain.last(),
                Some(NameStep::Generated { feature, from })
                    if *feature == CUT && from.iter().any(|f| f.chain.contains(&inst(i)))
            )
    });
    let first = found.next().cloned();
    assert!(found.next().is_none(), "hole {i} has two walls");
    first
}

/// The names a scenario holds on to, taken from the unedited scene.
#[derive(Clone)]
struct Refs {
    top_face: PersistentName,
    sides: [PersistentName; 3],
    top_edges: [PersistentName; 2],
    blend_face: PersistentName,
    walls: Vec<PersistentName>,
}

fn refs_of(s: &Scene) -> Refs {
    let names = table(s);
    let walls = (1..=s.holes).map(|i| wall(&names, i).unwrap()).collect();
    let refs = Refs {
        top_face: top_face(),
        sides: [side(1), side(2), side(4)],
        top_edges: [top_edge(1), top_edge(4)],
        blend_face: blend_face(),
        walls,
    };
    for name in refs.all() {
        assert!(
            names.contains(name),
            "{name} is in the scene it is taken from"
        );
    }
    refs
}

impl Refs {
    fn all(&self) -> impl Iterator<Item = &PersistentName> {
        std::iter::once(&self.top_face)
            .chain(&self.sides)
            .chain(&self.top_edges)
            .chain(std::iter::once(&self.blend_face))
            .chain(&self.walls)
    }

    /// Every name still names something in `after`.
    fn hold_in(&self, what: &str, after: &BTreeSet<PersistentName>) {
        for name in self.all() {
            assert!(after.contains(name), "{what}: {name} names nothing");
        }
    }
}

#[test]
fn a_width_edit_under_the_fillet_renames_nothing() {
    let base = Scene::new();
    let refs = refs_of(&base);
    for width in [30.0, 55.0, 80.0] {
        let after = table(&Scene {
            width,
            ..base.clone()
        });
        refs.hold_in(&format!("width {width}"), &after);
        // Same topology, so the whole table is the same set of names.
        assert_eq!(after, table(&base), "width {width}");
    }
}

#[test]
fn a_hole_moved_across_the_plate_keeps_every_name() {
    let base = Scene::new();
    let refs = refs_of(&base);
    // The seed from 4 mm on to 22 mm: the holes move 18 mm toward the
    // fillet, on the plate's top face the whole way. Two holes remain.
    for seed_x in [10.0, 16.0, 22.0] {
        let after = table(&Scene {
            seed_x,
            holes: 2,
            ..base.clone()
        });
        let kept = Refs {
            walls: refs.walls[..2].to_vec(),
            ..refs.clone()
        };
        kept.hold_in(&format!("seed at {seed_x}"), &after);
    }
}

#[test]
fn a_pattern_count_raised_and_lowered_renames_no_hole_that_stays() {
    let base = Scene::new();
    let refs = refs_of(&base);
    let three = table(&base);
    for holes in [1, 2, 4, 5] {
        let after = table(&Scene {
            holes,
            ..base.clone()
        });
        for (i, wall_name) in refs.walls.iter().enumerate() {
            let stays = i < holes as usize;
            assert_eq!(
                after.contains(wall_name),
                stays,
                "{holes} holes, hole {}",
                i + 1
            );
        }
        // Everything that is not a hole's keeps its name; what the count
        // changes is the holes' own entities, and no other.
        let not_hole = |n: &&PersistentName| !touches_a_hole(n);
        let before: BTreeSet<_> = three.iter().filter(not_hole).collect();
        let now: BTreeSet<_> = after.iter().filter(not_hole).collect();
        assert_eq!(before, now, "{holes} holes");
    }
    // Lowered to none: no wall remains (the opposite case).
    let none = table(&Scene { holes: 0, ..base });
    assert!(refs.walls.iter().all(|w| !none.contains(w)));
    assert!(none.contains(&top_face()) && none.contains(&blend_face()));
}

/// Whether `name` is made from a pattern copy anywhere in its chain.
fn touches_a_hole(name: &PersistentName) -> bool {
    fn mentions(name: &PersistentName) -> bool {
        name.chain.iter().any(|step| match step {
            NameStep::Instance { feature, .. } => *feature == PATTERN,
            NameStep::Generated { from, .. } => from.iter().any(mentions),
            NameStep::Modified { .. } => false,
        })
    }
    mentions(name) || matches!(name.root, NameRoot::Sweep { feature, .. } if feature == PIN)
}

#[test]
fn a_sketch_curve_redrawn_between_two_kept_ones_keeps_every_name() {
    let base = Scene::new();
    let refs = refs_of(&base);
    // Side 3 as an arc: the same curve, so its entities keep their names too.
    let arc = table(&Scene {
        top: Top::Arc,
        ..base.clone()
    });
    refs.hold_in("side 3 as an arc", &arc);
    assert_eq!(arc, table(&base));
    // The opposite: the curve replaced by two new ones. What the old one
    // bounded is gone, what the kept ones bound is not.
    let replaced = table(&Scene {
        top: Top::Replaced,
        ..base
    });
    refs.hold_in("side 3 replaced", &replaced);
    assert!(!replaced.contains(&side(3)));
    assert!(!replaced.contains(&top_edge(3)));
    assert!(replaced.contains(&side(5)) && replaced.contains(&side(6)));
}

#[test]
fn a_feature_inserted_before_the_filleted_edge_renames_nothing() {
    let base = Scene::new();
    let refs = refs_of(&base);
    let with_boss = table(&Scene {
        boss: true,
        ..base.clone()
    });
    refs.hold_in("a boss fused first", &with_boss);
    // Nothing went; the boss adds its own entities.
    assert!(table(&base).is_subset(&with_boss));
    assert!(with_boss.len() > table(&base).len());
}

#[test]
fn a_fillet_added_removed_or_reordered_above_the_top_face_keeps_its_name() {
    let base = Scene::new();
    let refs = refs_of(&base);
    let without = table(&Scene {
        blend: Blend::None,
        ..base.clone()
    });
    let after_holes = table(&Scene {
        blend: Blend::After,
        ..base.clone()
    });
    for (what, names) in [("without", &without), ("after the holes", &after_holes)] {
        // The top face, the plate's sides, the kept top edges and the
        // holes' walls are what they were whether or not, and wherever,
        // the fillet is.
        for name in refs
            .walls
            .iter()
            .chain(&refs.sides)
            .chain(&refs.top_edges)
            .chain([&refs.top_face])
        {
            assert!(names.contains(name), "{what}: {name} names nothing");
        }
    }
    // Reordered, the fillet makes the same entities with the same names.
    assert_eq!(after_holes, table(&base));
    // The opposite cases: with no fillet the blended edge is still there
    // and nothing is made from it; with it, the edge is gone and its blend
    // face exists.
    assert!(without.contains(&top_edge(2)) && !without.contains(&refs.blend_face));
    assert!(!after_holes.contains(&top_edge(2)) && after_holes.contains(&refs.blend_face));
}
