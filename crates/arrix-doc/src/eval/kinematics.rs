//! Forward kinematics (ADR-0029 §4), a pure function of the solved
//! frames: the evaluator's assembly pass and the app's joint drag share it
//! (ADR-0030 §3). No kernel call is involved.

use std::collections::BTreeMap;

use arrix_core::{AssemblyId, ComponentId, DMat3, DVec3, Diagnostic, Frame, JointId, Severity};

use super::assembly::{AssemblyOutcome, JointOutcome, PlacedComponent, diagnostic, error};
use crate::assembly::{Assembly, ComponentOf, JointKind};
use crate::document::Document;

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

    /// `p` carried by the motion.
    pub(super) fn point(self, p: DVec3) -> DVec3 {
        self.r * p + self.t
    }

    /// `v` turned by the motion, not moved.
    pub(super) fn vector(self, v: DVec3) -> DVec3 {
        self.r * v
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

    /// The half turn about x that `flip` turns an anchor by.
    pub(super) fn half_turn() -> Rigid {
        Rigid {
            r: DMat3::from_rotation_x(std::f64::consts::PI),
            t: DVec3::ZERO,
        }
    }

    /// A joint's motion at the coordinate `q` (radians or metres).
    pub(super) fn motion(kind: JointKind, q: f64) -> Rigid {
        match kind {
            JointKind::Fixed => Rigid::IDENTITY,
            JointKind::Revolute => Rigid {
                r: DMat3::from_rotation_z(q),
                t: DVec3::ZERO,
            },
            JointKind::Prismatic => Rigid {
                r: DMat3::IDENTITY,
                t: DVec3::Z * q,
            },
        }
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

/// A joint whose frames are known: what forward kinematics needs of it.
pub(super) struct Solvable {
    pub(super) value: f64,
    pub(super) parent_top: ComponentId,
    /// The parent anchor's frame in its top component.
    pub(super) parent_local: Rigid,
    pub(super) child_top: ComponentId,
    /// The child anchor's frame in its top component, flipped if asked.
    pub(super) child_local: Rigid,
    pub(super) motion: Rigid,
    pub(super) diagnostics: Vec<Diagnostic>,
}

/// Forward kinematics (ADR-0029 §4): grounded components at their pose,
/// each joint's child at `parent · F_parent · motion · F_child⁻¹`, a
/// component with neither at the identity. Joints whose parents never
/// get placed close a loop through each other: the last by id is
/// `assembly.loop`, ignored, until the rest are placed.
pub(super) fn solve(
    assembly: &Assembly,
    grounded: &BTreeMap<ComponentId, Result<Rigid, Diagnostic>>,
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
    for (id, pose) in grounded {
        match pose {
            Ok(at) => {
                world.insert(*id, *at);
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

/// The components of `assembly` placed again from its last `outcome`, the
/// joint coordinates in `overrides` (SI, `value + offset`: radians or
/// metres) standing in for the evaluated ones, with every other input as
/// the outcome had it: the anchors' frames, the grounded poses, the
/// nested assemblies' placements. Equal to evaluating the document with
/// those values, without a kernel call. `None` when the assembly is not in
/// the document or `outcome` is not its solve.
pub fn place(
    doc: &Document,
    assembly: AssemblyId,
    outcome: &AssemblyOutcome,
    nested: &BTreeMap<AssemblyId, AssemblyOutcome>,
    overrides: &[(JointId, f64)],
) -> Option<Vec<PlacedComponent>> {
    let assembly = doc.assemblies.get(&assembly)?;
    let top = |c: ComponentId| outcome.pose(&[c]).map(|f| Rigid::of(&f));
    let mut grounded = BTreeMap::new();
    for c in assembly.components.values().filter(|c| c.pose.is_some()) {
        grounded.insert(c.id, Ok(top(c.id)?));
    }
    let mut outcomes = BTreeMap::new();
    let mut pending = BTreeMap::new();
    for joint in assembly.joints.values() {
        match outcome.joints.get(&joint.id)? {
            failed @ JointOutcome::Failed { .. } => {
                outcomes.insert(joint.id, failed.clone());
            }
            JointOutcome::Ok {
                value,
                parent,
                child,
                diagnostics,
            } => {
                let (Some(&parent_top), Some(&child_top)) =
                    (joint.parent.path.first(), joint.child.path.first())
                else {
                    return None;
                };
                let q = overrides
                    .iter()
                    .rev()
                    .find(|(id, _)| *id == joint.id)
                    .map_or(*value, |(_, q)| *q);
                pending.insert(
                    joint.id,
                    Solvable {
                        value: if joint.kind == JointKind::Fixed {
                            0.0
                        } else {
                            q
                        },
                        parent_top,
                        parent_local: top(parent_top)?.inverse().then(Rigid::of(parent)),
                        child_top,
                        child_local: top(child_top)?.inverse().then(Rigid::of(child)),
                        motion: Rigid::motion(joint.kind, q),
                        diagnostics: diagnostics.clone(),
                    },
                );
            }
        }
    }
    Some(solve(assembly, &grounded, pending, &mut outcomes, nested))
}
