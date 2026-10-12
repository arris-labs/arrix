//! The mass pass (ADR-0031 §4; docs/DATA-MODEL.md §Evaluation): after the
//! assemblies, the mass of every final body (its material's density times
//! the kernel's unit-density properties), of every part (its bodies summed
//! in its frame) and of every assembly (its placed parts summed at their
//! solved poses). The kernel's properties are held by body version, so an
//! unchanged body is not measured again; the rest is arithmetic. Fail
//! soft: a body without a density has `mass.no-material`, is left out of
//! the sums and marks them partial.

use std::collections::BTreeMap;

use arrix_core::{AssemblyId, ComponentId, Diagnostic, MaterialId, PartId, QuantityKind, Severity};
use arrix_kernel::MassProperties;
use serde::{Deserialize, Serialize};

use super::assembly::{AssemblyOutcome, Stopped};
use super::host::kernel_diagnostic;
use super::meshing::FinalBody;
use super::resolve::Params;
use super::{Evaluator, interrupted};
use crate::assembly::ComponentOf;
use crate::document::{Document, Part};
use crate::mass::{BodyMass, Inertial, MassOutcome};
use crate::material::SlotRef;

/// Every mass result of an evaluation.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Masses {
    /// Every final body, in the document's order.
    pub bodies: Vec<BodyMass>,
    pub parts: BTreeMap<PartId, MassOutcome>,
    pub assemblies: BTreeMap<AssemblyId, MassOutcome>,
    /// Each material's density in kg/m³, where it has a valid one.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub densities: BTreeMap<MaterialId, f64>,
}

fn diag(code: &str, message: String) -> Diagnostic {
    Diagnostic::new(Severity::Warning, code, message).expect("a literal code is well-formed")
}

impl Evaluator {
    pub(super) fn masses(
        &mut self,
        doc: &Document,
        params: &Params,
        finals: &[FinalBody],
        assemblies: &BTreeMap<AssemblyId, AssemblyOutcome>,
    ) -> Result<Masses, Stopped> {
        let mut bodies = Vec::new();
        for body in finals {
            let part = &doc.parts[&body.part];
            bodies.push(self.body_mass(doc, params, part, body)?);
        }
        let mut parts = BTreeMap::new();
        for part in doc.parts.keys() {
            let own: Vec<&BodyMass> = bodies.iter().filter(|b| b.part == *part).collect();
            parts.insert(*part, part_outcome(&own));
        }
        let mut out = BTreeMap::new();
        for (id, outcome) in assemblies {
            out.insert(*id, assembly_outcome(doc, *id, outcome, &parts));
        }
        let densities = doc
            .materials
            .keys()
            .filter_map(|m| density(doc, params, *m).ok().map(|d| (*m, d)))
            .collect();
        Ok(Masses {
            bodies,
            parts,
            assemblies: out,
            densities,
        })
    }

    fn body_mass(
        &mut self,
        doc: &Document,
        params: &Params,
        part: &Part,
        body: &FinalBody,
    ) -> Result<BodyMass, Stopped> {
        let unit = match self.unit_properties(body) {
            Ok(unit) => unit,
            Err(d) if interrupted(&d) => return Err(Stopped),
            Err(d) => {
                return Ok(BodyMass {
                    part: body.part,
                    slot: body.slot.clone(),
                    volume: 0.0,
                    centroid: [0.0; 3],
                    material: None,
                    mass: None,
                    diagnostic: Some(d),
                });
            }
        };
        let key = SlotRef {
            feature: body.slot.feature,
            slot: body.slot.slot.clone(),
        };
        let material = part.body_materials.get(&key).copied().or(part.material);
        let density = material
            .map(|m| density(doc, params, m))
            .unwrap_or_else(|| Err(diag("mass.no-material", format!("{key} has no material"))));
        let (mass, diagnostic) = match density {
            Ok(d) => (
                Some(Inertial::of_body(
                    d,
                    unit.volume,
                    unit.centroid,
                    unit.inertia,
                )),
                None,
            ),
            Err(d) => (None, Some(d)),
        };
        Ok(BodyMass {
            part: body.part,
            slot: body.slot.clone(),
            volume: unit.volume,
            centroid: unit.centroid.to_array(),
            material,
            mass,
            diagnostic,
        })
    }

    /// The kernel's unit-density properties of `body`, from what a
    /// feature's measure already asked or from one recorded call.
    fn unit_properties(&mut self, body: &FinalBody) -> Result<MassProperties, Diagnostic> {
        if let Some(p) = self.unit.get(&body.body) {
            return Ok(*p);
        }
        let p = self
            .kernel
            .mass_properties(body.body)
            .map_err(|e| kernel_diagnostic(&e))?;
        self.unit.insert(body.body, p);
        Ok(p)
    }
}

/// The density of `material` in kg/m³, or why there is none.
fn density(doc: &Document, params: &Params, material: MaterialId) -> Result<f64, Diagnostic> {
    let m = &doc.materials[&material];
    let Some(expr) = &m.density else {
        return Err(diag(
            "mass.no-material",
            format!("material {:?} has no density", m.name),
        ));
    };
    let q = params.quantity(expr, QuantityKind::Density).map_err(|d| {
        diag(
            "mass.density",
            format!("material {:?}: {}", m.name, d.message),
        )
    })?;
    if q.si > 0.0 && q.si.is_finite() {
        Ok(q.si)
    } else {
        Err(diag(
            "mass.density",
            format!("material {:?} has a density of {} kg/m3", m.name, q.si),
        ))
    }
}

fn part_outcome(bodies: &[&BodyMass]) -> MassOutcome {
    let known: Vec<Inertial> = bodies.iter().filter_map(|b| b.mass).collect();
    MassOutcome {
        volume: bodies.iter().map(|b| b.volume).sum(),
        mass: Inertial::sum(&known),
        partial: known.len() < bodies.len(),
        diagnostics: bodies.iter().filter_map(|b| b.diagnostic.clone()).collect(),
    }
}

/// The assembly's parts at their solved poses. A component whose own pose
/// or an enclosing one is `assembly.unplaced` is not summed.
fn assembly_outcome(
    doc: &Document,
    id: AssemblyId,
    solved: &AssemblyOutcome,
    parts: &BTreeMap<PartId, MassOutcome>,
) -> MassOutcome {
    let mut volume = 0.0;
    let mut placed = Vec::new();
    let mut partial = false;
    let mut diagnostics = Vec::new();
    for c in &solved.components {
        let Some(part) = leaf_part(doc, id, &c.path) else {
            continue;
        };
        let unplaced = (1..=c.path.len()).any(|n| {
            solved
                .components
                .iter()
                .find(|p| p.path == c.path[..n])
                .is_some_and(|p| {
                    p.diagnostics
                        .iter()
                        .any(|d| d.code.as_str() == "assembly.unplaced")
                })
        });
        if unplaced {
            partial = true;
            diagnostics.push(diag(
                "mass.unplaced",
                format!("a component of part {part} is unplaced, so its mass is left out"),
            ));
            continue;
        }
        let outcome = &parts[&part];
        volume += outcome.volume;
        partial |= outcome.partial;
        diagnostics.extend(outcome.diagnostics.iter().cloned());
        if let Some(mass) = &outcome.mass {
            placed.push(mass.moved(&c.pose));
        }
    }
    MassOutcome {
        volume,
        mass: Inertial::sum(&placed),
        partial,
        diagnostics,
    }
}

/// The part a component at `path` from `assembly` places, `None` for a
/// nested assembly (its own components are listed at their own depth).
fn leaf_part(doc: &Document, assembly: AssemblyId, path: &[ComponentId]) -> Option<PartId> {
    let mut at = doc.assemblies.get(&assembly)?;
    for (i, id) in path.iter().enumerate() {
        match at.components.get(id)?.of {
            ComponentOf::Part(p) if i + 1 == path.len() => return Some(p),
            ComponentOf::Part(_) => return None,
            ComponentOf::Assembly(a) => at = doc.assemblies.get(&a)?,
        }
    }
    None
}
