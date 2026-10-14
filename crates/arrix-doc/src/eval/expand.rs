//! Commands expanded (ADR-0038; docs/PLUGINS.md §Contribution points, 3):
//! a plugin command run on a finished evaluation. The picks the client
//! made resolve into the command's inputs (an edge's component path, name
//! and geometry, moved into the context's frame), the options evaluate as
//! an exporter's do, the plugin expands against the snapshot of the
//! evaluation, and the group it answers is checked and translated into one
//! core command. Nothing is applied here.

use std::collections::BTreeMap;

use arrix_core::{ComponentId, Diagnostic, Id, PartId, PersistentName};
use arrix_plugin_api as api;
use serde::{Deserialize, Serialize};

use super::assembly::error;
use super::host::kernel_diagnostic;
use super::kinematics::Rigid;
use super::snapshot::{DocSnapshot, Form, options};
use super::{Evaluation, Evaluator};
use crate::command::Command;
use crate::command::plugin::translate;
use crate::dag::{Dag, anchor_target};
use crate::document::Document;
use crate::registry::CommandType;

/// One edge a client picked: its component path from the assembly down
/// (empty in a part) and its name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct EdgePick {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<ComponentId>,
    pub edge: PersistentName,
}

/// What `Evaluator::expand` takes besides the document and its evaluation.
pub struct Expansion<'a> {
    pub command: &'a dyn CommandType,
    pub context: api::Context,
    /// The form's expressions and words; what is not given is the spec's
    /// default.
    pub params: &'a BTreeMap<String, String>,
    pub choices: &'a BTreeMap<String, String>,
    /// The picks of each input, by the input's name, in the order picked.
    pub picks: &'a BTreeMap<String, Vec<EdgePick>>,
    /// Fresh ids, taken by the plugin in order.
    pub ids: &'a [Id],
}

/// How many ids a command gets to mint with when its spec says none.
pub const DEFAULT_MAX_NEW: usize = 64;

impl Evaluator {
    /// Expands `expansion` against `evaluation`, the latest finished
    /// evaluation of `doc` (nothing was evaluated since). The result is
    /// one `Command::Group`; a refused context, pick, option or record is
    /// a diagnostic (`command.*`, `feature.*`), and so is whatever the
    /// plugin answers.
    pub fn expand(
        &mut self,
        doc: &Document,
        evaluation: &Evaluation,
        expansion: &Expansion,
    ) -> Result<Command, Diagnostic> {
        let spec = expansion.command.spec();
        let limit = spec.max_new.map_or(DEFAULT_MAX_NEW, |n| n as usize);
        if expansion.ids.len() > limit {
            return Err(error(
                "command.ids",
                format!(
                    "{} takes at most {limit} ids, and was given {}",
                    spec.id,
                    expansion.ids.len()
                ),
            ));
        }
        let exists = match expansion.context {
            api::Context::Part(p) => doc.parts().contains_key(&p),
            api::Context::Assembly(a) => doc.assemblies().contains_key(&a),
        };
        if !exists {
            return Err(error(
                "command.context",
                format!("the document has no such context: {:?}", expansion.context),
            ));
        }
        let inputs = self.command_inputs(doc, evaluation, expansion)?;
        let dag =
            Dag::build_with(doc, &self.registry).expect("a document is valid by construction");
        let values = super::resolve::Params::evaluate(doc, &dag);
        let form = Form {
            id: &spec.id,
            params: &spec.params,
            choices: &spec.choices,
        };
        let (params, choices) = options(form, &values, expansion.params, expansion.choices)?;
        let mut snapshot = DocSnapshot {
            evaluator: self,
            doc,
            evaluation,
            params: values,
        };
        let group = expansion.command.expand(
            &mut snapshot,
            &expansion.context,
            &params,
            &choices,
            &inputs,
            expansion.ids,
        )?;
        translate(&group, expansion.ids, &self.registry, doc)
    }

    /// The picks as the command's resolved inputs, one per element in the
    /// order of the spec's inputs, then the order picked.
    fn command_inputs(
        &mut self,
        doc: &Document,
        evaluation: &Evaluation,
        expansion: &Expansion,
    ) -> Result<Vec<api::CommandInput>, Diagnostic> {
        let spec = expansion.command.spec();
        for name in expansion.picks.keys() {
            if !spec.inputs.iter().any(|i| &i.name == name) {
                return Err(error(
                    "command.unknown-input",
                    format!("{} has no input `{name}`", spec.id),
                ));
            }
        }
        let mut out = Vec::new();
        for input in &spec.inputs {
            let Some(picks) = expansion.picks.get(&input.name) else {
                continue;
            };
            if !input.list && picks.len() > 1 {
                return Err(error(
                    "command.input",
                    format!(
                        "{}'s input `{}` takes one edge, and was given {}",
                        spec.id,
                        input.name,
                        picks.len()
                    ),
                ));
            }
            for pick in picks {
                let edge = self.resolve_edge(doc, evaluation, expansion.context, pick)?;
                out.push(api::CommandInput {
                    name: input.name.clone(),
                    value: api::CommandInputValue::Edge(edge),
                });
            }
        }
        Ok(out)
    }

    /// An edge on its part's final body, its geometry moved into the
    /// context's frame by the component's solved pose.
    fn resolve_edge(
        &mut self,
        doc: &Document,
        evaluation: &Evaluation,
        context: api::Context,
        pick: &EdgePick,
    ) -> Result<api::ResolvedEdge, Diagnostic> {
        let (part, motion) = self.pick_part(doc, evaluation, context, pick)?;
        let pool: Vec<&super::FinalBody> = self.finals.iter().filter(|b| b.part == part).collect();
        if pool.is_empty() {
            return Err(error(
                "input.unavailable",
                format!("part {part} has no body for {} to name", pick.edge),
            ));
        }
        let body = self.find(&pool, &pick.edge)?.body;
        let geometry = self
            .kernel
            .edge_geometry(body, &pick.edge)
            .map_err(|e| kernel_diagnostic(&e))?;
        let curve = match geometry.curve {
            arrix_kernel::EdgeCurve::Circle {
                centre,
                axis,
                radius,
            } => api::EdgeCurve::Circle(api::CircleEdge {
                centre: motion.point(centre),
                axis: motion.vector(axis),
                radius,
            }),
            arrix_kernel::EdgeCurve::Line { start, end } => api::EdgeCurve::Line(api::LineEdge {
                start: motion.point(start),
                end: motion.point(end),
            }),
            arrix_kernel::EdgeCurve::Other => api::EdgeCurve::Other,
        };
        Ok(api::ResolvedEdge {
            path: pick.path.clone(),
            name: pick.edge.clone(),
            curve,
            face_normals: geometry
                .face_normals
                .into_iter()
                .map(|n| motion.vector(n))
                .collect(),
        })
    }

    /// The part a pick is on and where it stands in the context's frame.
    fn pick_part(
        &self,
        doc: &Document,
        evaluation: &Evaluation,
        context: api::Context,
        pick: &EdgePick,
    ) -> Result<(PartId, Rigid), Diagnostic> {
        match context {
            api::Context::Part(part) => {
                if !pick.path.is_empty() {
                    return Err(error(
                        "command.pick",
                        format!("{} is picked through a component, in a part", pick.edge),
                    ));
                }
                Ok((part, Rigid::IDENTITY))
            }
            api::Context::Assembly(assembly) => {
                let Some((_, part)) = anchor_target(doc, assembly, &pick.path) else {
                    return Err(error(
                        "ref.lost",
                        format!("the path of {} leads to no part of the assembly", pick.edge),
                    ));
                };
                let pose = evaluation
                    .assemblies
                    .get(&assembly)
                    .and_then(|o| o.pose(&pick.path))
                    .ok_or_else(|| {
                        error(
                            "assembly.unplaced",
                            format!("the component of {} has no solved pose", pick.edge),
                        )
                    })?;
                Ok((part, Rigid::of(&pose)))
            }
        }
    }
}
