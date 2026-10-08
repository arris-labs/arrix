//! A call record as an Arris fixture recipe (docs/ARCHITECTURE.md §The
//! kernel choke point, "Fixture export"). Arris's `fixture.json` has no
//! operand made of body bytes, so the recipe replays the call's closure:
//! every call that made one of its operands, and theirs, one recipe step
//! each in call order, then the call itself. The grammar written is the
//! one `arris-debug`'s `fixtures` module reads at the release the records
//! name (`tests/fixtures/README.md` in Arris).

use std::collections::BTreeSet;

use arrix_core::{DVec2, DVec3, DiagnosticCode, Profile, ProfileLoop, ProfileSegment};
use serde::Serialize;

use crate::record::{CallIndex, KernelCall, KernelOp};

/// Why a record could not become a recipe.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum FixtureError {
    #[error("no kernel call {0:?} was recorded")]
    UnknownCall(CallIndex),
    /// The recipe grammar has no step for what made this call's body: a
    /// body read from bytes (a frozen result).
    #[error("kernel call {call:?} ({op}) has no recipe step")]
    NoStep { call: CallIndex, op: &'static str },
    /// A record whose operands do not name the calls that made them.
    #[error("kernel call {0:?} does not name the calls that made its operands")]
    NoSources(CallIndex),
    /// A blend whose edges are not each given by a point.
    #[error("kernel call {0:?} has an edge with no point on it")]
    EdgeWithoutPoint(CallIndex),
}

/// An Arris fixture recipe, ready to be written as `fixture.json`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FixtureRecipe {
    description: String,
    steps: Vec<Step>,
    result: String,
    precision: Precision,
    tolerances: Tolerances,
    #[serde(skip_serializing_if = "Option::is_none")]
    analytic: Option<Analytic>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
struct Precision {
    default_tolerance: f64,
}

/// A mesh chord for a part measured in metres, as Arris's own `probe-*-m`
/// fixtures set it; the rest are Arris's defaults.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
struct Tolerances {
    mesh_chord: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct Analytic {
    expect_error: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "lowercase")]
enum Step {
    Profile {
        name: String,
        plane: Plane,
        outer: Loop,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        holes: Vec<Loop>,
    },
    Extrude {
        name: String,
        profile: String,
        direction: [f64; 3],
        length: f64,
    },
    Revolve {
        name: String,
        profile: String,
        axis: Axis,
        angle_deg: f64,
    },
    Transform {
        name: String,
        of: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        translate: Option<[f64; 3]>,
        #[serde(skip_serializing_if = "Option::is_none")]
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
        edges: Vec<[f64; 3]>,
        radius: f64,
    },
    Chamfer {
        name: String,
        of: String,
        edges: Vec<[f64; 3]>,
        distance: f64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct Plane {
    origin: [f64; 3],
    x: [f64; 3],
    y: [f64; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
enum Loop {
    Circle {
        circle: Circle,
    },
    Path {
        start: [f64; 2],
        segments: Vec<Segment>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct Circle {
    center: [f64; 2],
    radius: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
enum Segment {
    Line { line_to: [f64; 2] },
    Arc { arc_to: [f64; 2], via: [f64; 2] },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct Axis {
    origin: [f64; 3],
    direction: [f64; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct Rotate {
    axis: [f64; 3],
    origin: [f64; 3],
    angle_deg: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct MirrorPlane {
    origin: [f64; 3],
    normal: [f64; 3],
}

impl FixtureRecipe {
    /// The recipe of `call` in `records` (a `Kernel`'s, in call order),
    /// which failed as `code`. Its steps are the calls that made the
    /// operands, transitively, named `c<call>`; its result is the call's
    /// own step, or for a query (a projection, a face frame, mass
    /// properties, a mesh, body bytes) the body it was asked of, a mesh's
    /// chord its `mesh_chord`. A failure Arris's runner asserts
    /// by type (`blend-too-large`, `unsupported`, …) is its
    /// `analytic.expect_error`; any other is left for the runner to meet.
    pub fn of(
        records: &[KernelCall],
        call: CallIndex,
        code: &DiagnosticCode,
    ) -> Result<Self, FixtureError> {
        Self::build(
            records,
            call,
            &format!("which failed as {code}"),
            expect_error(code),
        )
    }

    /// The recipe of `call`, which succeeded and is kept for another
    /// reason, `why` ("measured slow: …"): built as [`of`](Self::of)'s,
    /// asserting no refusal.
    pub fn measured(
        records: &[KernelCall],
        call: CallIndex,
        why: &str,
    ) -> Result<Self, FixtureError> {
        Self::build(records, call, why, None)
    }

    fn build(
        records: &[KernelCall],
        call: CallIndex,
        why: &str,
        expect_error: Option<&'static str>,
    ) -> Result<Self, FixtureError> {
        let failed = get(records, call)?;
        let mut closure = BTreeSet::new();
        let mut stack = vec![call];
        while let Some(c) = stack.pop() {
            if closure.insert(c) {
                let record = get(records, c)?;
                if record.sources.len() != record.operands.len() {
                    return Err(FixtureError::NoSources(c));
                }
                stack.extend(&record.sources);
            }
        }
        let mut steps = Vec::new();
        for &c in &closure {
            push_steps(&mut steps, c, get(records, c)?)?;
        }
        let result = match failed.op {
            KernelOp::Project { .. }
            | KernelOp::FaceFrame { .. }
            | KernelOp::VertexPoint { .. }
            | KernelOp::EdgeAxis { .. }
            | KernelOp::MassProperties
            | KernelOp::Tessellate { .. }
            | KernelOp::ToBytes => name(failed.sources[0]),
            // Several operands and no one result to name.
            KernelOp::Export { .. } => return Err(FixtureError::NoStep { call, op: "export" }),
            _ => name(call),
        };
        Ok(Self {
            description: format!(
                "ArriX's kernel call {} ({}) on arris {}, {why}: the call and every \
                 call that made its operands, one step each named c<call>, exported \
                 from their records. Lengths in metres.",
                call.0,
                op_word(&failed.op),
                failed.arris_version,
            ),
            steps,
            result,
            precision: Precision {
                default_tolerance: failed.precision.default_tolerance,
            },
            tolerances: Tolerances {
                mesh_chord: match failed.op {
                    KernelOp::Tessellate { chord, .. } => chord,
                    _ => 1e-5,
                },
            },
            analytic: expect_error.map(|expect_error| Analytic { expect_error }),
        })
    }

    /// The recipe as `fixture.json`'s text.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a recipe is plain data") + "\n"
    }
}

fn get(records: &[KernelCall], call: CallIndex) -> Result<&KernelCall, FixtureError> {
    records
        .get(call.0 as usize)
        .ok_or(FixtureError::UnknownCall(call))
}

fn name(call: CallIndex) -> String {
    format!("c{}", call.0)
}

/// The step (two for a sweep: its profile first) that remakes `record`'s
/// body; none for a query, which makes no body.
fn push_steps(
    steps: &mut Vec<Step>,
    call: CallIndex,
    record: &KernelCall,
) -> Result<(), FixtureError> {
    let name = name(call);
    let operand = |i: usize| self::name(record.sources[i]);
    let blend_edges = |edges: &[_], at: &[DVec3]| {
        if at.len() == edges.len() {
            Ok(at.iter().map(|p| p.to_array()).collect())
        } else {
            Err(FixtureError::EdgeWithoutPoint(call))
        }
    };
    let step = match &record.op {
        KernelOp::Extrude {
            profile, distance, ..
        } => {
            let profile_name = format!("{name}.profile");
            steps.push(profile_step(&profile_name, profile));
            Step::Extrude {
                name,
                profile: profile_name,
                direction: (profile.plane().z_axis() * distance.signum()).to_array(),
                length: distance.abs(),
            }
        }
        KernelOp::Revolve {
            profile,
            axis_origin,
            axis_direction,
            angle,
            ..
        } => {
            let profile_name = format!("{name}.profile");
            steps.push(profile_step(&profile_name, profile));
            Step::Revolve {
                name,
                profile: profile_name,
                axis: Axis {
                    origin: axis_origin.to_array(),
                    direction: axis_direction.to_array(),
                },
                angle_deg: angle.to_degrees(),
            }
        }
        KernelOp::Fuse { .. } => Step::Fuse {
            name,
            a: operand(0),
            b: operand(1),
        },
        KernelOp::Common { .. } => Step::Common {
            name,
            a: operand(0),
            b: operand(1),
        },
        KernelOp::Cut { .. } => Step::Cut {
            name,
            target: operand(0),
            tool: operand(1),
        },
        KernelOp::Fillet {
            edges, at, radius, ..
        } => Step::Fillet {
            name,
            of: operand(0),
            edges: blend_edges(edges, at)?,
            radius: *radius,
        },
        KernelOp::Chamfer {
            edges,
            at,
            distance,
            ..
        } => Step::Chamfer {
            name,
            of: operand(0),
            edges: blend_edges(edges, at)?,
            distance: *distance,
        },
        // Arris's transform turns first, about `origin`, then translates:
        // a `Motion`'s order.
        KernelOp::Copy { motion, .. } => Step::Transform {
            name,
            of: operand(0),
            translate: (motion.translation != DVec3::ZERO).then(|| motion.translation.to_array()),
            rotate: (motion.angle != 0.0).then(|| Rotate {
                axis: motion.axis.to_array(),
                origin: [0.0; 3],
                angle_deg: motion.angle.to_degrees(),
            }),
        },
        KernelOp::Mirror { origin, normal, .. } => Step::Mirror {
            name,
            of: operand(0),
            plane: MirrorPlane {
                origin: origin.to_array(),
                normal: normal.to_array(),
            },
        },
        KernelOp::Project { .. }
        | KernelOp::FaceFrame { .. }
        | KernelOp::VertexPoint { .. }
        | KernelOp::EdgeAxis { .. }
        | KernelOp::MassProperties
        | KernelOp::Tessellate { .. }
        | KernelOp::ToBytes => return Ok(()),
        KernelOp::Export { .. } | KernelOp::FromBytes { .. } => {
            return Err(FixtureError::NoStep {
                call,
                op: op_word(&record.op),
            });
        }
    };
    steps.push(step);
    Ok(())
}

fn profile_step(name: &str, profile: &Profile) -> Step {
    let plane = profile.plane();
    Step::Profile {
        name: name.to_owned(),
        plane: Plane {
            origin: plane.origin().to_array(),
            x: plane.x_axis().to_array(),
            y: plane.y_axis().to_array(),
        },
        outer: profile_loop(profile.outer()),
        holes: profile.holes().iter().map(profile_loop).collect(),
    }
}

fn profile_loop(l: &ProfileLoop) -> Loop {
    let uv = |p: &DVec2| p.to_array();
    match l {
        ProfileLoop::Circle { center, radius, .. } => Loop::Circle {
            circle: Circle {
                center: uv(center),
                radius: *radius,
            },
        },
        ProfileLoop::Path { start, segments } => Loop::Path {
            start: uv(start),
            segments: segments
                .iter()
                .map(|s| match s {
                    ProfileSegment::Line { to, .. } => Segment::Line { line_to: uv(to) },
                    ProfileSegment::Arc { to, via, .. } => Segment::Arc {
                        arc_to: uv(to),
                        via: uv(via),
                    },
                })
                .collect(),
        },
    }
}

fn op_word(op: &KernelOp) -> &'static str {
    match op {
        KernelOp::Extrude { .. } => "extrude",
        KernelOp::Revolve { .. } => "revolve",
        KernelOp::Fuse { .. } => "fuse",
        KernelOp::Cut { .. } => "cut",
        KernelOp::Common { .. } => "common",
        KernelOp::Project { .. } => "project",
        KernelOp::Fillet { .. } => "fillet",
        KernelOp::Chamfer { .. } => "chamfer",
        KernelOp::Copy { .. } => "copy",
        KernelOp::Mirror { .. } => "mirror",
        KernelOp::FaceFrame { .. } => "face_frame",
        KernelOp::VertexPoint { .. } => "vertex_point",
        KernelOp::EdgeAxis { .. } => "edge_axis",
        KernelOp::MassProperties => "mass_properties",
        KernelOp::Tessellate { .. } => "tessellate",
        KernelOp::ToBytes => "to_bytes",
        KernelOp::Export { .. } => "export",
        KernelOp::FromBytes { .. } => "from_bytes",
    }
}

/// Arris's `ExpectError` word for a failure its corpus runner asserts by
/// type; `None` for any other.
fn expect_error(code: &DiagnosticCode) -> Option<&'static str> {
    const DEGENERATE: [&str; 6] = [
        "tangent-contact",
        "non-manifold",
        "blend-too-large",
        "tangent-chain",
        "vertex-blend",
        "elliptic-revolve",
    ];
    let code = code.as_str();
    if code == "kernel.unsupported" {
        return Some("unsupported");
    }
    let reason = code.strip_prefix("kernel.degenerate.")?;
    DEGENERATE.into_iter().find(|word| *word == reason)
}
