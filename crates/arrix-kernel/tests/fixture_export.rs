//! A failed call's record as an Arris fixture recipe (plans/c1-m2-naming
//! step 9, docs/ARCHITECTURE.md §The kernel choke point, "Fixture
//! export"): the call and the calls that made its operands, replayed as
//! recipe steps, read back through Arris's grammar (`arris_fixture`). The
//! goldens under `tests/recipes/` are Arris fixture directories; the one of
//! the oversize fillet, and those of the two Arris gaps this plan met, go
//! to Arris from there.

mod arris_fixture;

use arrix_core::{
    CurveKey, DVec2, DVec3, FeatureId, Frame, Id, NameRoot, PersistentName, Profile, ProfileLoop,
    ProfileSegment, SweepPartName, TopoKind,
};
use arrix_kernel::{FixtureError, FixtureRecipe, Kernel, KernelBody, KernelError, Motion};

use arris_fixture::{ExpectError, Step, point};

const MM: f64 = 1e-3;
const PLATE: FeatureId = FeatureId(Id(1));
const BLEND: FeatureId = FeatureId(Id(2));
const HOLE: FeatureId = FeatureId(Id(3));
const COPY: FeatureId = FeatureId(Id(4));

fn key(n: u64) -> CurveKey {
    CurveKey(Id(n))
}

fn p(x: f64, y: f64) -> DVec2 {
    DVec2::new(x * MM, y * MM)
}

/// A 40 × 30 × 5 mm plate, sides keyed 1–4 from the origin anticlockwise.
fn plate(k: &mut Kernel) -> KernelBody {
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
    let profile = Profile::new(Frame::WORLD_XY, outer, vec![]).unwrap();
    k.extrude(PLATE, 0, &profile, 5.0 * MM).unwrap()
}

/// A pin of radius `r` mm at `(x, y)` mm from 1 mm below the plate to
/// 1 mm above.
fn pin(k: &mut Kernel, (x, y): (f64, f64), r: f64) -> KernelBody {
    let plane = Frame::new(DVec3::new(0.0, 0.0, -MM), DVec3::X, DVec3::Z).unwrap();
    let circle = ProfileLoop::Circle {
        key: key(9),
        center: p(x, y),
        radius: r * MM,
    };
    let profile = Profile::new(plane, circle, vec![]).unwrap();
    k.extrude(HOLE, 0, &profile, 7.0 * MM).unwrap()
}

/// The plate's top edge along side `s`.
fn top(s: u64) -> PersistentName {
    PersistentName {
        kind: TopoKind::Edge,
        root: NameRoot::Sweep {
            feature: PLATE,
            index: 0,
            part: SweepPartName::EndEdge(key(s)),
        },
        chain: vec![],
    }
}

fn export(k: &Kernel, err: &KernelError) -> Result<String, FixtureError> {
    FixtureRecipe::of(k.records(), err.call().unwrap(), &err.code()).map(|r| r.to_json())
}

fn ops(recipe: &arris_fixture::Recipe) -> Vec<(&str, &str)> {
    recipe.steps.iter().map(|s| (s.name(), s.op())).collect()
}

#[test]
fn an_oversize_fillet_exports_as_a_recipe_arris_refuses_the_same_way() {
    let mut k = Kernel::new();
    let body = plate(&mut k);
    let err = k.fillet(BLEND, body, &[top(1)], 30.0 * MM).unwrap_err();
    assert_eq!(err.code().as_str(), "kernel.degenerate.blend-too-large");
    let json = export(&k, &err).unwrap();
    arris_fixture::check_golden("oversize-fillet", &json);
    let recipe = arris_fixture::parse(&json);
    assert_eq!(
        ops(&recipe),
        [
            ("c0.profile", "profile"),
            ("c0", "extrude"),
            ("c1", "fillet")
        ]
    );
    assert_eq!(recipe.result, "c1");
    assert_eq!(
        recipe.analytic.expect_error,
        Some(ExpectError::BlendTooLarge)
    );
    assert_eq!(
        recipe.precision.default_tolerance,
        Kernel::DEFAULT_TOLERANCE
    );
    // The edge is named by its midpoint: side 1 runs along y = 0 on top.
    let Step::Fillet { edges, radius, .. } = &recipe.steps[2] else {
        panic!("{:?}", recipe.steps[2])
    };
    assert_eq!(radius.value(), 30.0 * MM);
    let [at] = edges.as_slice() else {
        panic!("{edges:?}")
    };
    let at = point(at);
    for (got, want) in at.iter().zip([20.0 * MM, 0.0, 5.0 * MM]) {
        assert!((got - want).abs() < 1e-12, "{at:?}");
    }
}

/// Arris 0.5.0 gap (plans/c1-m2-naming, the open question on step 7): a
/// 1 mm fillet of a stadium's top rim fails inside Arris. The recipe goes
/// to Arris as a regression fixture; it states no refusal, since the
/// fillet is one Arris should make.
#[test]
fn a_small_fillet_of_a_stadium_rim_exports_with_no_refusal_stated() {
    let mut k = Kernel::new();
    let outer = ProfileLoop::Path {
        start: p(5.0, 0.0),
        segments: vec![
            ProfileSegment::Line {
                key: key(1),
                to: p(15.0, 0.0),
            },
            ProfileSegment::Arc {
                key: key(2),
                to: p(15.0, 10.0),
                via: p(20.0, 5.0),
            },
            ProfileSegment::Line {
                key: key(3),
                to: p(5.0, 10.0),
            },
            ProfileSegment::Arc {
                key: key(4),
                to: p(5.0, 0.0),
                via: p(0.0, 5.0),
            },
        ],
    };
    let profile = Profile::new(Frame::WORLD_XY, outer, vec![]).unwrap();
    let body = k.extrude(PLATE, 0, &profile, 5.0 * MM).unwrap();
    let err = k.fillet(BLEND, body, &[top(1)], 1.0 * MM).unwrap_err();
    assert_eq!(err.code().as_str(), "kernel.internal.invariant");
    let json = export(&k, &err).unwrap();
    arris_fixture::check_golden("stadium-rim-small-fillet", &json);
    let recipe = arris_fixture::parse(&json);
    assert_eq!(recipe.analytic.expect_error, None);
    assert_eq!(recipe.result, "c1");
}

/// Arris 0.5.0 gap (plans/c1-m2-naming, *Arris dependencies*): a 2 mm
/// hole at (39, 15) mm through a plate whose top edge at x = 40 mm is
/// filleted at 3 mm fails in Arris's `cut`. The recipe is the fillet's
/// closure and the pin's, then the cut.
#[test]
fn a_hole_across_a_fillet_exports_both_operands_closures() {
    let mut k = Kernel::new();
    let body = plate(&mut k);
    let filleted = k.fillet(BLEND, body, &[top(2)], 3.0 * MM).unwrap();
    let tool = pin(&mut k, (39.0, 15.0), 2.0);
    let err = k.cut(HOLE, filleted, tool).unwrap_err();
    assert_eq!(err.code().as_str(), "kernel.internal.split");
    let json = export(&k, &err).unwrap();
    arris_fixture::check_golden("hole-across-fillet", &json);
    let recipe = arris_fixture::parse(&json);
    assert_eq!(
        ops(&recipe),
        [
            ("c0.profile", "profile"),
            ("c0", "extrude"),
            ("c1", "fillet"),
            ("c2.profile", "profile"),
            ("c2", "extrude"),
            ("c3", "cut"),
        ]
    );
    assert_eq!(recipe.analytic.expect_error, None);
}

#[test]
fn a_recipe_holds_the_calls_its_operands_came_from_and_nothing_else() {
    let mut k = Kernel::new();
    let body = plate(&mut k);
    let _unrelated = pin(&mut k, (5.0, 5.0), 1.0);
    k.mass_properties(body).unwrap();
    let tool = pin(&mut k, (20.0, 15.0), 2.0);
    let moved = k
        .copy(
            COPY,
            1,
            tool,
            Motion {
                translation: DVec3::new(5.0 * MM, 0.0, 0.0),
                axis: DVec3::Z,
                angle: std::f64::consts::FRAC_PI_2,
            },
        )
        .unwrap();
    let image = k
        .mirror(COPY, moved, DVec3::new(20.0 * MM, 0.0, 0.0), DVec3::X)
        .unwrap();
    let tools = k.fuse(COPY, moved, image).unwrap();
    let cut = k.cut(HOLE, body, tools).unwrap();
    let err = k.chamfer(BLEND, cut, &[top(1)], 40.0 * MM).unwrap_err();
    let recipe = arris_fixture::parse(&export(&k, &err).unwrap());
    assert_eq!(
        ops(&recipe),
        [
            ("c0.profile", "profile"),
            ("c0", "extrude"),
            ("c3.profile", "profile"),
            ("c3", "extrude"),
            ("c4", "transform"),
            ("c5", "mirror"),
            ("c6", "fuse"),
            ("c7", "cut"),
            ("c8", "chamfer"),
        ],
        "neither the unrelated pin (c1) nor the query (c2) is in it"
    );
    let Step::Transform {
        translate, rotate, ..
    } = &recipe.steps[4]
    else {
        panic!("{:?}", recipe.steps[4])
    };
    assert_eq!(point(translate.as_ref().unwrap()), [5.0 * MM, 0.0, 0.0]);
    let rotate = rotate.as_ref().unwrap();
    assert_eq!(rotate.angle_deg.value(), 90.0);
    assert_eq!(point(rotate.origin.as_ref().unwrap()), [0.0; 3]);
}

#[test]
fn a_failed_query_exports_the_body_it_was_asked_of() {
    let mut k = Kernel::new();
    let body = plate(&mut k);
    let face = PersistentName {
        kind: TopoKind::Face,
        root: NameRoot::Sweep {
            feature: PLATE,
            index: 0,
            part: SweepPartName::Side(key(1)),
        },
        chain: vec![],
    };
    // A side face is planar; a frame of it succeeds, and its record is
    // exported as a recipe ending at the plate.
    k.face_frame(body, &face).unwrap();
    let recipe = FixtureRecipe::of(
        k.records(),
        arrix_kernel::CallIndex(1),
        &arrix_core::DiagnosticCode::new("kernel.internal.frame").unwrap(),
    )
    .unwrap();
    let recipe = arris_fixture::parse(&recipe.to_json());
    assert_eq!(ops(&recipe), [("c0.profile", "profile"), ("c0", "extrude")]);
    assert_eq!(recipe.result, "c0");
}

#[test]
fn a_common_exports_as_arris_s_common_step() {
    let mut k = Kernel::new();
    let body = plate(&mut k);
    // A pin clear of the plate: the two share no material.
    let tool = pin(&mut k, (60.0, 15.0), 2.0);
    let err = k.common(HOLE, body, tool).unwrap_err();
    assert_eq!(err.code().as_str(), "kernel.degenerate.empty");
    let recipe = arris_fixture::parse(&export(&k, &err).unwrap());
    assert_eq!(
        ops(&recipe),
        [
            ("c0.profile", "profile"),
            ("c0", "extrude"),
            ("c1.profile", "profile"),
            ("c1", "extrude"),
            ("c2", "common"),
        ]
    );
    let Step::Common { a, b, .. } = &recipe.steps[4] else {
        panic!("{:?}", recipe.steps[4])
    };
    assert_eq!([a.as_str(), b.as_str()], ["c0", "c1"]);
    assert_eq!(recipe.result, "c2");
}

#[test]
fn a_failed_projection_exports_the_body_it_was_asked_of() {
    let mut k = Kernel::new();
    let body = plate(&mut k);
    let rise = PersistentName {
        kind: TopoKind::Edge,
        root: NameRoot::Sweep {
            feature: PLATE,
            index: 0,
            part: SweepPartName::Rise(key(1)),
        },
        chain: vec![],
    };
    let err = k.project(body, &[rise], &Frame::WORLD_XY).unwrap_err();
    assert_eq!(
        err.code().as_str(),
        "kernel.degenerate.projection-collapses"
    );
    let recipe = arris_fixture::parse(&export(&k, &err).unwrap());
    assert_eq!(ops(&recipe), [("c0.profile", "profile"), ("c0", "extrude")]);
    assert_eq!(recipe.result, "c0");
    assert_eq!(recipe.analytic.expect_error, None);
}

#[test]
fn a_body_read_from_bytes_has_no_recipe_step() {
    let mut k = Kernel::new();
    let body = plate(&mut k);
    let bytes = k.to_bytes(body).unwrap();
    let frozen = k.from_bytes(&bytes.bytes, &bytes.names).unwrap();
    let err = k.fillet(BLEND, frozen, &[top(1)], 30.0 * MM).unwrap_err();
    assert_eq!(
        export(&k, &err),
        Err(FixtureError::NoStep {
            call: arrix_kernel::CallIndex(2),
            op: "from_bytes"
        })
    );
}

#[test]
fn a_field_arris_would_not_read_is_refused_by_the_vendored_grammar() {
    let mut k = Kernel::new();
    let body = plate(&mut k);
    let err = k.fillet(BLEND, body, &[top(1)], 30.0 * MM).unwrap_err();
    let json = export(&k, &err).unwrap();
    let misspelt = json.replacen("\"length\"", "\"lenght\"", 1);
    assert_ne!(misspelt, json);
    assert!(serde_json::from_str::<arris_fixture::Recipe>(&misspelt).is_err());
}
