//! The evaluated document as an exporter reads it (ADR-0037;
//! docs/PLUGINS.md §4): the plugin API's [`Snapshot`] over a finished
//! evaluation, and `Evaluator::export_with`, which runs one exporter on it.
//! Nothing here calls the kernel except to mesh a body, which is cached
//! by version like the viewport's meshes.

use std::collections::BTreeMap;

use arrix_core::{ComponentId, DVec3, Diagnostic, MeshDetail, PartId, Severity};
use arrix_plugin_api as api;
use arrix_plugin_api::{ChoiceValue, ExportFile, ParamValue, Snapshot, Subject};

use super::assembly::{AssemblyOutcome, JointOutcome};
use super::kinematics::Rigid;
use super::resolve::Params;
use super::{Evaluation, Evaluator};
use crate::assembly::{Assembly, ComponentOf, JointKind};
use crate::dag::Dag;
use crate::document::Document;
use crate::expr::Expr;
use crate::mass::Inertial;
use crate::registry::ExporterType;

fn error(code: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Severity::Error, code, message.into()).expect("a literal code is well-formed")
}

/// A relative path of `/`-separated parts, none empty, `.` or `..`, with
/// no drive and no leading `/` (ADR-0037 §7).
pub fn check_export_name(name: &str) -> Result<(), Diagnostic> {
    let bad = |why: &str| error("export.path", format!("the file name {name:?} {why}"));
    if name.is_empty() {
        return Err(bad("is empty"));
    }
    if name.contains('\\') || name.contains(':') || name.contains('\0') {
        return Err(bad(
            "has a backslash, a colon or a NUL, which a relative path does not",
        ));
    }
    for part in name.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return Err(bad("has an empty, `.` or `..` part, or starts at the root"));
        }
    }
    Ok(())
}

/// The snapshot of one finished evaluation of `doc`.
pub(super) struct DocSnapshot<'a> {
    pub(super) evaluator: &'a mut Evaluator,
    pub(super) doc: &'a Document,
    pub(super) evaluation: &'a Evaluation,
    pub(super) params: Params,
}

impl DocSnapshot<'_> {
    /// A part's history, each feature with its choices (the record's, or
    /// the spec's default) and the params the evaluation can read, in SI
    /// (ADR-0038 §4). A param that fails to evaluate is left out, as is
    /// everything a type the registry lacks would have declared.
    fn features(&self, part: &crate::document::Part) -> Vec<api::FeatureInfo> {
        part.history
            .iter()
            .filter_map(|id| part.features.get(id))
            .map(|record| {
                let spec = self.evaluator.registry().get(&record.type_id);
                let mut params = Vec::new();
                let mut choices = Vec::new();
                if let Some(spec) = spec.map(|t| t.spec()) {
                    for p in &spec.params {
                        let expr = match record.params.get(&p.name) {
                            Some(e) => Some(e.clone()),
                            None => Expr::parse(&p.default).ok(),
                        };
                        if let Some(Ok(value)) = expr.map(|e| self.params.quantity(&e, p.kind)) {
                            params.push(ParamValue {
                                name: p.name.clone(),
                                value,
                            });
                        }
                    }
                    for c in &spec.choices {
                        if let Some(value) = record.choices.get(&c.name).or(c.default.as_ref()) {
                            choices.push(ChoiceValue {
                                name: c.name.clone(),
                                value: value.clone(),
                            });
                        }
                    }
                }
                api::FeatureInfo {
                    id: record.id,
                    type_id: record.type_id.to_string(),
                    name: record.name.clone(),
                    choices,
                    params,
                }
            })
            .collect()
    }
}

impl Snapshot for DocSnapshot<'_> {
    fn parts(&mut self) -> Vec<api::PartInfo> {
        let finals = self.evaluator.final_bodies();
        self.doc
            .parts()
            .values()
            .map(|part| api::PartInfo {
                id: part.id,
                name: part.name.clone(),
                bodies: finals
                    .iter()
                    .filter(|b| b.part == part.id)
                    .map(|b| {
                        let key = crate::material::SlotRef {
                            feature: b.slot.feature,
                            slot: b.slot.slot.clone(),
                        };
                        let material = part
                            .body_materials
                            .get(&key)
                            .copied()
                            .or(part.material)
                            .and_then(|m| self.doc.materials().get(&m))
                            .map(|m| api::MaterialInfo {
                                name: m.name.clone(),
                                color: api::Rgb {
                                    red: m.appearance.0[0],
                                    green: m.appearance.0[1],
                                    blue: m.appearance.0[2],
                                },
                            });
                        api::BodyInfo {
                            body: api::BodyRef {
                                feature: b.slot.feature,
                                slot: b.slot.slot.clone(),
                            },
                            material,
                        }
                    })
                    .collect(),
                features: self.features(part),
            })
            .collect()
    }

    fn mesh(&mut self, part: PartId, body: &api::BodyRef) -> Result<api::Triangles, Diagnostic> {
        let found = self
            .evaluator
            .final_bodies()
            .iter()
            .find(|b| b.part == part && b.slot.feature == body.feature && b.slot.slot == body.slot)
            .cloned();
        let Some(found) = found else {
            return Err(error(
                "export.unknown-body",
                format!(
                    "{}.{} is not a body of part {part}",
                    body.feature, body.slot
                ),
            ));
        };
        let mesh = self.evaluator.mesh(&found, MeshDetail::Refined)?;
        // Positions are relative to the mesh's origin, which is in the
        // world; a part's frame is the world's (ADR-0037 §3).
        let mut positions = Vec::with_capacity(mesh.positions.len() * 3);
        for p in &mesh.positions {
            let w = mesh.world(*p);
            positions.extend([w.x as f32, w.y as f32, w.z as f32]);
        }
        Ok(api::Triangles {
            positions,
            normals: mesh.normals.iter().flatten().copied().collect(),
            indices: mesh.triangles.iter().flatten().copied().collect(),
        })
    }

    fn mass(&mut self, part: PartId) -> Result<api::Inertial, Diagnostic> {
        let Some(outcome) = self.evaluation.masses.parts.get(&part) else {
            return Err(error("export.unknown-part", format!("no part {part}")));
        };
        match &outcome.mass {
            Some(i) => Ok(inertial(i)),
            None => {
                Err(outcome.diagnostics.first().cloned().unwrap_or_else(|| {
                    error("mass.no-material", format!("part {part} has no mass"))
                }))
            }
        }
    }

    fn assemblies(&mut self) -> Vec<api::AssemblyInfo> {
        self.doc
            .assemblies()
            .values()
            .filter_map(|a| {
                let outcome = self.evaluation.assemblies.get(&a.id)?;
                Some(assembly_info(self.doc, &self.params, a, outcome))
            })
            .collect()
    }
}

fn inertial(i: &Inertial) -> api::Inertial {
    let m = i.inertia;
    api::Inertial {
        mass: i.mass,
        centroid: DVec3::from_array(i.centroid),
        inertia: api::InertiaTensor {
            xx: m[0][0],
            xy: m[0][1],
            xz: m[0][2],
            yy: m[1][1],
            yz: m[1][2],
            zz: m[2][2],
        },
    }
}

/// The component at `path` from `assembly` down: its name and what it
/// places.
fn leaf<'d>(
    doc: &'d Document,
    assembly: &'d Assembly,
    path: &[ComponentId],
) -> Option<&'d crate::assembly::Component> {
    let (first, rest) = path.split_first()?;
    let component = assembly.components.get(first)?;
    match (component.of, rest.is_empty()) {
        (_, true) => Some(component),
        (ComponentOf::Assembly(a), false) => leaf(doc, doc.assemblies().get(&a)?, rest),
        _ => None,
    }
}

fn assembly_info(
    doc: &Document,
    params: &Params,
    assembly: &Assembly,
    outcome: &AssemblyOutcome,
) -> api::AssemblyInfo {
    let components = outcome
        .components
        .iter()
        .filter_map(|placed| {
            let c = leaf(doc, assembly, &placed.path)?;
            Some(api::ComponentInfo {
                path: placed.path.clone(),
                name: c.name.clone(),
                of: match c.of {
                    ComponentOf::Part(p) => api::ComponentOf::Part(p),
                    ComponentOf::Assembly(a) => api::ComponentOf::Assembly(a),
                },
                pose: placed.pose,
                diagnostics: placed.diagnostics.clone(),
            })
        })
        .collect();
    let joints = assembly
        .joints
        .values()
        .filter_map(|joint| {
            let state = match outcome.joints.get(&joint.id)? {
                JointOutcome::Failed { diagnostic } => api::JointState::Failed(diagnostic.clone()),
                JointOutcome::Ok {
                    value,
                    parent,
                    child,
                    diagnostics,
                } => {
                    // The outcome's frames are the anchors in the assembly;
                    // an exporter wants each in its own component's frame,
                    // which does not move with the joint's value.
                    let local = |path: &[ComponentId], frame: &arrix_core::Frame| {
                        let at = Rigid::of(&outcome.pose(path)?);
                        Some(at.inverse().then(Rigid::of(frame)).frame())
                    };
                    match (
                        local(&joint.parent.path, parent),
                        local(&joint.child.path, child),
                    ) {
                        (Some(parent_anchor), Some(child_anchor)) => {
                            api::JointState::Placed(api::JointPlacement {
                                parent_anchor,
                                child_anchor,
                                value: *value,
                                diagnostics: diagnostics.clone(),
                            })
                        }
                        _ => api::JointState::Failed(error(
                            "assembly.unplaced",
                            format!(
                                "joint {} has no placed component to hang its anchors on",
                                joint.name
                            ),
                        )),
                    }
                }
            };
            let limits = joint
                .limits
                .as_ref()
                .and_then(|[lo, hi]| Some([params.si(lo).ok()?, params.si(hi).ok()?]));
            Some(api::JointInfo {
                id: joint.id,
                name: joint.name.clone(),
                kind: match joint.kind {
                    JointKind::Fixed => api::JointKind::Fixed,
                    JointKind::Revolute => api::JointKind::Revolute,
                    JointKind::Prismatic => api::JointKind::Prismatic,
                },
                parent: joint.parent.path.clone(),
                child: joint.child.path.clone(),
                limits,
                state,
            })
        })
        .collect();
    api::AssemblyInfo {
        id: assembly.id,
        name: assembly.name.clone(),
        components,
        joints,
    }
}

/// An exporter's options as the host evaluates them: each named param's
/// expression (the spec's default when not given) to a quantity of its
/// kind, each choice its given word or default (ADR-0037 §6).
pub(super) fn options(
    form: Form,
    params: &Params,
    given_params: &BTreeMap<String, String>,
    given_choices: &BTreeMap<String, String>,
) -> Result<(Vec<ParamValue>, Vec<ChoiceValue>), Diagnostic> {
    let Form {
        id,
        params: param_specs,
        choices: choice_specs,
    } = form;
    let unknown = |name: &str| {
        error(
            "feature.unknown-field",
            format!("{id} has no option `{name}`"),
        )
    };
    for name in given_params.keys() {
        if !param_specs.iter().any(|p| &p.name == name)
            && !choice_specs.iter().any(|c| &c.name == name)
        {
            return Err(unknown(name));
        }
    }
    let mut out_params = Vec::new();
    for p in param_specs {
        let text = given_params.get(&p.name).unwrap_or(&p.default);
        let expr = Expr::parse(text).map_err(|e| e.diagnostic())?;
        let q = params.quantity(&expr, p.kind)?;
        out_params.push(ParamValue {
            name: p.name.clone(),
            value: q,
        });
    }
    let mut out_choices = Vec::new();
    for c in choice_specs {
        let word = given_choices.get(&c.name).or(c.default.as_ref());
        let Some(word) = word else { continue };
        if !c.options.iter().any(|o| &o.name == word) {
            return Err(error(
                "feature.choice",
                format!("`{word}` is no option of `{}` of {id}", c.name),
            ));
        }
        out_choices.push(ChoiceValue {
            name: c.name.clone(),
            value: word.clone(),
        });
    }
    for name in given_choices.keys() {
        if !choice_specs.iter().any(|c| &c.name == name) {
            return Err(unknown(name));
        }
    }
    Ok((out_params, out_choices))
}

/// The fields of a form that options are read against: an exporter's or a
/// command's.
pub(super) struct Form<'a> {
    pub id: &'a str,
    pub params: &'a [api::ParamSpec],
    pub choices: &'a [api::ChoiceSpec],
}

impl Evaluator {
    /// Runs `exporter` on the latest finished evaluation of `doc`
    /// (`evaluation` is what `evaluate` returned, and nothing was
    /// evaluated since). `params` are the options' expressions and
    /// `choices` the words; what is not given is the spec's default.
    /// A refused subject, option or file name is a diagnostic
    /// (`export.subject`, `feature.*`, `export.path`).
    pub fn export_with(
        &mut self,
        doc: &Document,
        evaluation: &Evaluation,
        exporter: &dyn ExporterType,
        subject: Subject,
        params: &BTreeMap<String, String>,
        choices: &BTreeMap<String, String>,
    ) -> Result<Vec<ExportFile>, Diagnostic> {
        let spec = exporter.spec();
        let (kind, exists) = match subject {
            Subject::Part(p) => (api::SubjectKind::Part, doc.parts().contains_key(&p)),
            Subject::Assembly(a) => (
                api::SubjectKind::Assembly,
                doc.assemblies().contains_key(&a),
            ),
        };
        if kind != spec.subject {
            return Err(error(
                "export.subject",
                format!("{} exports a {:?}, not a {kind:?}", spec.id, spec.subject),
            ));
        }
        if !exists {
            return Err(error(
                "export.unknown-subject",
                format!("the document has no such {kind:?}: {subject:?}"),
            ));
        }
        let dag =
            Dag::build_with(doc, &self.registry).expect("a document is valid by construction");
        let values = Params::evaluate(doc, &dag);
        let form = Form {
            id: &spec.id,
            params: &spec.params,
            choices: &spec.choices,
        };
        let (params, choices) = options(form, &values, params, choices)?;
        let mut snapshot = DocSnapshot {
            evaluator: self,
            doc,
            evaluation,
            params: values,
        };
        let files = exporter.export(&mut snapshot, &subject, &params, &choices)?;
        if files.is_empty() {
            return Err(error(
                "export.no-files",
                format!("{} wrote no file", spec.id),
            ));
        }
        let mut seen = std::collections::BTreeSet::new();
        for f in &files {
            check_export_name(&f.name)?;
            if !seen.insert(f.name.as_str()) {
                return Err(error(
                    "export.path",
                    format!("the file name {:?} is written twice", f.name),
                ));
            }
        }
        Ok(files)
    }
}
