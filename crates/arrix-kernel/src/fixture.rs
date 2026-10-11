//! A call record as an Arris fixture recipe (docs/ARCHITECTURE.md §The
//! kernel choke point, "Fixture export"). Arris's `fixture.json` has no
//! operand made of body bytes, so the recipe replays the call's closure:
//! every call that made one of its operands, and theirs, one recipe step
//! each in call order, then the call itself. The grammar written is the
//! one `arris-debug`'s `fixtures` module reads at the release the records
//! name (`tests/fixtures/README.md` in Arris).

use std::collections::BTreeSet;
use std::sync::Arc;

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
    /// An import whose record holds no file: one read back from text.
    #[error("kernel call {0:?} holds no file to write beside the recipe")]
    NoFile(CallIndex),
    /// An import whose read found no solid to name: the file did not
    /// parse, or had fewer solids than asked.
    #[error("kernel call {0:?} names no solid of its file")]
    NoFileSolid(CallIndex),
    /// An import of a solid the file places more than once: Arris's step
    /// op then takes a point near the placement's centroid, which the
    /// record does not hold.
    #[error("kernel call {0:?} reads a solid the file places more than once")]
    Placement(CallIndex),
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
    /// The files its `step` steps read, to be written beside it.
    #[serde(skip)]
    files: Vec<RecipeFile>,
}

/// A file a recipe reads, by the name its `step` step gives it.
#[derive(Clone, PartialEq)]
struct RecipeFile {
    name: String,
    bytes: Arc<[u8]>,
}

impl std::fmt::Debug for RecipeFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({} bytes)", self.name, self.bytes.len())
    }
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
    /// A solid of a STEP file beside the recipe, by its `#id`.
    #[serde(rename = "step")]
    Read {
        name: String,
        file: String,
        sha256: String,
        id: u64,
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
    /// An import is a `step` step over the file, written beside the
    /// recipe ([`files`](Self::files)); since that step reads in Arris's
    /// default millimetres, a recipe holding one is in millimetres, every
    /// length and the precision the records' times 1000.
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
        let imports = closure
            .iter()
            .any(|&c| matches!(records[c.0 as usize].op, KernelOp::ImportStep { .. }));
        let scale = if imports { 1e3 } else { 1.0 };
        let mut out = Out {
            steps: Vec::new(),
            files: Vec::new(),
            scale,
        };
        for &c in &closure {
            out.push(c, get(records, c)?)?;
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
                 from their records. {}",
                call.0,
                op_word(&failed.op),
                failed.arris_version,
                if imports {
                    "Lengths in millimetres, the records' metres times 1000, since a \
                     step step reads its file in millimetres."
                } else {
                    "Lengths in metres."
                },
            ),
            steps: out.steps,
            result,
            precision: Precision {
                default_tolerance: failed.precision.default_tolerance * scale,
            },
            tolerances: Tolerances {
                mesh_chord: scale
                    * match failed.op {
                        KernelOp::Tessellate { chord, .. } => chord,
                        _ => 1e-5,
                    },
            },
            analytic: expect_error.map(|expect_error| Analytic { expect_error }),
            files: out.files,
        })
    }

    /// The files the recipe reads, each by the name it gives it, to be
    /// written beside `fixture.json`.
    pub fn files(&self) -> impl Iterator<Item = (&str, &[u8])> {
        self.files.iter().map(|f| (f.name.as_str(), &*f.bytes))
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

/// A recipe being built: its steps, the files they read, and the factor
/// every length is written at (1 in metres, 1000 in millimetres).
struct Out {
    steps: Vec<Step>,
    files: Vec<RecipeFile>,
    scale: f64,
}

impl Out {
    fn len(&self, x: f64) -> f64 {
        x * self.scale
    }

    fn point(&self, p: DVec3) -> [f64; 3] {
        (p * self.scale).to_array()
    }

    /// The step (two for a sweep: its profile first) that remakes
    /// `record`'s body; none for a query, which makes no body.
    fn push(&mut self, call: CallIndex, record: &KernelCall) -> Result<(), FixtureError> {
        let name = name(call);
        let operand = |i: usize| self::name(record.sources[i]);
        let blend_edges = |edges: &[_], at: &[DVec3]| {
            if at.len() == edges.len() {
                Ok(at.iter().map(|&p| self.point(p)).collect())
            } else {
                Err(FixtureError::EdgeWithoutPoint(call))
            }
        };
        let step = match &record.op {
            KernelOp::Extrude {
                profile, distance, ..
            } => {
                let profile_name = format!("{name}.profile");
                let profile_step = self.profile_step(&profile_name, profile);
                self.steps.push(profile_step);
                Step::Extrude {
                    name,
                    profile: profile_name,
                    direction: (profile.plane().z_axis() * distance.signum()).to_array(),
                    length: self.len(distance.abs()),
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
                let profile_step = self.profile_step(&profile_name, profile);
                self.steps.push(profile_step);
                Step::Revolve {
                    name,
                    profile: profile_name,
                    axis: Axis {
                        origin: self.point(*axis_origin),
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
                radius: self.len(*radius),
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
                distance: self.len(*distance),
            },
            // Arris's transform turns first, about `origin`, then translates:
            // a `Motion`'s order.
            KernelOp::Copy { motion, .. } => Step::Transform {
                name,
                of: operand(0),
                translate: (motion.translation != DVec3::ZERO)
                    .then(|| self.point(motion.translation)),
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
                    origin: self.point(*origin),
                    normal: normal.to_array(),
                },
            },
            KernelOp::ImportStep {
                file,
                entity,
                bytes,
                ..
            } => {
                let entity = entity.ok_or(FixtureError::NoFileSolid(call))?;
                if entity.instance > 0 {
                    return Err(FixtureError::Placement(call));
                }
                let bytes = bytes.0.clone().ok_or(FixtureError::NoFile(call))?;
                let file_name = format!("{}.step", &file.as_str()[..12]);
                let sha256 = sha256_hex(&bytes);
                if !self.files.iter().any(|f| f.name == file_name) {
                    self.files.push(RecipeFile {
                        name: file_name.clone(),
                        bytes,
                    });
                }
                Step::Read {
                    name,
                    file: file_name,
                    sha256,
                    id: entity.id,
                }
            }
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
        self.steps.push(step);
        Ok(())
    }

    fn profile_step(&self, name: &str, profile: &Profile) -> Step {
        let plane = profile.plane();
        Step::Profile {
            name: name.to_owned(),
            plane: Plane {
                origin: self.point(plane.origin()),
                x: plane.x_axis().to_array(),
                y: plane.y_axis().to_array(),
            },
            outer: self.profile_loop(profile.outer()),
            holes: profile
                .holes()
                .iter()
                .map(|l| self.profile_loop(l))
                .collect(),
        }
    }

    fn profile_loop(&self, l: &ProfileLoop) -> Loop {
        let uv = |p: &DVec2| (*p * self.scale).to_array();
        match l {
            ProfileLoop::Circle { center, radius, .. } => Loop::Circle {
                circle: Circle {
                    center: uv(center),
                    radius: self.len(*radius),
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
}

/// The SHA-256 Arris's step step checks its file against, lower-case hex.
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    use std::fmt::Write as _;
    let mut hex = String::with_capacity(64);
    for b in sha2::Sha256::digest(bytes) {
        write!(hex, "{b:02x}").expect("a String takes any text");
    }
    hex
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
        KernelOp::ImportStep { .. } => "import_step",
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
