//! The assembly pass (ADR-0029 §3 to §5 and §11; docs/DATA-MODEL.md
//! §Evaluation): after the parts, each assembly in the DAG's order, nested
//! ones first. An anchor resolves against its part's final bodies, its
//! frame is a recorded kernel call cached by input hash, and forward
//! kinematics places every component. Fail soft: a lost anchor, a loop or
//! a missing placement is a diagnostic, and every other result stands.

use std::collections::BTreeMap;

use arrix_core::{
    AssemblyId, ComponentId, DMat3, DVec3, Diagnostic, Frame, JointId, PersistentName,
    QuantityKind, Ref, Severity, TopoKind,
};
use arrix_kernel::ARRIS_VERSION;
use serde::{Deserialize, Serialize};

use super::hash::InputHash;
use super::host::kernel_diagnostic;
use super::meshing::FinalBody;
use super::resolve::{Params, candidates};
use super::{Evaluator, interrupted};
use crate::assembly::{Anchor, Assembly, ComponentOf, Joint, JointKind, Pose};
use crate::dag::{Dag, Node, anchor_target};
use crate::document::Document;

/// An assembly as the evaluator solved it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct AssemblyOutcome {
    /// Over everything the poses are a function of: the components' poses
    /// and nested assemblies, and each joint's values and anchors (the
    /// version of the body each names, ADR-0029 §2).
    pub hash: InputHash,
    /// Reused from the cache: nothing it reads changed.
    pub cached: bool,
    /// Every component at every depth, by its path from this assembly, in
    /// id order, each nested assembly's after its own component.
    pub components: Vec<PlacedComponent>,
    pub joints: BTreeMap<JointId, JointOutcome>,
}

impl AssemblyOutcome {
    /// The pose of the component at `path`.
    pub fn pose(&self, path: &[ComponentId]) -> Option<Frame> {
        self.components
            .iter()
            .find(|c| c.path == path)
            .map(|c| c.pose)
    }
}

/// A component where the assembly places it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PlacedComponent {
    pub path: Vec<ComponentId>,
    /// Where the origin and axes of its part or nested assembly land, in
    /// the frame of the assembly the path starts at.
    pub pose: Frame,
    /// Why one of this assembly's own components is not where it was
    /// authored to be: `assembly.unplaced`, or its pose's expression. A
    /// nested component's are its own assembly's.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,
}

/// How one joint's solve ended.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum JointOutcome {
    Ok {
        /// `value + offset` in SI, radians or metres; 0 for a fixed joint.
        value: f64,
        /// The anchors' frames in the assembly: the motion carries the
        /// parent's into the child's.
        parent: Frame,
        child: Frame,
        /// `assembly.limit`, when the value is outside its limits.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        diagnostics: Vec<Diagnostic>,
    },
    /// The joint places nothing; the child it would have placed is
    /// `assembly.unplaced`.
    Failed { diagnostic: Diagnostic },
}

fn diagnostic(severity: Severity, code: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(severity, code, message).expect("a literal code is well-formed")
}

fn error(code: &str, message: impl Into<String>) -> Diagnostic {
    diagnostic(Severity::Error, code, message)
}

/// A proper rigid motion as a rotation and a translation: `x ↦ r·x + t`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Rigid {
    r: DMat3,
    t: DVec3,
}

impl Rigid {
    pub(super) const IDENTITY: Rigid = Rigid {
        r: DMat3::IDENTITY,
        t: DVec3::ZERO,
    };

    /// The motion taking the world's frame onto `frame`.
    pub(super) fn of(frame: &Frame) -> Self {
        Self {
            r: DMat3::from_cols(frame.x_axis(), frame.y_axis(), frame.z_axis()),
            t: frame.origin(),
        }
    }

    pub(super) fn frame(self) -> Frame {
        Frame::new(self.t, self.r.x_axis, self.r.z_axis)
            .expect("a rigid motion's axes make a frame")
    }

    /// `self` after `then`: `then` applied first.
    pub(super) fn then(self, then: Rigid) -> Rigid {
        Rigid {
            r: self.r * then.r,
            t: self.r * then.t + self.t,
        }
    }

    pub(super) fn inverse(self) -> Rigid {
        let r = self.r.transpose();
        Rigid {
            r,
            t: -(r * self.t),
        }
    }

    fn rotation(r: DMat3) -> Rigid {
        Rigid { r, t: DVec3::ZERO }
    }

    /// A pose: the position, then URDF's fixed-axis roll, pitch and yaw,
    /// `Rz(yaw)·Ry(pitch)·Rx(roll)`.
    pub(super) fn pose([x, y, z, roll, pitch, yaw]: [f64; 6]) -> Rigid {
        Rigid {
            r: DMat3::from_rotation_z(yaw)
                * DMat3::from_rotation_y(pitch)
                * DMat3::from_rotation_x(roll),
            t: DVec3::new(x, y, z),
        }
    }
}

/// An anchor found on its part's final body, ready for its frame.
#[derive(Serialize)]
struct Ready<'a> {
    path: &'a [ComponentId],
    /// The frame's cache key: the body's version and the names.
    key: InputHash,
    #[serde(skip)]
    top: ComponentId,
    /// The anchored part's pose in the top component.
    #[serde(skip)]
    inner: Rigid,
    #[serde(skip)]
    body: arrix_kernel::KernelBody,
    #[serde(skip)]
    axis: &'a PersistentName,
    #[serde(skip)]
    origin: Option<&'a PersistentName>,
}

/// A joint's values in SI.
#[derive(Clone, Copy, Serialize)]
struct Values {
    value: f64,
    offset: f64,
    limits: Option<[f64; 2]>,
}

/// A joint as the pass reads it, before any kernel call: all its hash
/// covers.
#[derive(Serialize)]
struct Prepared<'a> {
    kind: JointKind,
    flip: bool,
    values: Result<Values, Diagnostic>,
    parent: Result<Ready<'a>, Diagnostic>,
    child: Result<Ready<'a>, Diagnostic>,
}

/// A component as the hash covers it.
#[derive(Serialize)]
struct ComponentInput<'a> {
    of: ComponentOf,
    /// A nested assembly's own hash.
    nested: Option<InputHash>,
    pose: Option<&'a Result<[f64; 6], Diagnostic>>,
}

#[derive(Serialize)]
struct Inputs<'a> {
    arris: &'a str,
    assembly: AssemblyId,
    components: Vec<(ComponentId, ComponentInput<'a>)>,
    /// A joint that is a loop by its structure alone is its diagnostic.
    joints: Vec<(JointId, &'a Result<Prepared<'a>, Diagnostic>)>,
}

/// A joint whose frames are known: what forward kinematics needs of it.
struct Solvable {
    value: f64,
    parent_top: ComponentId,
    /// The parent anchor's frame in its top component.
    parent_local: Rigid,
    child_top: ComponentId,
    /// The child anchor's frame in its top component, flipped if asked.
    child_local: Rigid,
    motion: Rigid,
    diagnostics: Vec<Diagnostic>,
}

/// What the cache keeps of one solve.
pub(super) struct Solved {
    outcome: AssemblyOutcome,
    pub(super) used: u64,
}

/// What a kernel call stopped by the interrupt flag leaves: nothing.
pub(super) struct Stopped;

impl Evaluator {
    /// Solves every assembly in the DAG's order, nested ones first; stopped
    /// when a frame's kernel call was interrupted.
    pub(super) fn assemblies(
        &mut self,
        doc: &Document,
        dag: &Dag,
        params: &Params,
        finals: &[FinalBody],
    ) -> Result<BTreeMap<AssemblyId, AssemblyOutcome>, Stopped> {
        let mut out = BTreeMap::new();
        for node in dag.order() {
            let Node::Assembly(id) = node else { continue };
            let outcome = self.assembly(doc, &doc.assemblies[id], params, finals, &out)?;
            out.insert(*id, outcome);
        }
        Ok(out)
    }

    fn assembly(
        &mut self,
        doc: &Document,
        assembly: &Assembly,
        params: &Params,
        finals: &[FinalBody],
        nested: &BTreeMap<AssemblyId, AssemblyOutcome>,
    ) -> Result<AssemblyOutcome, Stopped> {
        let poses: BTreeMap<ComponentId, Result<[f64; 6], Diagnostic>> = assembly
            .components
            .values()
            .filter_map(|c| Some((c.id, pose_values(params, c.pose.as_ref()?))))
            .collect();
        let loops = loops(assembly);
        let joints: Vec<(JointId, Result<Prepared, Diagnostic>)> = assembly
            .joints
            .values()
            .map(|j| {
                let prepared = match loops.get(&j.id) {
                    Some(d) => Err(d.clone()),
                    None => Ok(self.prepare(doc, assembly, j, params, finals, nested)),
                };
                (j.id, prepared)
            })
            .collect();
        let hash = InputHash::of_canonical(&Inputs {
            arris: ARRIS_VERSION,
            assembly: assembly.id,
            components: assembly
                .components
                .values()
                .map(|c| {
                    let nested = match c.of {
                        ComponentOf::Assembly(a) => nested.get(&a).map(|o| o.hash),
                        ComponentOf::Part(_) => None,
                    };
                    let input = ComponentInput {
                        of: c.of,
                        nested,
                        pose: poses.get(&c.id),
                    };
                    (c.id, input)
                })
                .collect(),
            joints: joints.iter().map(|(id, j)| (*id, j)).collect(),
        });
        if let Some(solved) = self.solved.get_mut(&hash) {
            solved.used = self.run;
            let mut outcome = solved.outcome.clone();
            outcome.cached = true;
            return Ok(outcome);
        }
        let mut outcomes = BTreeMap::new();
        let mut solvable = BTreeMap::new();
        for (id, prepared) in joints {
            match self.frames(prepared)? {
                Ok(s) => {
                    solvable.insert(id, s);
                }
                Err(diagnostic) => {
                    outcomes.insert(id, JointOutcome::Failed { diagnostic });
                }
            }
        }
        let outcome = AssemblyOutcome {
            hash,
            cached: false,
            components: solve(assembly, &poses, solvable, &mut outcomes, nested),
            joints: outcomes,
        };
        self.solved.insert(
            hash,
            Solved {
                outcome: outcome.clone(),
                used: self.run,
            },
        );
        Ok(outcome)
    }

    /// A joint's values and anchors, without a kernel call.
    fn prepare<'a>(
        &self,
        doc: &Document,
        assembly: &Assembly,
        joint: &'a Joint,
        params: &Params,
        finals: &[FinalBody],
        nested: &BTreeMap<AssemblyId, AssemblyOutcome>,
    ) -> Prepared<'a> {
        Prepared {
            kind: joint.kind,
            flip: joint.flip,
            values: joint_values(params, joint),
            parent: self.ready(doc, assembly, &joint.parent, finals, nested),
            child: self.ready(doc, assembly, &joint.child, finals, nested),
        }
    }

    /// The anchor's part, its pose in the top component and the one final
    /// body both its names are on.
    fn ready<'a>(
        &self,
        doc: &Document,
        assembly: &Assembly,
        anchor: &'a Anchor,
        finals: &[FinalBody],
        nested: &BTreeMap<AssemblyId, AssemblyOutcome>,
    ) -> Result<Ready<'a>, Diagnostic> {
        let topo = |name: &PersistentName| Ref::Topo(name.clone());
        if anchor.axis.kind == TopoKind::Vertex {
            return Err(error(
                "input.wrong-kind",
                format!(
                    "{} is a vertex; an anchor's axis is a face or an edge",
                    anchor.axis
                ),
            )
            .with_refs([topo(&anchor.axis)]));
        }
        if let Some(origin) = anchor.origin.as_ref().filter(|o| o.kind != TopoKind::Face) {
            return Err(error(
                "input.wrong-kind",
                format!("{origin} is not a face; an anchor's origin is a planar face"),
            )
            .with_refs([topo(origin)]));
        }
        let path: Vec<String> = anchor.path.iter().map(ToString::to_string).collect();
        let Some((_, part)) = anchor_target(doc, assembly.id, &anchor.path) else {
            return Err(error(
                "ref.lost",
                format!(
                    "the path [{}] leads to no part of assembly {}",
                    path.join(", "),
                    assembly.name
                ),
            ));
        };
        let top = anchor.path[0];
        let inner = match (assembly.components[&top].of, &anchor.path[1..]) {
            (ComponentOf::Assembly(a), rest) if !rest.is_empty() => nested
                .get(&a)
                .and_then(|o| o.pose(rest))
                .map_or(Rigid::IDENTITY, |f| Rigid::of(&f)),
            _ => Rigid::IDENTITY,
        };
        let pool: Vec<&FinalBody> = finals.iter().filter(|b| b.part == part).collect();
        if pool.is_empty() {
            return Err(error(
                "input.unavailable",
                format!("part {part} has no body for {} to name", anchor.axis),
            )
            .with_refs([topo(&anchor.axis)]));
        }
        let body = self.find(&pool, &anchor.axis)?;
        if let Some(origin) = &anchor.origin
            && self.find(&pool, origin)?.body != body.body
        {
            return Err(error(
                "ref.lost",
                format!("{origin} is on another body than {}", anchor.axis),
            )
            .with_refs([topo(origin)]));
        }
        let key = InputHash::of_canonical(&(
            ARRIS_VERSION,
            body.hash,
            &body.slot,
            &anchor.axis,
            &anchor.origin,
        ));
        Ok(Ready {
            path: &anchor.path,
            key,
            top,
            inner,
            body: body.body,
            axis: &anchor.axis,
            origin: anchor.origin.as_ref(),
        })
    }

    /// The one body of `pool` that has `name`; none, or more than one, is
    /// `ref.lost` with the nearest names of the part as candidates.
    fn find<'b>(
        &self,
        pool: &[&'b FinalBody],
        name: &PersistentName,
    ) -> Result<&'b FinalBody, Diagnostic> {
        let names = |b: &FinalBody| self.kernel.names(b.body).map_err(|e| kernel_diagnostic(&e));
        let mut matches = Vec::new();
        for b in pool {
            if names(b)?.contains(name) {
                matches.push(*b);
            }
        }
        if let [one] = matches[..] {
            return Ok(one);
        }
        let mut all = Vec::new();
        for b in pool {
            all.extend(names(b)?.of_kind(name.kind).cloned());
        }
        let why = if matches.is_empty() {
            "matches nothing in its part's final bodies"
        } else {
            "matches more than one of its part's final bodies"
        };
        Err(error("ref.lost", format!("{name} {why}"))
            .with_refs([Ref::Topo(name.clone())])
            .with_candidates(candidates(name, &all)))
    }

    /// The anchors' frames, from the cache or the kernel; `Stopped` when a
    /// call was interrupted.
    fn frames(
        &mut self,
        prepared: Result<Prepared, Diagnostic>,
    ) -> Result<Result<Solvable, Diagnostic>, Stopped> {
        let prepared = match prepared {
            Ok(p) => p,
            Err(d) => return Ok(Err(d)),
        };
        let (values, parent, child) = match (prepared.values, prepared.parent, prepared.child) {
            (Ok(v), Ok(p), Ok(c)) => (v, p, c),
            (Err(d), _, _) | (_, Err(d), _) | (_, _, Err(d)) => return Ok(Err(d)),
        };
        let parent_frame = match self.frame(&parent)? {
            Ok(f) => f,
            Err(d) => return Ok(Err(d)),
        };
        let child_frame = match self.frame(&child)? {
            Ok(f) => f,
            Err(d) => return Ok(Err(d)),
        };
        let mut child_local = child.inner.then(Rigid::of(&child_frame));
        if prepared.flip {
            child_local = child_local.then(Rigid::rotation(DMat3::from_rotation_x(
                std::f64::consts::PI,
            )));
        }
        let q = values.value + values.offset;
        let (value, motion) = match prepared.kind {
            JointKind::Fixed => (0.0, Rigid::IDENTITY),
            JointKind::Revolute => (q, Rigid::rotation(DMat3::from_rotation_z(q))),
            JointKind::Prismatic => (
                q,
                Rigid {
                    r: DMat3::IDENTITY,
                    t: DVec3::Z * q,
                },
            ),
        };
        let mut diagnostics = Vec::new();
        if let Some([lo, hi]) = values.limits
            && !(lo..=hi).contains(&values.value)
        {
            diagnostics.push(diagnostic(
                Severity::Warning,
                "assembly.limit",
                format!(
                    "the value {} is outside the limits [{lo}, {hi}]; it is applied as given",
                    values.value
                ),
            ));
        }
        Ok(Ok(Solvable {
            value,
            parent_top: parent.top,
            parent_local: parent.inner.then(Rigid::of(&parent_frame)),
            child_top: child.top,
            child_local,
            motion,
            diagnostics,
        }))
    }

    /// One anchor's frame on its part, cached by its key.
    fn frame(&mut self, ready: &Ready) -> Result<Result<Frame, Diagnostic>, Stopped> {
        if let Some((frame, used)) = self.frames.get_mut(&ready.key) {
            *used = self.run;
            return Ok(Ok(*frame));
        }
        match self
            .kernel
            .anchor_frame(ready.body, ready.axis, ready.origin)
        {
            Ok(frame) => {
                self.frames.insert(ready.key, (frame, self.run));
                Ok(Ok(frame))
            }
            Err(e) => {
                let d = kernel_diagnostic(&e).with_refs([Ref::Topo(ready.axis.clone())]);
                if interrupted(&d) {
                    return Err(Stopped);
                }
                Ok(Err(d))
            }
        }
    }
}

fn pose_values(params: &Params, pose: &Pose) -> Result<[f64; 6], Diagnostic> {
    let mut out = [0.0; 6];
    let kinds = [QuantityKind::Length; 3]
        .into_iter()
        .chain([QuantityKind::Angle; 3]);
    for ((slot, expr), kind) in out
        .iter_mut()
        .zip(pose.position.iter().chain(&pose.rpy))
        .zip(kinds)
    {
        *slot = params.quantity(expr, kind)?.si;
    }
    Ok(out)
}

fn joint_values(params: &Params, joint: &Joint) -> Result<Values, Diagnostic> {
    let kind = match joint.kind {
        JointKind::Fixed => {
            return Ok(Values {
                value: 0.0,
                offset: 0.0,
                limits: None,
            });
        }
        JointKind::Revolute => QuantityKind::Angle,
        JointKind::Prismatic => QuantityKind::Length,
    };
    let si = |field: &str, e| {
        params
            .quantity(e, kind)
            .map(|q| q.si)
            .map_err(|mut d: Diagnostic| {
                d.message = format!("`{field}`: {}", d.message);
                d
            })
    };
    Ok(Values {
        value: si("value", &joint.value)?,
        offset: si("offset", &joint.offset)?,
        limits: match &joint.limits {
            Some([lo, hi]) => Some([si("limits", lo)?, si("limits", hi)?]),
            None => None,
        },
    })
}

/// The joints that are loops by structure alone (ADR-0029 §4): anchors on
/// one component, a child grounded by a pose, or a child an earlier joint
/// (by id) already places.
fn loops(assembly: &Assembly) -> BTreeMap<JointId, Diagnostic> {
    let mut placed_by = BTreeMap::new();
    let mut out = BTreeMap::new();
    for joint in assembly.joints.values() {
        let (Some(parent), Some(child)) = (joint.parent.path.first(), joint.child.path.first())
        else {
            continue;
        };
        let Some(component) = assembly.components.get(child) else {
            continue;
        };
        let why = if parent == child {
            format!("both its anchors are on component {}", component.name)
        } else if component.pose.is_some() {
            format!("component {} is grounded by its pose", component.name)
        } else if let Some(earlier) = placed_by.get(child) {
            format!(
                "component {} is already placed by joint {}",
                component.name, assembly.joints[earlier].name
            )
        } else {
            placed_by.insert(*child, joint.id);
            continue;
        };
        out.insert(
            joint.id,
            error(
                "assembly.loop",
                format!("joint {} closes a loop: {why}; it is ignored", joint.name),
            ),
        );
    }
    out
}

/// Forward kinematics (ADR-0029 §4): grounded components at their pose,
/// each joint's child at `parent · F_parent · motion · F_child⁻¹`, a
/// component with neither at the identity. Joints whose parents never
/// get placed close a loop through each other: the last by id is
/// `assembly.loop`, ignored, until the rest are placed.
fn solve(
    assembly: &Assembly,
    poses: &BTreeMap<ComponentId, Result<[f64; 6], Diagnostic>>,
    mut pending: BTreeMap<JointId, Solvable>,
    outcomes: &mut BTreeMap<JointId, JointOutcome>,
    nested: &BTreeMap<AssemblyId, AssemblyOutcome>,
) -> Vec<PlacedComponent> {
    let mut world: BTreeMap<ComponentId, Rigid> = BTreeMap::new();
    let mut notes: BTreeMap<ComponentId, Vec<Diagnostic>> = BTreeMap::new();
    let unplaced = |name: &str, why: &str| {
        diagnostic(
            Severity::Warning,
            "assembly.unplaced",
            format!("component {name} {why}; it sits at the assembly's origin"),
        )
    };
    for (id, pose) in poses {
        match pose {
            Ok(values) => {
                world.insert(*id, Rigid::pose(*values));
            }
            Err(d) => {
                world.insert(*id, Rigid::IDENTITY);
                notes.entry(*id).or_default().push(d.clone());
            }
        }
    }
    for c in assembly.components.values() {
        if c.pose.is_some() || pending.values().any(|s| s.child_top == c.id) {
            continue;
        }
        let failed = assembly.joints.values().find(|j| {
            j.child.path.first() == Some(&c.id)
                && matches!(outcomes.get(&j.id), Some(JointOutcome::Failed { .. }))
        });
        let why = match failed {
            Some(j) => format!("is placed by joint {}, which failed", j.name),
            None => "is neither grounded by a pose nor placed by a joint".to_owned(),
        };
        world.insert(c.id, Rigid::IDENTITY);
        notes.entry(c.id).or_default().push(unplaced(&c.name, &why));
    }
    while !pending.is_empty() {
        let ready: Vec<JointId> = pending
            .iter()
            .filter(|(_, s)| world.contains_key(&s.parent_top))
            .map(|(id, _)| *id)
            .collect();
        if ready.is_empty() {
            let (id, s) = pending.pop_last().expect("pending is not empty");
            let name = &assembly.joints[&id].name;
            let child = &assembly.components[&s.child_top].name;
            outcomes.insert(
                id,
                JointOutcome::Failed {
                    diagnostic: error(
                        "assembly.loop",
                        format!(
                            "joint {name} closes a loop: its parent is placed through its own child; it is ignored"
                        ),
                    ),
                },
            );
            world.insert(s.child_top, Rigid::IDENTITY);
            notes.entry(s.child_top).or_default().push(unplaced(
                child,
                &format!("is placed by joint {name}, which closes a loop"),
            ));
            continue;
        }
        for id in ready {
            let s = pending.remove(&id).expect("listed above");
            let parent = world[&s.parent_top].then(s.parent_local);
            let placed = parent.then(s.motion).then(s.child_local.inverse());
            world.insert(s.child_top, placed);
            outcomes.insert(
                id,
                JointOutcome::Ok {
                    value: s.value,
                    parent: parent.frame(),
                    child: placed.then(s.child_local).frame(),
                    diagnostics: s.diagnostics,
                },
            );
        }
    }
    let mut out = Vec::new();
    for c in assembly.components.values() {
        let at = world[&c.id];
        out.push(PlacedComponent {
            path: vec![c.id],
            pose: at.frame(),
            diagnostics: notes.remove(&c.id).unwrap_or_default(),
        });
        if let ComponentOf::Assembly(a) = c.of
            && let Some(inner) = nested.get(&a)
        {
            out.extend(inner.components.iter().map(|p| {
                PlacedComponent {
                    path: std::iter::once(c.id)
                        .chain(p.path.iter().copied())
                        .collect(),
                    pose: at.then(Rigid::of(&p.pose)).frame(),
                    diagnostics: Vec::new(),
                }
            }));
        }
    }
    out
}
