//! The robot, read from an assembly: a link per part component at every
//! depth, each hung from `base_link` or from the link of the joint that
//! places it. Pure data; `urdf` writes it.
//!
//! A link's frame is its component's frame moved to the anchor of the joint
//! that places it, since URDF turns a joint about its child link's origin
//! and ArriX turns it about the anchor (ADR-0029 §4). The solved poses are
//! the truth: a joint is used only if its anchors reproduce the child's
//! pose, and a component it does not explain hangs from `base_link` at its
//! pose.

use std::collections::BTreeMap;

use arrix_plugin_api::{
    AssemblyId, AssemblyInfo, ComponentId, ComponentInfo, ComponentOf, JointKind, JointState,
    PartId, Snapshot,
};

use crate::rigid::Rigid;

/// How far a joint's anchors may be from the child's solved pose, in metres
/// and unit-matrix entries.
const AGREE: f64 = 1e-6;

pub struct Joint {
    pub name: String,
    pub kind: JointKind,
    /// Index into [`Robot::links`].
    pub parent: usize,
    pub limits: Option<[f64; 2]>,
    /// The joint's frame in the parent link's.
    pub origin: Rigid,
}

pub struct Link {
    pub name: String,
    pub part: PartId,
    /// The link's frame in the assembly's.
    pub frame: Rigid,
    /// The part's frame in the link's.
    pub part_in_link: Rigid,
    /// The joint that places it; `None` hangs it from `base_link` at
    /// `frame`.
    pub joint: Option<Joint>,
}

pub struct Robot {
    pub name: String,
    pub links: Vec<Link>,
    pub warnings: Vec<String>,
}

/// A joint of the subject or of an assembly nested in it, with its paths
/// from the subject down.
struct Candidate<'a> {
    info: &'a arrix_plugin_api::JointInfo,
    parent: Vec<ComponentId>,
    child: Vec<ComponentId>,
}

pub fn read(snapshot: &mut dyn Snapshot, subject: AssemblyId) -> Option<Robot> {
    let assemblies = snapshot.assemblies();
    let root = assemblies.iter().find(|a| a.id == subject)?;
    let mut warnings = Vec::new();

    let parts: Vec<&ComponentInfo> = root
        .components
        .iter()
        .filter(|c| matches!(c.of, ComponentOf::Part(_)))
        .collect();
    let index: BTreeMap<&[ComponentId], usize> = parts
        .iter()
        .enumerate()
        .map(|(i, c)| (c.path.as_slice(), i))
        .collect();

    // The joint that places each component, in the order joints are met.
    let mut offsets: Vec<Option<Rigid>> = vec![None; parts.len()];
    let candidates = candidates(&assemblies, root);
    let mut accepted: Vec<(usize, usize, Rigid, usize)> = Vec::new();
    for (n, cand) in candidates.iter().enumerate() {
        let JointState::Placed(p) = &cand.info.state else {
            warnings.push(format!(
                "joint {} did not solve and is left out",
                cand.info.name
            ));
            continue;
        };
        let (Some(&pi), Some(&ci)) = (
            index.get(cand.parent.as_slice()),
            index.get(cand.child.as_slice()),
        ) else {
            warnings.push(format!(
                "joint {} does not join two parts and is left out",
                cand.info.name
            ));
            continue;
        };
        let motion = match cand.info.kind {
            JointKind::Fixed => Rigid::IDENTITY,
            JointKind::Revolute => Rigid::about_z(p.value),
            JointKind::Prismatic => Rigid::along_z(p.value),
        };
        let (fp, fc) = (Rigid::of(&p.parent_anchor), Rigid::of(&p.child_anchor));
        let expected = Rigid::of(&parts[pi].pose)
            .then(&fp)
            .then(&motion)
            .then(&fc.inverse());
        if !expected.near(&Rigid::of(&parts[ci].pose), AGREE) {
            warnings.push(format!(
                "joint {} does not reproduce the pose of {}; it is left out",
                cand.info.name, parts[ci].name
            ));
            continue;
        }
        if offsets[ci].is_some() || is_above(&accepted, ci, pi) {
            warnings.push(format!(
                "joint {} would place {} twice or in a loop; it is left out",
                cand.info.name, parts[ci].name
            ));
            continue;
        }
        offsets[ci] = Some(fc);
        accepted.push((ci, pi, fp, n));
    }

    // Link frames and names.
    let mut names = Names::default();
    names.take("base_link");
    let mut links: Vec<Link> = Vec::new();
    let mut joint_names = Names::default();
    for (ci, comp) in parts.iter().enumerate() {
        let ComponentOf::Part(part) = comp.of else {
            unreachable!("parts are part components")
        };
        let offset = offsets[ci].unwrap_or(Rigid::IDENTITY);
        let pose = Rigid::of(&comp.pose);
        links.push(Link {
            name: names.take(&link_name(root, &comp.path)),
            part,
            frame: pose.then(&offset),
            part_in_link: offset.inverse(),
            joint: None,
        });
        if offsets[ci].is_none() && !comp.diagnostics.is_empty() {
            let codes: Vec<_> = comp.diagnostics.iter().map(|d| d.code.as_str()).collect();
            warnings.push(format!(
                "{} is not placed ({}); it hangs from base_link at its solved pose",
                comp.name,
                codes.join(", ")
            ));
        }
    }
    for (ci, pi, fp, n) in accepted {
        let cand = &candidates[n];
        let origin = links[pi].part_in_link.then(&fp);
        links[ci].joint = Some(Joint {
            name: joint_names.take(&cand.info.name),
            kind: cand.info.kind,
            parent: pi,
            limits: cand.info.limits,
            origin,
        });
    }
    // The joints are written in link order, each after the link it places.
    Some(Robot {
        name: crate::names::sanitize(&root.name),
        links,
        warnings,
    })
}

/// Whether `child` is `parent` or above it in the joints accepted so far.
fn is_above(accepted: &[(usize, usize, Rigid, usize)], child: usize, parent: usize) -> bool {
    let mut at = parent;
    loop {
        if at == child {
            return true;
        }
        match accepted.iter().find(|(c, ..)| *c == at) {
            Some((_, up, ..)) => at = *up,
            None => return false,
        }
    }
}

/// The subject's joints, then those of each assembly nested in it, whose
/// paths are from that assembly down and so take its component's path.
fn candidates<'a>(all: &'a [AssemblyInfo], root: &'a AssemblyInfo) -> Vec<Candidate<'a>> {
    let mut out: Vec<Candidate> = root
        .joints
        .iter()
        .map(|j| Candidate {
            info: j,
            parent: j.parent.clone(),
            child: j.child.clone(),
        })
        .collect();
    for comp in &root.components {
        let ComponentOf::Assembly(id) = comp.of else {
            continue;
        };
        let Some(nested) = all.iter().find(|a| a.id == id) else {
            continue;
        };
        let under = |path: &[ComponentId]| [comp.path.as_slice(), path].concat();
        out.extend(nested.joints.iter().map(|j| Candidate {
            info: j,
            parent: under(&j.parent),
            child: under(&j.child),
        }));
    }
    out
}

/// A component's path as a name: its ancestors' names and its own.
fn link_name(root: &AssemblyInfo, path: &[ComponentId]) -> String {
    let parts: Vec<String> = (1..=path.len())
        .map(|n| {
            root.components
                .iter()
                .find(|c| c.path == path[..n])
                .map_or_else(|| path[n - 1].to_string(), |c| c.name.clone())
        })
        .collect();
    crate::names::sanitize(&parts.join("."))
}

/// Unique names: a taken one gets `_2`, `_3`, …
#[derive(Default)]
pub struct Names(std::collections::BTreeSet<String>);

impl Names {
    pub fn take(&mut self, wanted: &str) -> String {
        let wanted = crate::names::sanitize(wanted);
        let mut name = wanted.clone();
        let mut n = 2;
        while !self.0.insert(name.clone()) {
            name = format!("{wanted}_{n}");
            n += 1;
        }
        name
    }
}
