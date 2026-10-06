//! `core.pattern-linear` and `core.pattern-circular` (docs/DATA-MODEL.md
//! §The core feature types): copies of the tools of features that join or
//! cut, applied to a `target` body with each source's own operation, or,
//! with no tools, copies of the target itself fused into it (ADR-0013).
//! Copy `i` is instance `i` of the feature, from 1; instance 0 is the
//! source's own work, so a pattern of `count` makes `count - 1` copies.

use arrix_core::{DVec3, Diagnostic, Motion, QuantityKind, Severity, SlotName};
use arrix_plugin_api::{
    Body, FeatureOutput, FeatureTypeSpec, InputKind, InputSpec, InputValue, Kernel, OutputValue,
    ParamSpec, SlotKind, SlotOutput, SlotSpec, Tool, ToolOp,
};

use crate::registry::{FeatureArgs, FeatureType, TypeOutput};

/// The most copies a pattern makes: a count far past any part, caught
/// before the kernel is asked for them.
const MOST: f64 = 1000.0;

/// What a copy must change, as a fraction of the body's volume or area,
/// to count as having met the target at all.
const MISS: f64 = 1e-9;

fn error(code: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Severity::Error, code, message).expect("a literal code is well-formed")
}

/// The sources a pattern or a mirror copies, from its `tools` inputs: each
/// tool with its own operation, or, with none, the target itself fused.
pub(crate) fn sources<'a>(args: &'a FeatureArgs, target: &'a Body) -> Vec<(&'a Body, ToolOp)> {
    let tools: Vec<(&Body, ToolOp)> = args
        .inputs_named("tools")
        .filter_map(|i| match &i.value {
            InputValue::Tool(Tool { body, op }) => Some((body, *op)),
            _ => None,
        })
        .collect();
    if tools.is_empty() {
        vec![(target, ToolOp::Fuse)]
    } else {
        tools
    }
}

/// Makes copy `i` of a source: `(kernel, i, source)`.
pub(crate) type Copier<'a> = dyn Fn(&mut dyn Kernel, u32, &Body) -> Result<Body, Diagnostic> + 'a;

/// `target` with every source copied `copies` times (`make(kernel, i,
/// source)` is copy `i` from 1) and applied, one boolean per copy in the
/// order source by source, copy by copy, so a copy added at the end
/// renames none that stays (ADR-0014). A copy that does not meet the body
/// is `<ty>.misses`.
pub(crate) fn apply(
    kernel: &mut dyn Kernel,
    ty: &str,
    target: &Body,
    sources: &[(&Body, ToolOp)],
    copies: u32,
    make: &Copier<'_>,
) -> Result<Body, Diagnostic> {
    let mut result: Option<Body> = None;
    for (source, op) in sources {
        for at in 1..=copies {
            let copy = make(kernel, at, source)?;
            let from = result.as_ref().unwrap_or(target);
            let (was, tool) = (kernel.measure(from)?, kernel.measure(&copy)?);
            let next = match op {
                ToolOp::Fuse => kernel.fuse(from, &copy)?,
                ToolOp::Cut => kernel.cut(from, &copy)?,
            };
            let now = kernel.measure(&next)?;
            let met = match op {
                ToolOp::Cut => was.volume - now.volume > was.volume * MISS,
                // Disjoint solids fuse into the sum of both, in volume and
                // in area; a copy that overlaps or touches loses some.
                ToolOp::Fuse => {
                    (now.volume - was.volume - tool.volume).abs() > was.volume * MISS
                        || (now.area - was.area - tool.area).abs() > was.area * MISS
                }
            };
            if !met {
                return Err(error(
                    &format!("{ty}.misses"),
                    format!("copy {at} does not meet the body"),
                ));
            }
            result = Some(next);
        }
    }
    Ok(result.unwrap_or_else(|| unreachable!("a pattern or mirror makes a copy")))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pattern {
    Linear,
    Circular,
}

/// `core.pattern-linear`: `count` instances `spacing` apart along its
/// `direction`, or `core.pattern-circular`: `count` instances about its
/// `axis`, spread over `angle`.
pub struct CorePattern {
    pattern: Pattern,
    spec: FeatureTypeSpec,
}

impl CorePattern {
    pub fn linear() -> Self {
        Self::new(Pattern::Linear)
    }

    pub fn circular() -> Self {
        Self::new(Pattern::Circular)
    }

    fn new(pattern: Pattern) -> Self {
        let (id, title, along) = match pattern {
            Pattern::Linear => ("core.pattern-linear", "Linear pattern", "direction"),
            Pattern::Circular => ("core.pattern-circular", "Circular pattern", "axis"),
        };
        let mut params = vec![ParamSpec {
            name: "count".into(),
            title: "Count".into(),
            kind: QuantityKind::Count,
            default: "3".into(),
        }];
        params.push(match pattern {
            Pattern::Linear => ParamSpec {
                name: "spacing".into(),
                title: "Spacing".into(),
                kind: QuantityKind::Length,
                default: "10 mm".into(),
            },
            Pattern::Circular => ParamSpec {
                name: "angle".into(),
                title: "Angle".into(),
                kind: QuantityKind::Angle,
                default: "360 deg".into(),
            },
        });
        let input = |name: &str, title: &str, kind, list| InputSpec {
            name: name.into(),
            title: title.into(),
            kind,
            list,
        };
        Self {
            pattern,
            spec: FeatureTypeSpec {
                id: id.into(),
                version: 1,
                title: title.into(),
                params,
                inputs: vec![
                    input("target", "Body to pattern onto", InputKind::Body, false),
                    input("tools", "Features to pattern", InputKind::Tool, true),
                    input(along, "Direction or axis", InputKind::Axis, false),
                ],
                outputs: vec![SlotSpec {
                    name: SlotName::new("body").expect("a literal slot name"),
                    kind: SlotKind::Body,
                    modifies: Some("target".into()),
                }],
            },
        }
    }

    /// The placements of copies 1 to `count - 1`.
    fn placements(&self, args: &FeatureArgs, count: u32) -> Result<Vec<Motion>, Diagnostic> {
        let copies = 1..count;
        match self.pattern {
            Pattern::Linear => {
                let Some(InputValue::Axis(line)) = args.input("direction").map(|i| &i.value) else {
                    return Err(error(
                        "pattern.no-direction",
                        "a linear pattern needs a direction: a sketch line or a straight edge",
                    ));
                };
                let spacing = args.param("spacing").unwrap_or(0.0);
                if spacing == 0.0 {
                    return Err(error("pattern.spacing", "a pattern's spacing is not zero"));
                }
                let step = line.direction.normalize_or_zero() * spacing;
                Ok(copies
                    .map(|i| Motion::translation(step * f64::from(i)))
                    .collect())
            }
            Pattern::Circular => {
                let Some(InputValue::Axis(axis)) = args.input("axis").map(|i| &i.value) else {
                    return Err(error(
                        "pattern.no-axis",
                        "a circular pattern needs an axis: a sketch line or a straight edge",
                    ));
                };
                let angle = args.param("angle").unwrap_or(0.0);
                if angle == 0.0 {
                    return Err(error("pattern.angle", "a pattern's angle is not zero"));
                }
                // A full turn is shared out among all the instances; a
                // part of one runs from the first instance to the last.
                let full = angle.abs() >= std::f64::consts::TAU - 1e-9;
                let spans = if full { count } else { count - 1 };
                let step = angle / f64::from(spans.max(1));
                let (about, direction) = (axis.origin, axis.direction.normalize_or_zero());
                Ok(copies
                    .map(|i| {
                        let turn = step * f64::from(i);
                        Motion {
                            translation: about - rotate(about, direction, turn),
                            axis: direction,
                            angle: turn,
                        }
                    })
                    .collect())
            }
        }
    }
}

/// `v` turned `angle` about the axis through the origin along the unit
/// `axis` (Rodrigues).
fn rotate(v: DVec3, axis: DVec3, angle: f64) -> DVec3 {
    let (sin, cos) = angle.sin_cos();
    v * cos + axis.cross(v) * sin + axis * (axis.dot(v) * (1.0 - cos))
}

impl FeatureType for CorePattern {
    fn spec(&self) -> &FeatureTypeSpec {
        &self.spec
    }

    fn evaluate(
        &self,
        kernel: &mut dyn Kernel,
        args: &FeatureArgs,
    ) -> Result<TypeOutput, Diagnostic> {
        if let Some(other) = args.choices.keys().next() {
            return Err(error(
                "feature.unknown-choice",
                format!("a pattern has no choice `{other}`"),
            ));
        }
        let Some(InputValue::Body(target)) = args.input("target").map(|i| &i.value) else {
            return Err(error(
                "pattern.no-target",
                "a pattern needs the body to pattern onto",
            ));
        };
        let count = args.param("count").unwrap_or(0.0);
        if !(1.0..=MOST).contains(&count) {
            return Err(error(
                "pattern.count",
                format!("a pattern has between 1 and {MOST} instances"),
            ));
        }
        // Bounded by `MOST` above.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let placements = self.placements(args, count as u32)?;
        if placements.is_empty() {
            // One instance is the source's own work.
            return Err(error(
                "pattern.count",
                "a pattern of one instance copies nothing",
            ));
        }
        let sources = sources(args, target);
        let copies = u32::try_from(placements.len()).expect("bounded by `MOST`");
        let body = apply(
            kernel,
            "pattern",
            target,
            &sources,
            copies,
            &|k, at, source| k.copy(at, source, &placements[at as usize - 1]),
        )?;
        Ok(FeatureOutput {
            slots: vec![SlotOutput {
                name: self.spec.outputs[0].name.clone(),
                value: OutputValue::Body(body),
            }],
            tool: None,
        }
        .into())
    }
}
