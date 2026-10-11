//! Arris's fixture recipe grammar, vendored: the types of
//! `crates/arris-debug/src/fixtures/mod.rs` at Arris's tag `v0.5.0`
//! (`ARRIS_VERSION`), as far as ArriX's fixture export writes them, field
//! for field and with Arris's serde attributes. `arris-debug` is not
//! published, and a `path =` dependency on a checkout is never taken, so
//! the grammar is copied here and changes with an Arris bump.
//!
//! One deliberate difference: every type denies unknown fields. Arris's
//! reader ignores a field it does not know, so a misspelt one would be
//! dropped there silently; here it is an error.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum Num {
    Literal(f64),
    Expr(String),
}

impl Num {
    /// The literal; the export writes no expression.
    pub fn value(&self) -> f64 {
        match self {
            Num::Literal(v) => *v,
            Num::Expr(e) => panic!("the export writes literals, found {e:?}"),
        }
    }
}

pub fn point(p: &[Num; 3]) -> [f64; 3] {
    [p[0].value(), p[1].value(), p[2].value()]
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plane {
    pub origin: [Num; 3],
    pub x: [Num; 3],
    pub y: [Num; 3],
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum Segment {
    Line {
        line_to: [Num; 2],
    },
    Arc {
        arc_to: [Num; 2],
        via: [Num; 2],
    },
    Ellipse {
        ellipse_to: [Num; 2],
        center: [Num; 2],
        major: [Num; 2],
        minor_radius: Num,
        ccw: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum Loop {
    Circle {
        circle: Circle,
    },
    Ellipse {
        ellipse: Ellipse,
    },
    Path {
        start: [Num; 2],
        segments: Vec<Segment>,
    },
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Circle {
    pub center: [Num; 2],
    pub radius: Num,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ellipse {
    pub center: [Num; 2],
    pub major: [Num; 2],
    pub minor_radius: Num,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Axis {
    pub origin: [Num; 3],
    pub direction: [Num; 3],
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rotate {
    pub axis: [Num; 3],
    #[serde(default)]
    pub origin: Option<[Num; 3]>,
    pub angle_deg: Num,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MirrorPlane {
    pub origin: [Num; 3],
    pub normal: [Num; 3],
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase", deny_unknown_fields)]
pub enum Step {
    Box {
        name: String,
        min: [Num; 3],
        max: [Num; 3],
    },
    Cylinder {
        name: String,
        base: [Num; 3],
        axis: [Num; 3],
        radius: Num,
        height: Num,
    },
    Profile {
        name: String,
        plane: Plane,
        outer: Loop,
        #[serde(default)]
        holes: Vec<Loop>,
    },
    Extrude {
        name: String,
        profile: String,
        direction: [Num; 3],
        length: Num,
    },
    Revolve {
        name: String,
        profile: String,
        axis: Axis,
        angle_deg: Num,
    },
    Transform {
        name: String,
        of: String,
        #[serde(default)]
        translate: Option<[Num; 3]>,
        #[serde(default)]
        rotate: Option<Rotate>,
    },
    Mirror {
        name: String,
        of: String,
        plane: MirrorPlane,
    },
    Fuse {
        name: String,
        a: String,
        b: String,
    },
    Common {
        name: String,
        a: String,
        b: String,
    },
    Cut {
        name: String,
        target: String,
        tool: String,
    },
    Fillet {
        name: String,
        of: String,
        edges: Vec<[Num; 3]>,
        radius: Num,
    },
    Chamfer {
        name: String,
        of: String,
        edges: Vec<[Num; 3]>,
        distance: Num,
    },
    #[serde(rename = "step")]
    Read {
        name: String,
        file: String,
        sha256: String,
        id: u64,
        #[serde(default)]
        near: Option<[Num; 3]>,
    },
}

impl Step {
    pub fn name(&self) -> &str {
        match self {
            Step::Box { name, .. }
            | Step::Cylinder { name, .. }
            | Step::Profile { name, .. }
            | Step::Extrude { name, .. }
            | Step::Revolve { name, .. }
            | Step::Transform { name, .. }
            | Step::Mirror { name, .. }
            | Step::Fuse { name, .. }
            | Step::Common { name, .. }
            | Step::Cut { name, .. }
            | Step::Fillet { name, .. }
            | Step::Chamfer { name, .. }
            | Step::Read { name, .. } => name,
        }
    }

    /// The steps this one reads, by name.
    pub fn reads(&self) -> Vec<&str> {
        match self {
            Step::Box { .. } | Step::Cylinder { .. } | Step::Profile { .. } | Step::Read { .. } => {
                vec![]
            }
            Step::Extrude { profile, .. } | Step::Revolve { profile, .. } => vec![profile],
            Step::Transform { of, .. }
            | Step::Mirror { of, .. }
            | Step::Fillet { of, .. }
            | Step::Chamfer { of, .. } => vec![of],
            Step::Fuse { a, b, .. } | Step::Common { a, b, .. } => vec![a, b],
            Step::Cut { target, tool, .. } => vec![target, tool],
        }
    }

    /// The op word, as the recipe spells it.
    pub fn op(&self) -> &'static str {
        match self {
            Step::Box { .. } => "box",
            Step::Cylinder { .. } => "cylinder",
            Step::Profile { .. } => "profile",
            Step::Extrude { .. } => "extrude",
            Step::Revolve { .. } => "revolve",
            Step::Transform { .. } => "transform",
            Step::Mirror { .. } => "mirror",
            Step::Fuse { .. } => "fuse",
            Step::Common { .. } => "common",
            Step::Cut { .. } => "cut",
            Step::Fillet { .. } => "fillet",
            Step::Chamfer { .. } => "chamfer",
            Step::Read { .. } => "step",
        }
    }
}

/// Arris's `PrecisionSpec`, its defaults `Precision::DEFAULT`'s.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PrecisionSpec {
    pub default_tolerance: f64,
    pub min_tolerance: f64,
    pub max_tolerance: f64,
    pub angular_tolerance: f64,
    pub parametric_tolerance: f64,
    pub check_samples: usize,
}

impl Default for PrecisionSpec {
    fn default() -> Self {
        let p = arris::math::Precision::DEFAULT;
        PrecisionSpec {
            default_tolerance: p.default_tolerance,
            min_tolerance: p.min_tolerance,
            max_tolerance: p.max_tolerance,
            angular_tolerance: p.angular_tolerance,
            parametric_tolerance: p.parametric_tolerance,
            check_samples: p.check_samples,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Tolerances {
    pub volume_rel: f64,
    pub area_rel: f64,
    pub centroid_abs: f64,
    pub probe: f64,
    pub mesh_chord: f64,
    pub mesh_volume_rel: f64,
    pub inertia_rel: f64,
}

impl Default for Tolerances {
    fn default() -> Self {
        Tolerances {
            volume_rel: 1e-9,
            area_rel: 1e-9,
            centroid_abs: 1e-7,
            probe: 1e-7,
            mesh_chord: 1e-3,
            mesh_volume_rel: 2e-3,
            inertia_rel: 1e-9,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExpectError {
    TangentContact,
    NonManifold,
    BlendTooLarge,
    TangentChain,
    VertexBlend,
    EllipticRevolve,
    Nurbs,
    Unsupported,
}

/// The closed forms, as far as the export writes them: the refusal.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Analytic {
    pub degenerate: bool,
    pub expect_error: Option<ExpectError>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub params: BTreeMap<String, f64>,
    #[serde(default)]
    pub variants: BTreeMap<String, BTreeMap<String, f64>>,
    pub steps: Vec<Step>,
    pub result: String,
    #[serde(default)]
    pub precision: PrecisionSpec,
    #[serde(default)]
    pub tolerances: Tolerances,
    #[serde(default)]
    pub analytic: Analytic,
}

/// `json` read as a recipe, with what Arris's interpreter holds it to:
/// step names unique, every step reading only earlier ones, the result a
/// step. Panics with the reason otherwise.
pub fn parse(json: &str) -> Recipe {
    let recipe: Recipe = serde_json::from_str(json).expect("the recipe reads as Arris's grammar");
    let mut made = BTreeSet::new();
    for step in &recipe.steps {
        for read in step.reads() {
            assert!(
                made.contains(read),
                "{} reads {read} before it is made",
                step.name()
            );
        }
        assert!(made.insert(step.name()), "{} is made twice", step.name());
    }
    assert!(
        made.contains(recipe.result.as_str()),
        "the result names no step"
    );
    recipe
}

/// `json` against the golden `tests/recipes/<slug>/fixture.json`, laid out
/// as an Arris fixture directory; `UPDATE_SNAPSHOTS=1` rewrites it.
pub fn check_golden(slug: &str, json: &str) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/recipes")
        .join(slug)
        .join("fixture.json");
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, json).unwrap();
        return;
    }
    let golden = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}; rerun with UPDATE_SNAPSHOTS=1", path.display()));
    assert_eq!(
        json,
        golden,
        "{}: rerun with UPDATE_SNAPSHOTS=1 and read the diff",
        path.display()
    );
}
