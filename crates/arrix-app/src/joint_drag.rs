//! The joint drag (ADR-0030 §3 and §4): which joint a left drag on a
//! component moves, how the pointer reads as that joint's value, the
//! preview by forward kinematics on the UI thread, and what the release
//! commits. No kernel call is involved: the preview composes the frames of
//! the last `JointOutcome::Ok` with a trial value through `arrix-doc`'s
//! pure FK, the evaluator's own. The app's side is `joint_drag_app.rs`.

use std::collections::BTreeMap;

use arrix_core::{AssemblyId, ComponentId, DMat3, DVec3, Frame, JointId, ParamId, QuantityKind};
use arrix_doc::expr::Expr;
use arrix_doc::{
    AssemblyOutcome, Command, ComponentOf, Document, JointKind, JointOutcome, PlacedComponent,
    Replica,
};
use arrix_viewport::{OrbitCamera, Projection};

use crate::joint_form::quantity;
use crate::sketch_view::plane_transform;

/// How far the pointer moves across a revolute axis seen edge-on to turn
/// it a radian, in points.
const POINTS_PER_RADIAN: f64 = 100.0;

/// Below this sine between the line of sight and a revolute's plane, the
/// plane is taken as edge-on and the angle is read on the screen.
const EDGE_ON: f64 = 0.2;

/// Above this cosine between the line of sight and a prismatic's axis, the
/// axis faces the eye and the drag is refused.
const FACING: f64 = 0.97;

/// The joint a drag moves: the one placing the deepest component of the
/// picked path that a joint places.
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    /// The components from the context's assembly down to the joint's own.
    pub prefix: Vec<ComponentId>,
    /// The assembly the joint is of.
    pub assembly: AssemblyId,
    pub joint: JointId,
    pub name: String,
    pub kind: JointKind,
}

/// The assembly the component at `path` from `root` is of, following
/// nested assemblies; `None` where the path leads nowhere.
fn assembly_at(doc: &Document, root: AssemblyId, path: &[ComponentId]) -> Option<AssemblyId> {
    let mut assembly = root;
    for id in path {
        match doc.assemblies().get(&assembly)?.components.get(id)?.of {
            ComponentOf::Assembly(a) => assembly = a,
            ComponentOf::Part(_) => return None,
        }
    }
    Some(assembly)
}

/// The joint a drag on the component at `path` (from `root`) moves, or why
/// none does, for the status line.
pub fn target(replica: &Replica, root: AssemblyId, path: &[ComponentId]) -> Result<Target, String> {
    let doc = replica.document();
    for depth in (0..path.len()).rev() {
        let prefix = &path[..depth];
        let Some(assembly) = assembly_at(doc, root, prefix) else {
            continue;
        };
        let Some(record) = doc.assemblies().get(&assembly) else {
            continue;
        };
        let Some(joint) = record
            .joints
            .values()
            .find(|j| j.child.path.first() == Some(&path[depth]))
        else {
            continue;
        };
        let outcome = replica
            .assembly(assembly)
            .and_then(|(_, o)| o.joints.get(&joint.id));
        return match outcome {
            Some(JointOutcome::Ok { .. }) if joint.kind == JointKind::Fixed => Err(format!(
                "Joint {} is fixed: it has no value to drag.",
                joint.name
            )),
            Some(JointOutcome::Ok { .. }) => Ok(Target {
                prefix: prefix.to_vec(),
                assembly,
                joint: joint.id,
                name: joint.name.clone(),
                kind: joint.kind,
            }),
            Some(JointOutcome::Failed { diagnostic }) => Err(format!(
                "Joint {} places nothing ({}): it does not drag.",
                joint.name, diagnostic.message
            )),
            None => Err(format!(
                "Joint {} is still being solved: drag it once it is placed.",
                joint.name
            )),
        };
    }
    let names = crate::context::component_names(doc, root, path);
    let name = names.last().map_or("It", String::as_str);
    let grounded = assembly_at(doc, root, &path[..path.len().saturating_sub(1)])
        .and_then(|a| doc.assemblies().get(&a)?.components.get(path.last()?))
        .is_some_and(|c| c.pose.is_some());
    Err(match grounded {
        true => format!("{name} is grounded by its pose: only a joint's value drags."),
        false => format!("{name} is placed by no joint: it does not drag."),
    })
}

/// What the joint's value is written as: what the release edits.
#[derive(Clone, Debug, PartialEq)]
pub enum Drives {
    /// A literal: the joint's `value` is replaced.
    Literal,
    /// One parameter's name: that parameter is edited (ADR-0030 §4).
    Param(ParamId),
}

/// A joint's numbers as the drag reads them, in SI: the value the last
/// solve used, the offset, the limits, and what the value is written as.
#[derive(Clone, Debug, PartialEq)]
pub struct Values {
    pub value: f64,
    pub offset: f64,
    pub limits: Option<[f64; 2]>,
    pub drives: Drives,
}

impl Values {
    /// The joint's numbers, or why it does not drag: a value that is an
    /// expression other than one parameter's name. A parameter resolves
    /// from the replica's last finished values.
    pub fn of(replica: &Replica, target: &Target) -> Result<Self, String> {
        let doc = replica.document();
        let joint = doc
            .assemblies()
            .get(&target.assembly)
            .and_then(|a| a.joints.get(&target.joint))
            .ok_or("the joint is no longer there")?;
        let Some(JointOutcome::Ok { value: q, .. }) = replica
            .assembly(target.assembly)
            .and_then(|(_, o)| o.joints.get(&target.joint))
        else {
            return Err(format!("Joint {} is not placed.", joint.name));
        };
        let params = doc.params();
        let finished = replica.finished().map(|(_, v)| v);
        let values = |name: &str| {
            let (pid, _) = params.iter().find(|(_, p)| p.name == name)?;
            finished?.get(pid)?.as_ref().ok().map(|q| q.si)
        };
        let si = |e: &Expr| e.evaluate(&values).ok();
        let names = joint.value.names();
        let drives = match names.iter().next() {
            None => Drives::Literal,
            Some(name) if names.len() == 1 && joint.value.text().trim() == *name => {
                let (pid, _) = params
                    .iter()
                    .find(|(_, p)| p.name == *name)
                    .ok_or_else(|| format!("no parameter is named `{name}`"))?;
                Drives::Param(*pid)
            }
            Some(_) => {
                return Err(format!(
                    "Joint {}'s value is the expression `{}`: edit it in the joint's form.",
                    joint.name,
                    joint.value.text()
                ));
            }
        };
        let offset = si(&joint.offset).ok_or("the joint's offset has no value")?;
        let limits = joint
            .limits
            .as_ref()
            .and_then(|[lo, hi]| Some([si(lo)?, si(hi)?]));
        Ok(Self {
            value: q - offset,
            offset,
            limits,
            drives,
        })
    }

    /// The value `delta` (SI) from where the drag started, kept inside the
    /// limits, as the text the release commits and that text's own value:
    /// a tenth of a degree, a hundredth of a millimetre.
    pub fn trial(&self, kind: JointKind, delta: f64) -> (String, f64) {
        let Some((qkind, unit)) = quantity(kind) else {
            return ("0".into(), 0.0);
        };
        let (scale, step) = match qkind {
            QuantityKind::Angle => (180.0 / std::f64::consts::PI, 0.1),
            _ => (1e3, 0.01),
        };
        let mut v = self.value + delta;
        if let Some([lo, hi]) = self.limits {
            v = v.clamp(lo.min(hi), hi.max(lo));
        }
        let mut shown = (v * scale / step).round() * step;
        // A limit off the display's step keeps the rounded value inside it.
        if let Some([lo, hi]) = self.limits {
            if shown > hi.max(lo) * scale {
                shown -= step;
            } else if shown < lo.min(hi) * scale {
                shown += step;
            }
        }
        let text = format!("{} {unit}", trim(shown, step));
        let si = Expr::parse(&text)
            .ok()
            .and_then(|e| e.evaluate(&|_| None).ok())
            .unwrap_or(v);
        (text, si)
    }
}

/// `x` printed to the step's places, without trailing zeros or `-0`.
fn trim(x: f64, step: f64) -> String {
    let places = (-step.log10()).round().max(0.0) as usize;
    let text = format!("{:.*}", places, x + 0.0);
    let text = match text.contains('.') {
        true => text.trim_end_matches('0').trim_end_matches('.').to_owned(),
        false => text,
    };
    match text.as_str() {
        "-0" => "0".into(),
        _ => text,
    }
}

/// The command a release commits for the trial `text`, none when the
/// value is where the drag started.
pub fn release(
    doc: &Document,
    target: &Target,
    values: &Values,
    text: &str,
    si: f64,
) -> Option<Command> {
    if (si - values.value).abs() <= f64::EPSILON * values.value.abs().max(1.0) {
        return None;
    }
    let expr = Expr::parse(text).ok()?;
    match values.drives {
        Drives::Literal => {
            let mut joint = doc
                .assemblies()
                .get(&target.assembly)?
                .joints
                .get(&target.joint)?
                .clone();
            joint.value = expr;
            Some(Command::EditJoint {
                assembly: target.assembly,
                joint: Box::new(joint),
            })
        }
        Drives::Param(param) => Some(Command::SetParam { param, expr }),
    }
}

/// The outcomes of the assemblies `assembly` nests, as the replica last
/// heard them.
fn nested(replica: &Replica, assembly: AssemblyId) -> BTreeMap<AssemblyId, AssemblyOutcome> {
    let doc = replica.document();
    doc.assemblies()
        .get(&assembly)
        .into_iter()
        .flat_map(|a| a.components.values())
        .filter_map(|c| match c.of {
            ComponentOf::Assembly(inner) => Some((inner, replica.assembly(inner)?.1.clone())),
            ComponentOf::Part(_) => None,
        })
        .collect()
}

/// Every component of `root` placed with the target's joint at the
/// coordinate `q` (`value + offset`, SI) and everything else as the last
/// solves had it: the joint's own assembly placed again, then each one
/// above it with that one's placements. A joint of an assembly above
/// anchored on the moved subtree keeps its last frame until the release's
/// evaluation.
pub fn preview(
    replica: &Replica,
    root: AssemblyId,
    target: &Target,
    q: f64,
) -> Option<Vec<PlacedComponent>> {
    let doc = replica.document();
    let chain: Vec<AssemblyId> = (0..=target.prefix.len())
        .map(|depth| assembly_at(doc, root, &target.prefix[..depth]))
        .collect::<Option<_>>()?;
    let (_, inner) = replica.assembly(target.assembly)?;
    let mut placed = arrix_doc::place(
        doc,
        target.assembly,
        inner,
        &nested(replica, target.assembly),
        &[(target.joint, q)],
    )?;
    for pair in chain.windows(2).rev() {
        let (outer, below) = (pair[0], pair[1]);
        let mut moved = replica.assembly(below)?.1.clone();
        moved.components = placed;
        let mut nested = nested(replica, outer);
        nested.insert(below, moved);
        placed = arrix_doc::place(doc, outer, replica.assembly(outer)?.1, &nested, &[])?;
    }
    Some(placed)
}

/// `frame`, given in a component's frame, in the frame `pose` places that
/// component at.
fn compose(pose: &Frame, frame: &Frame) -> Frame {
    let r = DMat3::from_cols(pose.x_axis(), pose.y_axis(), pose.z_axis());
    Frame::new(
        pose.origin() + r * frame.origin(),
        r * frame.x_axis(),
        r * frame.z_axis(),
    )
    .unwrap_or(*frame)
}

/// The target's parent anchor frame in `root`'s frame: the joint turns
/// about its z, or slides along it.
pub fn axis_frame(replica: &Replica, root: AssemblyId, target: &Target) -> Option<Frame> {
    let Some(JointOutcome::Ok { parent, .. }) = replica
        .assembly(target.assembly)?
        .1
        .joints
        .get(&target.joint)
    else {
        return None;
    };
    if target.prefix.is_empty() {
        return Some(*parent);
    }
    let pose = replica.assembly(root)?.1.pose(&target.prefix)?;
    Some(compose(&pose, parent))
}

/// How the pointer reads as a joint's coordinate.
#[derive(Clone, Debug)]
pub enum Reader {
    /// A revolute's plane of rotation: the pointer's angle about the axis,
    /// turned through since the start.
    Plane {
        plane: Frame,
        last: f64,
        turned: f64,
    },
    /// A revolute seen edge-on: the pointer's travel across the axis's
    /// image from where the drag started.
    Screen {
        start: egui::Pos2,
        across: egui::Vec2,
    },
    /// A prismatic's axis: the pointer's distance along it from the start.
    Axis { plane: Frame, start: f64 },
}

/// The direction the eye looks along at `p`.
fn sight(camera: &OrbitCamera, p: DVec3) -> DVec3 {
    match camera.projection {
        Projection::Orthographic => camera.basis().0.as_dvec3(),
        Projection::Perspective => (p - camera.eye().as_dvec3()).normalize_or_zero(),
    }
}

/// Where `at` lands in `plane`, in its (u, v) metres.
fn in_plane(
    camera: &OrbitCamera,
    rect: egui::Rect,
    plane: &Frame,
    at: egui::Pos2,
) -> Option<[f64; 2]> {
    plane_transform(camera, rect, plane)?.unproject(at)
}

impl Reader {
    /// The reader for a joint of `kind` about `axis` (its z), the drag
    /// started at `start`; or why the view cannot drag it.
    pub fn start(
        kind: JointKind,
        axis: &Frame,
        camera: &OrbitCamera,
        rect: egui::Rect,
        start: egui::Pos2,
        name: &str,
    ) -> Result<Self, String> {
        let z = axis.z_axis();
        let eye = sight(camera, axis.origin());
        match kind {
            JointKind::Revolute if z.dot(eye).abs() >= EDGE_ON => {
                let last = in_plane(camera, rect, axis, start)
                    .map(|[u, v]| v.atan2(u))
                    .ok_or("the pointer is not over the joint's plane")?;
                Ok(Self::Plane {
                    plane: *axis,
                    last,
                    turned: 0.0,
                })
            }
            JointKind::Revolute => {
                let from = crate::badges::project(camera, rect, axis.origin().to_array());
                let to = crate::badges::project(camera, rect, (axis.origin() + z).to_array());
                let along = match (from, to) {
                    (Some(a), Some(b)) => (b - a).normalized(),
                    _ => egui::Vec2::X,
                };
                Ok(Self::Screen {
                    start,
                    across: egui::vec2(-along.y, along.x),
                })
            }
            JointKind::Prismatic if z.dot(eye).abs() > FACING => Err(format!(
                "Joint {name} slides along the line of sight: turn the view to drag it."
            )),
            JointKind::Prismatic => {
                let normal = z.cross(z.cross(eye));
                let plane = Frame::new(axis.origin(), z, normal)
                    .map_err(|_| "the joint's axis has no plane to read in".to_owned())?;
                let start = in_plane(camera, rect, &plane, start)
                    .ok_or("the pointer is not over the joint's axis")?[0];
                Ok(Self::Axis { plane, start })
            }
            JointKind::Fixed => Err(format!("Joint {name} is fixed: it has no value to drag.")),
        }
    }

    /// The coordinate's change since the start with the pointer at `at`
    /// (radians or metres); the last reading where the pointer is off the
    /// plane.
    pub fn read(&mut self, camera: &OrbitCamera, rect: egui::Rect, at: egui::Pos2) -> f64 {
        match self {
            Self::Plane {
                plane,
                last,
                turned,
            } => {
                if let Some([u, v]) = in_plane(camera, rect, plane, at) {
                    let angle = v.atan2(u);
                    let tau = std::f64::consts::TAU;
                    let step = (angle - *last + std::f64::consts::PI).rem_euclid(tau)
                        - std::f64::consts::PI;
                    *turned += step;
                    *last = angle;
                }
                *turned
            }
            Self::Screen { start, across } => {
                f64::from((at - *start).dot(*across)) / POINTS_PER_RADIAN
            }
            Self::Axis { plane, start } => {
                in_plane(camera, rect, plane, at).map_or(0.0, |[u, _]| u - *start)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(value: f64, limits: Option<[f64; 2]>) -> Values {
        Values {
            value,
            offset: 0.0,
            limits,
            drives: Drives::Literal,
        }
    }

    #[test]
    fn a_trial_is_the_display_step_and_the_texts_own_value() {
        let deg = std::f64::consts::PI / 180.0;
        let (text, si) = values(0.0, None).trial(JointKind::Revolute, 37.46 * deg);
        assert_eq!(text, "37.5 deg");
        assert_eq!(
            si,
            Expr::parse("37.5 deg")
                .unwrap()
                .evaluate(&|_| None)
                .unwrap()
        );
        let (text, _) = values(0.0, None).trial(JointKind::Revolute, -0.01 * deg);
        assert_eq!(text, "0 deg");
        let (text, si) = values(0.01, None).trial(JointKind::Prismatic, 0.002_345);
        assert_eq!(text, "12.35 mm");
        assert!((si - 0.012_35).abs() < 1e-15);
    }

    #[test]
    fn a_trial_stays_inside_the_limits() {
        let deg = std::f64::consts::PI / 180.0;
        let limits = Some([-90.0 * deg, 33.36 * deg]);
        let (text, _) = values(0.0, limits).trial(JointKind::Revolute, 50.0 * deg);
        assert_eq!(text, "33.3 deg", "the rounded limit would be outside");
        let (text, _) = values(0.0, limits).trial(JointKind::Revolute, -400.0 * deg);
        assert_eq!(text, "-90 deg");
    }

    fn camera() -> OrbitCamera {
        OrbitCamera {
            yaw: 0.3,
            pitch: 0.6,
            distance: 1.0,
            ..OrbitCamera::default()
        }
    }

    fn rect() -> egui::Rect {
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))
    }

    fn screen(camera: &OrbitCamera, p: DVec3) -> egui::Pos2 {
        crate::badges::project(camera, rect(), p.to_array()).unwrap()
    }

    #[test]
    fn a_revolute_reads_the_angle_the_pointer_turns_through() {
        let camera = camera();
        let axis = Frame::new(DVec3::ZERO, DVec3::X, DVec3::Z).unwrap();
        let at = |angle: f64| screen(&camera, DVec3::new(angle.cos(), angle.sin(), 0.0) * 0.1);
        let mut reader =
            Reader::start(JointKind::Revolute, &axis, &camera, rect(), at(0.0), "j").unwrap();
        assert!(matches!(reader, Reader::Plane { .. }));
        // Past a half turn, the reading keeps counting.
        let mut q = 0.0;
        for step in 1..=8 {
            q = reader.read(&camera, rect(), at(f64::from(step) * 0.5));
        }
        assert!((q - 4.0).abs() < 1e-3, "{q}");
    }

    #[test]
    fn an_edge_on_revolute_reads_on_the_screen_and_a_facing_prismatic_is_refused() {
        let camera = OrbitCamera {
            pitch: 0.0,
            ..camera()
        };
        let axis = Frame::new(DVec3::ZERO, DVec3::X, DVec3::Z).unwrap();
        let start = egui::pos2(400.0, 300.0);
        let mut reader =
            Reader::start(JointKind::Revolute, &axis, &camera, rect(), start, "j").unwrap();
        assert!(matches!(reader, Reader::Screen { .. }));
        let q = reader.read(&camera, rect(), start + egui::vec2(50.0, 0.0));
        assert!((q.abs() - 0.5).abs() < 1e-6, "{q}");

        let top = OrbitCamera {
            pitch: std::f32::consts::FRAC_PI_2,
            ..camera
        };
        let refused = Reader::start(JointKind::Prismatic, &axis, &top, rect(), start, "slide");
        assert!(refused.unwrap_err().contains("line of sight"));
    }

    #[test]
    fn a_prismatic_reads_the_distance_along_its_axis() {
        let camera = camera();
        let axis = Frame::new(DVec3::ZERO, DVec3::X, DVec3::Y).unwrap();
        let at = |t: f64| screen(&camera, DVec3::Y * t);
        let mut reader =
            Reader::start(JointKind::Prismatic, &axis, &camera, rect(), at(0.0), "j").unwrap();
        let q = reader.read(&camera, rect(), at(0.05));
        assert!((q - 0.05).abs() < 1e-4, "{q}");
    }
}
