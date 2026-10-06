//! `core.hole` (docs/DATA-MODEL.md §The core feature types): holes at a
//! list of sketch points, simple, counterbored or countersunk, blind or
//! through, cut from the `target` body. Each hole is one revolved tool
//! along the inward normal of its sketch's plane and cut in turn, so what
//! a hole's faces are called depends on the hole's place in the list and
//! on nothing else (docs/DATA-MODEL.md §Persistent naming).

use arrix_core::{
    Axis, CurveKey, DVec2, DVec3, Diagnostic, Frame, Id, Profile, ProfileLoop, ProfileSegment,
    QuantityKind, Severity, SlotName,
};
use arrix_plugin_api::{
    Body, FeatureOutput, FeatureTypeSpec, InputKind, InputSpec, InputValue, Kernel, OutputValue,
    ParamSpec, SlotKind, SlotOutput, SlotSpec, Tool, ToolOp,
};

use crate::registry::{FeatureArgs, FeatureType, TypeOutput};

/// How far a hole's tool starts outside the sketch plane, so its end is
/// clear of the face it opens, and how far a through hole reaches.
const MARGIN: f64 = 1e-3;
const THROUGH: f64 = 1.0;

/// What the volume a hole removes must exceed, as a fraction of the
/// target's, to count as a cut at all.
const MISS: f64 = 1e-9;

/// `core.hole`: a `target` body and a list of `points`, a `diameter`, an
/// `extent` choice (`blind`, with a `depth`, the default, or `through`)
/// and a `kind` choice (`simple`, the default, `counterbore` with its
/// `cb-diameter` and `cb-depth`, or `countersink` with `cs-diameter` and
/// the included `cs-angle`).
pub struct CoreHole {
    spec: FeatureTypeSpec,
}

fn error(code: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Severity::Error, code, message).expect("a literal code is well-formed")
}

impl CoreHole {
    pub fn new() -> Self {
        let length = |name: &str, title: &str, default: &str| ParamSpec {
            name: name.into(),
            title: title.into(),
            kind: QuantityKind::Length,
            default: default.into(),
        };
        Self {
            spec: FeatureTypeSpec {
                id: "core.hole".into(),
                version: 1,
                title: "Hole".into(),
                params: vec![
                    length("diameter", "Diameter", "5 mm"),
                    length("depth", "Depth", "10 mm"),
                    length("cb-diameter", "Counterbore diameter", "10 mm"),
                    length("cb-depth", "Counterbore depth", "3 mm"),
                    length("cs-diameter", "Countersink diameter", "10 mm"),
                    ParamSpec {
                        name: "cs-angle".into(),
                        title: "Countersink angle".into(),
                        kind: QuantityKind::Angle,
                        default: "90 deg".into(),
                    },
                ],
                inputs: vec![
                    InputSpec {
                        name: "target".into(),
                        title: "Body to drill".into(),
                        kind: InputKind::Body,
                        list: false,
                    },
                    InputSpec {
                        name: "points".into(),
                        title: "Hole centres".into(),
                        kind: InputKind::Point,
                        list: true,
                    },
                ],
                outputs: vec![SlotSpec {
                    name: SlotName::new("body").expect("a literal slot name"),
                    kind: SlotKind::Body,
                    modifies: Some("target".into()),
                }],
            },
        }
    }
}

impl Default for CoreHole {
    fn default() -> Self {
        Self::new()
    }
}

/// The sizes of one hole, in metres and radians.
struct Shape {
    radius: f64,
    /// How deep the hole goes below the sketch plane.
    depth: f64,
    kind: Kind,
}

enum Kind {
    Simple,
    Counterbore { radius: f64, depth: f64 },
    Countersink { radius: f64, half_angle: f64 },
}

impl Shape {
    /// The half-section the hole is turned from, in (radius, depth)
    /// coordinates, from the axis at `-MARGIN` round to the axis at the
    /// bottom: a polygon of its corners.
    fn section(&self) -> Vec<DVec2> {
        let at = |u: f64, v: f64| DVec2::new(u, v);
        let top = -MARGIN;
        let mut corners = vec![at(0.0, top)];
        match self.kind {
            Kind::Simple => corners.push(at(self.radius, top)),
            Kind::Counterbore { radius, depth } => {
                corners.extend([at(radius, top), at(radius, depth), at(self.radius, depth)])
            }
            Kind::Countersink { radius, half_angle } => {
                let slope = half_angle.tan();
                corners.extend([
                    at(radius + MARGIN * slope, top),
                    at(self.radius, (radius - self.radius) / slope),
                ]);
            }
        }
        corners.extend([at(self.radius, self.depth), at(0.0, self.depth)]);
        corners
    }

    /// The tool of the hole at `at`, drilled along `down`, its profile's
    /// curves keyed by their place in the section so a hole moved keeps
    /// every name (the sweep index is the hole's place in the list).
    fn tool(&self, kernel: &mut dyn Kernel, at: DVec3, down: DVec3) -> Result<Body, Diagnostic> {
        // A frame whose v axis (Z × X) is `down` and whose u axis is any
        // perpendicular.
        let x = down.any_orthonormal_vector();
        let plane = Frame::new(at, x, x.cross(down))
            .map_err(|e| error("hole.frame", format!("a hole has no plane: {e}")))?;
        let corners = self.section();
        let segments = corners
            .iter()
            .cycle()
            .skip(1)
            .take(corners.len())
            .zip(1u64..)
            .map(|(to, n)| ProfileSegment::Line {
                key: CurveKey(Id(n)),
                to: *to,
            })
            .collect();
        let outer = ProfileLoop::Path {
            start: corners[0],
            segments,
        };
        let profile = Profile::new(plane, outer, vec![])
            .map_err(|e| error("hole.profile", format!("a hole has no profile: {e}")))?;
        kernel.revolve(
            &profile,
            &Axis {
                origin: at,
                direction: down,
            },
            std::f64::consts::TAU,
        )
    }
}

impl CoreHole {
    /// The shape the record's parameters and choices describe.
    fn shape(args: &FeatureArgs) -> Result<Shape, Diagnostic> {
        if let Some(other) = args
            .choices
            .keys()
            .find(|k| !matches!(k.as_str(), "extent" | "kind"))
        {
            return Err(error(
                "feature.unknown-choice",
                format!("a hole has no choice `{other}`"),
            ));
        }
        let param = |name: &str| args.param(name).unwrap_or(0.0);
        let radius = param("diameter") / 2.0;
        if radius <= 0.0 {
            return Err(error("hole.diameter", "a hole's diameter is positive"));
        }
        let depth = match args.choices.get("extent").map_or("blind", String::as_str) {
            "blind" if param("depth") > 0.0 => param("depth"),
            "blind" => return Err(error("hole.depth", "a blind hole's depth is positive")),
            "through" => THROUGH,
            other => {
                return Err(error(
                    "hole.extent",
                    format!("`{other}` is not a hole's extent: blind or through"),
                ));
            }
        };
        let kind = match args.choices.get("kind").map_or("simple", String::as_str) {
            "simple" => Kind::Simple,
            "counterbore" => {
                let (bore, bore_depth) = (param("cb-diameter") / 2.0, param("cb-depth"));
                if bore <= radius || bore_depth <= 0.0 || bore_depth >= depth {
                    return Err(error(
                        "hole.counterbore",
                        "a counterbore is wider than its hole and shallower than it",
                    ));
                }
                Kind::Counterbore {
                    radius: bore,
                    depth: bore_depth,
                }
            }
            "countersink" => {
                let (sink, angle) = (param("cs-diameter") / 2.0, param("cs-angle"));
                let half_angle = angle / 2.0;
                let reach = (sink - radius) / half_angle.tan();
                if sink <= radius
                    || !(angle > 0.0 && angle < std::f64::consts::PI)
                    || reach >= depth
                {
                    return Err(error(
                        "hole.countersink",
                        "a countersink is wider than its hole, under a flat angle, and shallower than it",
                    ));
                }
                Kind::Countersink {
                    radius: sink,
                    half_angle,
                }
            }
            other => {
                return Err(error(
                    "hole.kind",
                    format!("`{other}` is not a kind of hole: simple, counterbore or countersink"),
                ));
            }
        };
        Ok(Shape {
            radius,
            depth,
            kind,
        })
    }
}

impl FeatureType for CoreHole {
    fn spec(&self) -> &FeatureTypeSpec {
        &self.spec
    }

    fn evaluate(
        &self,
        kernel: &mut dyn Kernel,
        args: &FeatureArgs,
    ) -> Result<TypeOutput, Diagnostic> {
        let Some(InputValue::Body(target)) = args.input("target").map(|i| &i.value) else {
            return Err(error("hole.no-target", "a hole needs the body to drill"));
        };
        let points: Vec<_> = args
            .inputs_named("points")
            .filter_map(|i| match &i.value {
                InputValue::Point(p) => Some(*p),
                _ => None,
            })
            .collect();
        if points.is_empty() {
            return Err(error("hole.no-points", "a hole needs at least one point"));
        }
        let shape = Self::shape(args)?;
        // One cut per hole, in the list's order: a cut's boolean number is
        // the hole's place, so a hole added at the end renames no other
        // (ADR-0014). The tools are fused after, for a pattern to copy.
        let mut body: Option<Body> = None;
        let mut tools: Option<Body> = None;
        for point in &points {
            let Some(normal) = point.normal else {
                return Err(error(
                    "hole.no-normal",
                    "a hole is drilled from a sketch point, whose plane gives its direction",
                ));
            };
            let tool = shape.tool(kernel, point.at, -normal)?;
            let from = body.as_ref().unwrap_or(target);
            let before = kernel.measure(from)?.volume;
            let drilled = kernel.cut(from, &tool)?;
            if before - kernel.measure(&drilled)?.volume <= before * MISS {
                return Err(error(
                    "hole.misses",
                    "a hole cuts nothing: its point is not on the body",
                ));
            }
            body = Some(drilled);
            tools = Some(match tools {
                None => tool,
                Some(so_far) => kernel.fuse(&so_far, &tool)?,
            });
        }
        let (drilled, tools) = (
            body.expect("at least one point"),
            tools.expect("at least one point"),
        );
        Ok(FeatureOutput {
            slots: vec![SlotOutput {
                name: self.spec.outputs[0].name.clone(),
                value: OutputValue::Body(drilled),
            }],
            // Kept for a pattern or a mirror of the hole to copy
            // (ADR-0013).
            tool: Some(Tool {
                body: tools,
                op: ToolOp::Cut,
            }),
        }
        .into())
    }
}
