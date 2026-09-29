//! Step 1 of evaluation (docs/DATA-MODEL.md §Evaluation): parameters to SI
//! and references to resolved geometry, or a diagnostic. A reference
//! resolves to exactly one thing or fails; it never guesses.

use std::collections::BTreeMap;

use arrix_core::{
    Diagnostic, FeatureId, Frame, ParamId, PersistentName, Profile, Quantity, QuantityKind, Ref,
    RegionKey, Severity, SlotName, TopoKind,
};
use arrix_kernel::{Kernel, KernelBody};
use arrix_sketch::{ResolveRegion, Sketch};

use super::host::kernel_diagnostic;
use super::{BodyRead, Failure, InputHash, RegionView, SlotOut, State};
use crate::bodies::{self, Effect, SlotKey};
use crate::dag::{Dag, Node, ref_features};
use crate::document::{Document, Part};
use crate::expr::Expr;
use crate::registry::Registry;

/// How many candidates a lost reference lists.
const MAX_CANDIDATES: usize = 5;

fn error(code: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Severity::Error, code, message).expect("a literal code is well-formed")
}

/// A reference that points at something that did not produce its outputs.
pub(super) fn unavailable(r: Ref, why: impl Into<String>) -> Diagnostic {
    error("input.unavailable", why).with_refs([r])
}

/// Every parameter's value, or why it has none.
pub(super) struct Params {
    by_name: BTreeMap<String, (ParamId, QuantityKind)>,
    pub(super) values: BTreeMap<ParamId, Result<Quantity, Diagnostic>>,
}

impl Params {
    /// Evaluates every parameter in the DAG's order, so each one's names
    /// are resolved before it.
    pub(super) fn evaluate(doc: &Document, dag: &Dag) -> Params {
        let mut out = Params {
            by_name: doc
                .params()
                .iter()
                .map(|(id, p)| (p.name.clone(), (*id, p.kind)))
                .collect(),
            values: BTreeMap::new(),
        };
        for node in dag.order() {
            if let Node::Param(id) = node {
                let p = &doc.params()[id];
                let v = out
                    .quantity(&p.expr, p.kind)
                    .map_err(|d| d.with_refs([Ref::Param(*id)]));
                out.values.insert(*id, v);
            }
        }
        out
    }

    /// `expr` as a quantity of `kind`: a failed parameter it reads makes
    /// it unavailable, naming that parameter.
    pub(super) fn quantity(&self, expr: &Expr, kind: QuantityKind) -> Result<Quantity, Diagnostic> {
        for name in expr.names() {
            if let Some((id, _)) = self.by_name.get(name)
                && matches!(self.values.get(id), Some(Err(_)))
            {
                return Err(unavailable(
                    Ref::Param(*id),
                    format!("parameter `{name}` has no value"),
                ));
            }
        }
        let kinds = |n: &str| self.by_name.get(n).map(|(_, k)| *k);
        let values = |n: &str| {
            let (id, _) = self.by_name.get(n)?;
            self.values.get(id)?.as_ref().ok().map(|q| q.si)
        };
        expr.quantity(kind, &kinds, &values)
            .map_err(|e| e.diagnostic())
    }
}

/// Where a feature is read from: the document and what earlier features
/// produced, and the reader's place in its part's history, which decides
/// the version of a body it sees (ADR-0007).
pub(super) struct Scope<'a> {
    pub(super) registry: &'a Registry,
    pub(super) doc: &'a Document,
    pub(super) states: &'a BTreeMap<FeatureId, State>,
    pub(super) part: &'a Part,
    pub(super) at: usize,
}

/// The outputs of a feature a reference reads, or why there are none.
fn outputs<'s>(
    doc: &Document,
    states: &'s BTreeMap<FeatureId, State>,
    feature: FeatureId,
    r: &Ref,
) -> Result<&'s BTreeMap<SlotName, SlotOut>, Diagnostic> {
    produced(doc, states, feature, r).map(|(_, slots)| slots)
}

/// The same with the input hash the feature was evaluated under, which
/// identifies its outputs.
fn produced<'s>(
    doc: &Document,
    states: &'s BTreeMap<FeatureId, State>,
    feature: FeatureId,
    r: &Ref,
) -> Result<(InputHash, &'s BTreeMap<SlotName, SlotOut>), Diagnostic> {
    match states.get(&feature) {
        Some(State::Ok { hash, slots }) => Ok((*hash, slots)),
        Some(State::Unavailable(why)) => Err(unavailable(
            Ref::Feature(feature),
            format!("feature {feature} {why}"),
        )),
        None if doc.feature(feature).is_none() => Err(error(
            "ref.lost",
            format!("feature {feature} is not in the document"),
        )
        .with_refs([r.clone()])),
        // The DAG orders every read feature first; one not yet evaluated
        // is in another part's rolled-back or unordered tail.
        None => Err(unavailable(
            Ref::Feature(feature),
            format!("feature {feature} was not evaluated"),
        )),
    }
}

/// A body input: the version of the slot it names that is current at the
/// reader (ADR-0007). The last feature before the reader that modifies the
/// slot gives it, else the feature that made it. A slot a feature before
/// the reader consumed is `slot.consumed`, and a modifier that failed
/// leaves the version unknown, so the reader is unavailable rather than
/// given an earlier one.
pub(super) fn body(scope: &Scope, r: &Ref) -> Result<BodyRead, Failure> {
    let Ref::Slot { feature, slot } = r else {
        return Err(error(
            "input.wrong-kind",
            "a body input takes a feature's body slot",
        )
        .with_refs([r.clone()])
        .into());
    };
    let key = SlotKey {
        feature: *feature,
        slot: slot.clone(),
    };
    version(scope, &key, r)
}

fn version(scope: &Scope, key: &SlotKey, r: &Ref) -> Result<BodyRead, Failure> {
    let Scope {
        registry,
        doc,
        states,
        part,
        at,
    } = *scope;
    let Some((maker_part, _, maker)) = doc.feature(key.feature) else {
        return Err(error(
            "ref.lost",
            format!("feature {} is not in the document", key.feature),
        )
        .with_refs([r.clone()])
        .into());
    };
    if maker_part.id != part.id {
        return Err(error(
            "input.cross-part",
            format!(
                "slot `{}` of feature {} is in another part; bodies are read within their part",
                key.slot, key.feature
            ),
        )
        .with_refs([r.clone()])
        .into());
    }
    let is_version = |ty: &std::sync::Arc<dyn crate::registry::FeatureType>| {
        ty.spec()
            .outputs
            .iter()
            .any(|s| s.name == key.slot && bodies::is_version(s, maker))
    };
    if registry.get(&maker.type_id).is_some_and(is_version) {
        return Err(error(
            "ref.lost",
            format!(
                "slot `{}` of feature {} is a version of another slot; name the feature that made it",
                key.slot, key.feature
            ),
        )
        .with_refs([r.clone()])
        .into());
    }
    let (hash, slots) = produced(doc, states, key.feature, r)?;
    let mut current = (hash, slots, key.slot.clone());
    for (record, effect) in bodies::touching(registry, part, at, key) {
        if record.suppressed {
            continue;
        }
        match effect {
            Effect::Consumes => {
                return Err(error(
                    "slot.consumed",
                    format!(
                        "slot `{}` of feature {} was consumed by feature {}",
                        key.slot, key.feature, record.id
                    ),
                )
                .with_refs([r.clone(), Ref::Feature(record.id)])
                .into());
            }
            Effect::Modifies(out) => {
                let (hash, slots) = produced(doc, states, record.id, r)?;
                current = (hash, slots, out);
            }
        }
    }
    let (hash, slots, name) = current;
    match slots.get(&name) {
        Some(SlotOut::Body { body, .. }) => Ok(BodyRead {
            body: *body,
            hash,
            slot: name,
        }),
        Some(_) => Err(error(
            "input.wrong-kind",
            format!(
                "slot `{}` of feature {} is not a body, where a body is wanted",
                key.slot, key.feature
            ),
        )
        .with_refs([r.clone()])
        .into()),
        None => Err(error(
            "ref.lost",
            format!("feature {} has no slot `{}`", key.feature, key.slot),
        )
        .with_refs([r.clone()])
        .into()),
    }
}

/// A plane input: a feature's plane slot, or a planar face by name.
pub(super) fn plane(kernel: &mut Kernel, scope: &Scope, r: &Ref) -> Result<Frame, Failure> {
    let (doc, states) = (scope.doc, scope.states);
    match r {
        Ref::Slot { feature, slot } => match outputs(doc, states, *feature, r)?.get(slot) {
            Some(SlotOut::Plane(frame)) => Ok(*frame),
            Some(other) => {
                let kind = match other {
                    SlotOut::Body { .. } => "a body",
                    _ => "a sketch",
                };
                Err(error(
                    "input.wrong-kind",
                    format!(
                        "slot `{slot}` of feature {feature} is {kind}, where a plane is wanted"
                    ),
                )
                .with_refs([r.clone()])
                .into())
            }
            None => Err(error(
                "ref.lost",
                format!("feature {feature} has no slot `{slot}`"),
            )
            .with_refs([r.clone()])
            .into()),
        },
        Ref::Topo(name) if name.kind == TopoKind::Face => face_frame(kernel, scope, r, name),
        other => Err(error(
            "input.wrong-kind",
            "a plane input takes a feature's plane slot or a planar face",
        )
        .with_refs([other.clone()])
        .into()),
    }
}

/// A region input: a sketch feature's region by its key, as a profile on
/// the sketch's plane. A key its sketch no longer answers to exactly once
/// is lost, with the regions it has now as candidates; it never falls back
/// to the nearest one (docs/DATA-MODEL.md §Sketches).
pub(super) fn region(
    doc: &Document,
    states: &BTreeMap<FeatureId, State>,
    r: &Ref,
) -> Result<Profile, Failure> {
    let Ref::Region { feature, key } = r else {
        return Err(
            error("input.wrong-kind", "a region input takes a sketch's region")
                .with_refs([r.clone()])
                .into(),
        );
    };
    let view = outputs(doc, states, *feature, r)?
        .values()
        .find_map(|slot| match slot {
            SlotOut::Sketch(view) => Some(view),
            _ => None,
        })
        .ok_or_else(|| {
            error(
                "input.wrong-kind",
                format!("feature {feature} has no sketch to hold a region"),
            )
            .with_refs([r.clone()])
        })?;
    let Some(found) = key.resolve(&view.sketch) else {
        return Err(error(
            "ref.lost",
            format!(
                "no one region of feature {feature}'s sketch is bounded by the {} entities \
                 the reference names around its sample",
                key.entities.len()
            ),
        )
        .with_refs([r.clone()])
        .with_candidates(region_candidates(*feature, key, &view.regions))
        .into());
    };
    found.to_profile(&view.sketch, view.plane).map_err(|e| {
        error("sketch.profile", e.to_string())
            .with_refs([r.clone()])
            .into()
    })
}

/// The regions a lost key might be re-picked as, best first: the most
/// bounding entities shared with it, then the sketch's own order, largest
/// first. Never applied here.
fn region_candidates(feature: FeatureId, lost: &RegionKey, regions: &[RegionView]) -> Vec<Ref> {
    let shared = |k: &RegionKey| {
        k.entities
            .iter()
            .filter(|e| lost.entities.binary_search(e).is_ok())
            .count()
    };
    let mut ranked: Vec<&RegionView> = regions.iter().collect();
    ranked.sort_by_key(|v| std::cmp::Reverse(shared(&v.key)));
    ranked
        .into_iter()
        .take(MAX_CANDIDATES)
        .map(|v| Ref::Region {
            feature,
            key: v.key.clone(),
        })
        .collect()
}

fn face_frame(
    kernel: &mut Kernel,
    scope: &Scope,
    r: &Ref,
    name: &PersistentName,
) -> Result<Frame, Failure> {
    let mut pool: Vec<KernelBody> = Vec::new();
    let mut keys = std::collections::BTreeSet::new();
    for f in ref_features(r) {
        outputs(scope.doc, scope.states, f, r)?;
        keys.extend(bodies::named_slots(scope.registry, scope.doc, f));
    }
    for key in keys {
        let body = version(scope, &key, r)?.body;
        if !pool.contains(&body) {
            pool.push(body);
        }
    }
    let mut matches = Vec::new();
    for b in &pool {
        let names = kernel.names(*b).map_err(|e| kernel_diagnostic(&e))?;
        if names.contains(name) {
            matches.push(*b);
        }
    }
    if let [body] = matches[..] {
        return kernel.face_frame(body, name).map_err(|e| Failure {
            diagnostic: kernel_diagnostic(&e).with_refs([r.clone()]),
            call: e.call(),
        });
    }
    let mut all = Vec::new();
    for b in &pool {
        let names = kernel.names(*b).map_err(|e| kernel_diagnostic(&e))?;
        all.extend(names.of_kind(name.kind).cloned());
    }
    let why = if matches.is_empty() {
        "matches nothing"
    } else {
        "matches more than one entity"
    };
    Err(error("ref.lost", format!("{name} {why}"))
        .with_refs([r.clone()])
        .with_candidates(candidates(name, &all))
        .into())
}

/// How alike two names are, most alike greatest: the same root beats the
/// same feature's same kind of sweep part, which beats the same feature;
/// then the longer shared chain prefix.
fn likeness(lost: &PersistentName, other: &PersistentName) -> (u8, usize) {
    use arrix_core::NameRoot;
    let root = if lost.root == other.root {
        3
    } else {
        match (&lost.root, &other.root) {
            (
                NameRoot::Sweep {
                    feature: a,
                    part: p,
                },
                NameRoot::Sweep {
                    feature: b,
                    part: q,
                },
            ) if a == b => {
                if std::mem::discriminant(p) == std::mem::discriminant(q) {
                    2
                } else {
                    1
                }
            }
            _ => 0,
        }
    };
    let shared = lost
        .chain
        .iter()
        .zip(&other.chain)
        .take_while(|(a, b)| a == b)
        .count();
    (root, shared)
}

/// What a user might re-pick instead of `lost`, best first: names of the
/// same kind, by likeness, ties in name order. Never applied here.
pub(super) fn candidates(lost: &PersistentName, pool: &[PersistentName]) -> Vec<Ref> {
    let mut ranked: Vec<&PersistentName> = pool.iter().filter(|n| n.kind == lost.kind).collect();
    ranked.sort_by(|a, b| {
        likeness(lost, b)
            .cmp(&likeness(lost, a))
            .then_with(|| a.cmp(b))
    });
    ranked.dedup();
    ranked
        .into_iter()
        .take(MAX_CANDIDATES)
        .map(|n| Ref::Topo(n.clone()))
        .collect()
}

/// `sketch` with each driving dimension's expression resolved into its
/// value in SI, the quantity its constraint measures. A failure names the
/// constraint, as `Ref::Sketch`; a reference or suppressed record's
/// expression is not read.
pub(super) fn dimensions(
    feature: FeatureId,
    sketch: &Sketch,
    params: &Params,
) -> Result<Sketch, Diagnostic> {
    let mut out = sketch.clone();
    for (id, record) in sketch.constraints() {
        let Some(text) = record.expr.as_deref() else {
            continue;
        };
        if record.is_reference() || record.is_suppressed() {
            continue;
        }
        let named = Ref::Sketch {
            feature,
            entity: *id,
        };
        let Some(kind) = record.constraint.dimension_kind() else {
            return Err(error(
                "sketch.not-a-dimension",
                format!("constraint {id} has an expression and no value for it to drive"),
            )
            .with_refs([named]));
        };
        let expr = Expr::parse(text).map_err(|e| e.diagnostic().with_refs([named.clone()]))?;
        let value = params.quantity(&expr, kind).map_err(|mut d| {
            d.message = format!("dimension {id}: {}", d.message);
            d.with_refs([named])
        })?;
        out.get_constraint_mut(*id)
            .expect("the id is the sketch's")
            .set_dimensional_value(value.si);
    }
    Ok(out)
}
