//! The evaluator (docs/DATA-MODEL.md §Evaluation): a document snapshot in,
//! each feature's outputs or diagnostic out, memoised by input hash and
//! failing soft. `LocalSession` runs it on its worker, stopping between
//! features for a newer snapshot (docs/CONCURRENCY-WASM.md §The evaluator).

mod hash;
mod host;
mod line;
mod resolve;
#[cfg(test)]
mod sketch_tests;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};

use arrix_core::RegionKey;
use arrix_core::{
    Diagnostic, FeatureId, Frame, ParamId, Profile, Quantity, Ref, Severity, SlotName, TopoKind,
};
use arrix_kernel::{CallIndex, Kernel, KernelBody, KernelCall};
use arrix_plugin_api::{InputKind, InputValue, OutputValue, ParamValue, ResolvedInput, SlotKind};
use arrix_sketch::{ConstraintId, Sketch};
use serde::{Deserialize, Serialize};

pub use hash::InputHash;
pub use line::{BodyLine, EvalLine, EvalStatus, FeatureLine, ParamLine, SweepPoint, eval};

use crate::dag::{Dag, Node};
use crate::document::{Document, FeatureRecord, Part};
use crate::expr::Expr;
use crate::registry::{FeatureArgs, FeatureType, Registry, SketchArgs};
use host::FeatureKernel;
use resolve::Params;

/// A body's measures, in SI, and its topology counts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BodyMeasures {
    /// Cubic metres.
    pub volume: f64,
    /// Square metres.
    pub area: f64,
    pub faces: usize,
    pub edges: usize,
    pub vertices: usize,
}

/// An output slot as a client sees it: plain data, no kernel handle.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotView {
    Plane(Frame),
    Body(BodyMeasures),
    Sketch(Box<SketchView>),
}

/// A solved sketch, as its `core.sketch` feature outputs it: the plane it
/// stands on, how free it still is, the solved sketch and its regions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SketchView {
    pub plane: Frame,
    /// Degrees of freedom left; 0 is fully constrained.
    pub dof: i32,
    /// Constraints the others already imply. Consistent, so the sketch
    /// solves; a client shows them.
    pub redundant: Vec<ConstraintId>,
    /// Solved from the stored positions with the dimensions resolved.
    pub sketch: Sketch,
    /// Largest first, the order `arrix_sketch::find_regions` gives.
    pub regions: Vec<RegionView>,
}

/// One region: the key a feature stores to come back to it, and the
/// region as a profile on the sketch's plane, its curves keyed by entity
/// (docs/DATA-MODEL.md §Persistent naming).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RegionView {
    pub key: RegionKey,
    pub profile: Profile,
}

/// How one feature's evaluation ended.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum FeatureOutcome {
    Ok {
        hash: InputHash,
        /// Reused from the cache: nothing it reads changed.
        cached: bool,
        slots: BTreeMap<SlotName, SlotView>,
    },
    /// No outputs; the feature keeps its parameters and says why.
    Failed {
        diagnostic: Diagnostic,
        /// The kernel call it failed at, kept for a fixture.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        call: Option<Box<KernelCall>>,
    },
    Suppressed,
    /// At or after its part's rollback index.
    RolledBack,
}

/// One feature's outcome, as the evaluator publishes it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EvalEvent {
    pub feature: FeatureId,
    pub outcome: FeatureOutcome,
}

/// One snapshot's evaluation: every parameter's value, and an event per
/// feature in the DAG's order.
#[derive(Clone, Debug, PartialEq)]
pub struct Evaluation {
    pub params: BTreeMap<ParamId, Result<Quantity, Diagnostic>>,
    pub events: Vec<EvalEvent>,
}

impl Evaluation {
    pub fn outcome(&self, feature: FeatureId) -> Option<&FeatureOutcome> {
        self.events
            .iter()
            .find(|e| e.feature == feature)
            .map(|e| &e.outcome)
    }

    /// The feature's slot, when it evaluated.
    pub fn slot(&self, feature: FeatureId, slot: &str) -> Option<&SlotView> {
        match self.outcome(feature)? {
            FeatureOutcome::Ok { slots, .. } => slots.iter().find(|(n, _)| n.as_str() == slot),
            _ => None,
        }
        .map(|(_, v)| v)
    }
}

/// An output slot, with the kernel's handle for a body.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SlotOut {
    Plane(Frame),
    Body {
        body: KernelBody,
        measures: BodyMeasures,
    },
    Sketch(Box<SketchView>),
}

impl SlotOut {
    fn view(&self) -> SlotView {
        match self {
            SlotOut::Plane(f) => SlotView::Plane(*f),
            SlotOut::Body { measures, .. } => SlotView::Body(*measures),
            SlotOut::Sketch(s) => SlotView::Sketch(s.clone()),
        }
    }
}

type Slots = BTreeMap<SlotName, SlotOut>;

/// What a later feature finds when it reads one.
pub(crate) enum State {
    Ok(Slots),
    /// Why it has no outputs: "failed", "is suppressed", "is rolled back".
    Unavailable(&'static str),
}

/// A diagnostic, and the kernel call it came from, if one did.
pub(crate) struct Failure {
    pub(crate) diagnostic: Diagnostic,
    pub(crate) call: Option<CallIndex>,
}

impl From<Diagnostic> for Failure {
    fn from(diagnostic: Diagnostic) -> Self {
        Failure {
            diagnostic,
            call: None,
        }
    }
}

fn error(code: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Severity::Error, code, message).expect("a literal code is well-formed")
}

struct CacheEntry {
    slots: Slots,
    /// The evaluation that last used it.
    used: u64,
}

/// Evaluates snapshots of documents through one long-lived kernel, whose
/// bodies are held by the cache between evaluations.
pub struct Evaluator {
    registry: Registry,
    kernel: Kernel,
    cache: BTreeMap<InputHash, CacheEntry>,
    run: u64,
    capacity: usize,
}

impl Evaluator {
    /// How many cache entries outlive the evaluation that made them. By
    /// count until a body's size is known (body bytes, Arris ask A2).
    pub const DEFAULT_CAPACITY: usize = 256;

    pub fn new(registry: Registry) -> Self {
        Self {
            registry,
            kernel: Kernel::new(),
            cache: BTreeMap::new(),
            run: 0,
            capacity: Self::DEFAULT_CAPACITY,
        }
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Every kernel call made so far, in call order.
    pub fn kernel_calls(&self) -> &[KernelCall] {
        self.kernel.records()
    }

    /// Evaluates `doc`: parameters, then every feature in the DAG's order.
    pub fn evaluate(&mut self, doc: &Document) -> Evaluation {
        self.evaluate_while(doc, |_| true)
            .expect("an evaluation that is never stopped finishes")
    }

    /// Evaluates `doc` as [`Evaluator::evaluate`] does, handing each
    /// feature's event to `go` as it is made. When `go` returns `false` the
    /// evaluation stops there, between features, and returns `None`: a
    /// newer snapshot wins (docs/CONCURRENCY-WASM.md §The evaluator). What
    /// it computed so far stays cached.
    pub fn evaluate_while(
        &mut self,
        doc: &Document,
        mut go: impl FnMut(&EvalEvent) -> bool,
    ) -> Option<Evaluation> {
        self.run += 1;
        let dag = Dag::build(doc).expect("a document is valid by construction");
        let params = Params::evaluate(doc, &dag);
        let mut states = BTreeMap::new();
        let mut events = Vec::new();
        for node in dag.order() {
            let Node::Feature(id) = node else { continue };
            let (part, at, record) = doc.feature(*id).expect("the DAG's features are the doc's");
            let (outcome, state) = self.feature(doc, part, at, record, &params, &states);
            states.insert(*id, state);
            let event = EvalEvent {
                feature: *id,
                outcome,
            };
            let more = go(&event);
            events.push(event);
            if !more {
                self.collect();
                return None;
            }
        }
        self.collect();
        Some(Evaluation {
            params: params.values,
            events,
        })
    }

    fn feature(
        &mut self,
        doc: &Document,
        part: &Part,
        at: usize,
        record: &FeatureRecord,
        params: &Params,
        states: &BTreeMap<FeatureId, State>,
    ) -> (FeatureOutcome, State) {
        if part.rollback.is_some_and(|r| at >= r) {
            return (
                FeatureOutcome::RolledBack,
                State::Unavailable("is rolled back"),
            );
        }
        if record.suppressed {
            return (
                FeatureOutcome::Suppressed,
                State::Unavailable("is suppressed"),
            );
        }
        match self.compute(doc, record, params, states) {
            Ok((hash, cached, slots)) => {
                let views = slots.iter().map(|(n, s)| (n.clone(), s.view())).collect();
                let outcome = FeatureOutcome::Ok {
                    hash,
                    cached,
                    slots: views,
                };
                (outcome, State::Ok(slots))
            }
            Err(f) => {
                let call = f
                    .call
                    .and_then(|c| self.kernel.record(c))
                    .map(|r| Box::new(r.clone()));
                let mut diagnostic = f.diagnostic;
                if diagnostic.refs.is_empty() {
                    diagnostic.refs.push(Ref::Feature(record.id));
                }
                let outcome = FeatureOutcome::Failed { diagnostic, call };
                (outcome, State::Unavailable("failed"))
            }
        }
    }

    /// Resolves, hashes and evaluates one feature, or finds it cached.
    fn compute(
        &mut self,
        doc: &Document,
        record: &FeatureRecord,
        params: &Params,
        states: &BTreeMap<FeatureId, State>,
    ) -> Result<(InputHash, bool, Slots), Failure> {
        let ty = self.registry.get(&record.type_id).cloned().ok_or_else(|| {
            // Frozen results (docs/DATA-MODEL.md §Frozen results) replace
            // this failure when they land.
            error(
                "feature.unknown-type",
                format!("no feature type {} is registered", record.type_id),
            )
        })?;
        let spec = ty.spec();
        if record.type_version != spec.version {
            return Err(error(
                "feature.type-version",
                format!(
                    "the feature is {} version {}, and this build has version {}",
                    record.type_id, record.type_version, spec.version
                ),
            )
            .into());
        }
        let args = self.resolve(doc, &*ty, record, params, states)?;
        let hash = InputHash::of(record, &*ty, &args);
        if let Some(entry) = self.cache.get_mut(&hash) {
            entry.used = self.run;
            return Ok((hash, true, entry.slots.clone()));
        }
        let slots = self.run_type(&*ty, record.id, &args)?;
        self.cache.insert(
            hash,
            CacheEntry {
                slots: slots.clone(),
                used: self.run,
            },
        );
        Ok((hash, false, slots))
    }

    /// The feature's arguments: its form's parameters in SI and its inputs
    /// resolved. A field the form does not list is refused, not ignored.
    fn resolve(
        &mut self,
        doc: &Document,
        ty: &dyn FeatureType,
        record: &FeatureRecord,
        params: &Params,
        states: &BTreeMap<FeatureId, State>,
    ) -> Result<FeatureArgs, Failure> {
        let spec = ty.spec();
        let unknown = |what: &str, name: &str| {
            error(
                "feature.unknown-field",
                format!("{} has no {what} `{name}`", spec.id),
            )
        };
        if let Some(name) = record
            .params
            .keys()
            .find(|k| !spec.params.iter().any(|p| &p.name == *k))
        {
            return Err(unknown("parameter", name).into());
        }
        if let Some(name) = record
            .inputs
            .keys()
            .find(|k| !spec.inputs.iter().any(|i| &i.name == *k))
        {
            return Err(unknown("input", name).into());
        }
        let mut args = FeatureArgs {
            choices: record.choices.clone(),
            sketch: record
                .sketch
                .as_deref()
                .map(|sketch| {
                    Ok::<_, Diagnostic>(SketchArgs {
                        feature: record.id,
                        sketch: resolve::dimensions(record.id, sketch, params)?,
                    })
                })
                .transpose()?,
            ..FeatureArgs::default()
        };
        for p in &spec.params {
            let expr = match record.params.get(&p.name) {
                Some(e) => e.clone(),
                None => Expr::parse(&p.default).map_err(|e| {
                    error(
                        "feature.bad-default",
                        format!("{}'s default for `{}`: {e}", spec.id, p.name),
                    )
                })?,
            };
            let value = params.quantity(&expr, p.kind).map_err(|mut d| {
                d.message = format!("`{}`: {}", p.name, d.message);
                d
            })?;
            args.params.push(ParamValue {
                name: p.name.clone(),
                value,
            });
        }
        for i in &spec.inputs {
            let Some(r) = record.inputs.get(&i.name) else {
                continue;
            };
            let value = match i.kind {
                InputKind::Plane => {
                    InputValue::Plane(resolve::plane(&mut self.kernel, doc, states, r)?)
                }
                InputKind::Region => InputValue::Region(resolve::region(doc, states, r)?),
            };
            args.inputs.push(ResolvedInput {
                name: i.name.clone(),
                value,
            });
        }
        Ok(args)
    }

    /// Calls the type's `evaluate` and checks its outputs against the
    /// slots it declared.
    fn run_type(
        &mut self,
        ty: &dyn FeatureType,
        feature: FeatureId,
        args: &FeatureArgs,
    ) -> Result<Slots, Failure> {
        let spec = ty.spec();
        let mut k = FeatureKernel::new(&mut self.kernel, feature);
        let output = ty.evaluate(&mut k, args).map_err(|diagnostic| Failure {
            diagnostic,
            call: k.failed,
        })?;
        let bad = |message: String| Failure::from(error("feature.output", message));
        let mut slots = Slots::new();
        for (name, view) in output.sketches {
            if !ty.sketch_slots().contains(&name) {
                return Err(bad(format!("{} declares no sketch slot `{name}`", spec.id)));
            }
            if slots
                .insert(name.clone(), SlotOut::Sketch(Box::new(view)))
                .is_some()
            {
                return Err(bad(format!("slot `{name}` is filled twice")));
            }
        }
        for out in output.output.slots {
            let declared = spec.outputs.iter().find(|s| s.name == out.name);
            let slot = match (declared.map(|s| s.kind), out.value) {
                (Some(SlotKind::Plane), OutputValue::Plane(f)) => SlotOut::Plane(f),
                (Some(SlotKind::Body), OutputValue::Body(b)) => {
                    let body = k.body(&b).ok_or_else(|| {
                        bad(format!("slot `{}` holds a body it did not build", out.name))
                    })?;
                    SlotOut::Body {
                        body,
                        measures: BodyMeasures::default(),
                    }
                }
                (None, _) => {
                    return Err(bad(format!("{} declares no slot `{}`", spec.id, out.name)));
                }
                (Some(_), _) => {
                    return Err(bad(format!("slot `{}` holds the wrong kind", out.name)));
                }
            };
            if slots.insert(out.name.clone(), slot).is_some() {
                return Err(bad(format!("slot `{}` is filled twice", out.name)));
            }
        }
        let declared = spec.outputs.iter().map(|s| &s.name);
        if let Some(missing) = declared
            .chain(ty.sketch_slots())
            .find(|name| !slots.contains_key(*name))
        {
            return Err(bad(format!("slot `{missing}` was not filled")));
        }
        for slot in slots.values_mut() {
            if let SlotOut::Body { body, measures } = slot {
                *measures = self.measure(*body)?;
            }
        }
        Ok(slots)
    }

    fn measure(&mut self, body: KernelBody) -> Result<BodyMeasures, Failure> {
        let fail = |e: arrix_kernel::KernelError| Failure {
            diagnostic: host::kernel_diagnostic(&e),
            call: e.call(),
        };
        let p = self.kernel.mass_properties(body).map_err(fail)?;
        let names = self.kernel.names(body).map_err(fail)?;
        Ok(BodyMeasures {
            volume: p.volume,
            area: p.area,
            faces: names.count(TopoKind::Face),
            edges: names.count(TopoKind::Edge),
            vertices: names.count(TopoKind::Vertex),
        })
    }

    /// Drops the least recently used entries past the capacity, never one
    /// this evaluation used, then every body no entry holds.
    fn collect(&mut self) {
        let mut by_age: Vec<(u64, InputHash)> = self
            .cache
            .iter()
            .filter(|(_, e)| e.used != self.run)
            .map(|(h, e)| (e.used, *h))
            .collect();
        by_age.sort();
        let excess = self.cache.len().saturating_sub(self.capacity);
        for (_, h) in by_age.into_iter().take(excess) {
            self.cache.remove(&h);
        }
        let keep: BTreeSet<KernelBody> = self
            .cache
            .values()
            .flat_map(|e| e.slots.values())
            .filter_map(|s| match s {
                SlotOut::Body { body, .. } => Some(*body),
                SlotOut::Plane(_) | SlotOut::Sketch(_) => None,
            })
            .collect();
        self.kernel.retain(&keep);
    }
}
