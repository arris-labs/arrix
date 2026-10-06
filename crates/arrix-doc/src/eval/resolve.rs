//! Step 1 of evaluation (docs/DATA-MODEL.md §Evaluation): parameters to SI
//! and references to resolved geometry, or a diagnostic. A reference
//! resolves to exactly one thing or fails; it never guesses.

use std::collections::BTreeMap;

use arrix_core::{
    Axis, DVec2, DVec3, Diagnostic, FeatureId, Frame, ParamId, PersistentName, Profile, Quantity,
    QuantityKind, Ref, RegionKey, Severity, SlotName, TopoKind,
};
use arrix_kernel::{Kernel, KernelBody, KernelError};
use arrix_plugin_api::{Point, ToolOp};
use arrix_sketch::{Entity, ResolveRegion, Sketch};

use super::hash::Resolved;
use super::host::kernel_diagnostic;
use super::{BodyRead, Failure, InputHash, RegionView, SketchView, SlotOut, State};
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

    /// A failed parameter `expr` reads makes it unavailable, naming that
    /// parameter.
    fn readable(&self, expr: &Expr) -> Result<(), Diagnostic> {
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
        Ok(())
    }

    fn kind_of(&self, name: &str) -> Option<QuantityKind> {
        self.by_name.get(name).map(|(_, k)| *k)
    }

    fn value_of(&self, name: &str) -> Option<f64> {
        let (id, _) = self.by_name.get(name)?;
        self.values.get(id)?.as_ref().ok().map(|q| q.si)
    }

    /// `expr` as a quantity of `kind`: a failed parameter it reads makes
    /// it unavailable, naming that parameter.
    pub(super) fn quantity(&self, expr: &Expr, kind: QuantityKind) -> Result<Quantity, Diagnostic> {
        self.readable(expr)?;
        expr.quantity(kind, &|n| self.kind_of(n), &|n| self.value_of(n))
            .map_err(|e| e.diagnostic())
    }

    /// `expr` in SI, of whatever kind it is: what a feature whose form is
    /// not at hand reads of its parameters.
    pub(super) fn si(&self, expr: &Expr) -> Result<f64, Diagnostic> {
        self.readable(expr)?;
        expr.check(&|n| self.kind_of(n))
            .map_err(|e| e.diagnostic())?;
        expr.evaluate(&|n| self.value_of(n))
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
        Some(State::Ok { hash, slots, .. }) => Ok((*hash, slots)),
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

/// An input by the shape of its reference alone, for a feature whose form
/// is not at hand: a body slot is a body, any other slot or a face a plane,
/// a region a region.
pub(super) fn blind(kernel: &mut Kernel, scope: &Scope, r: &Ref) -> Result<Resolved, Failure> {
    match r {
        Ref::Slot { feature, slot }
            if matches!(
                outputs(scope.doc, scope.states, *feature, r)?.get(slot),
                Some(SlotOut::Body { .. })
            ) =>
        {
            body(scope, r).map(Resolved::Body)
        }
        Ref::Region { .. } => region(scope.doc, scope.states, r).map(Resolved::Region),
        Ref::Feature(_) => tool(scope, r).map(|(read, _)| Resolved::Tool(read)),
        // An edge is an `edge` input, whose hash is its name; a type that
        // read it as an `axis` hashed its line, so a frozen feature of
        // that shape is reported stale until its plugin is back.
        Ref::Topo(name) if name.kind == TopoKind::Edge => {
            edge(kernel, scope, r).map(Resolved::Edge)
        }
        Ref::Topo(name) if name.kind == TopoKind::Vertex => {
            point(kernel, scope, r).map(Resolved::Point)
        }
        Ref::Sketch { feature, entity } => {
            let view = sketch_view(scope.doc, scope.states, *feature, r)?;
            if matches!(view.sketch.entity(*entity), Some(Entity::Line { .. })) {
                axis(kernel, scope, r).map(Resolved::Axis)
            } else {
                point(kernel, scope, r).map(Resolved::Point)
            }
        }
        _ => plane(kernel, scope, r).map(Resolved::Plane),
    }
}

/// The sketch `feature` solved, as its outputs hold it.
fn sketch_view<'s>(
    doc: &Document,
    states: &'s BTreeMap<FeatureId, State>,
    feature: FeatureId,
    r: &Ref,
) -> Result<&'s SketchView, Failure> {
    outputs(doc, states, feature, r)?
        .values()
        .find_map(|slot| match slot {
            SlotOut::Sketch(view) => Some(&**view),
            _ => None,
        })
        .ok_or_else(|| {
            error(
                "input.wrong-kind",
                format!("feature {feature} has no sketch to name a point or line of"),
            )
            .with_refs([r.clone()])
            .into()
        })
}

/// The tool a feature that joins or cuts kept (ADR-0013): a body read, by
/// the feature's own input hash, and the operation it was applied with. A
/// feature that keeps none, a frozen one included, is `input.no-tool`.
pub(super) fn tool(scope: &Scope, r: &Ref) -> Result<(BodyRead, ToolOp), Failure> {
    let Ref::Feature(feature) = r else {
        return Err(error(
            "input.wrong-kind",
            "a tool input takes a feature that joins or cuts",
        )
        .with_refs([r.clone()])
        .into());
    };
    let Some((maker_part, _, _)) = scope.doc.feature(*feature) else {
        return Err(error(
            "ref.lost",
            format!("feature {feature} is not in the document"),
        )
        .with_refs([r.clone()])
        .into());
    };
    if maker_part.id != scope.part.id {
        return Err(error(
            "input.cross-part",
            format!("the tool of feature {feature} is in another part; bodies are read within their part"),
        )
        .with_refs([r.clone()])
        .into());
    }
    produced(scope.doc, scope.states, *feature, r)?;
    let Some(State::Ok {
        hash,
        tool: Some(kept),
        ..
    }) = scope.states.get(feature)
    else {
        return Err(error(
            "input.no-tool",
            format!(
                "feature {feature} keeps no tool: only a feature that joins or cuts does, and a \
                 plugin feature whose result is stored does not"
            ),
        )
        .with_refs([r.clone()])
        .into());
    };
    let read = BodyRead {
        body: kept.body,
        hash: *hash,
        slot: SlotName::new("tool").expect("a literal slot name"),
    };
    Ok((read, kept.op))
}

/// An edge input: a name of an edge that exactly one body of the features
/// it names has. Which body the feature then applies it to is its kernel
/// call's to check (`kernel.unknown-name`).
pub(super) fn edge(kernel: &mut Kernel, scope: &Scope, r: &Ref) -> Result<PersistentName, Failure> {
    let Ref::Topo(name) = r else {
        return Err(
            error("input.wrong-kind", "an edge input takes an edge by name")
                .with_refs([r.clone()])
                .into(),
        );
    };
    if name.kind != TopoKind::Edge {
        return Err(error(
            "input.wrong-kind",
            format!("{name} is not an edge, where an edge is wanted"),
        )
        .with_refs([r.clone()])
        .into());
    }
    locate(kernel, scope, r, name)?;
    Ok(name.clone())
}

/// A point input: a sketch point (or a lone point entity) in the world,
/// with its plane's normal, or a vertex by name.
pub(super) fn point(kernel: &mut Kernel, scope: &Scope, r: &Ref) -> Result<Point, Failure> {
    match r {
        Ref::Sketch { feature, entity } => {
            let view = sketch_view(scope.doc, scope.states, *feature, r)?;
            let id = match view.sketch.entity(*entity) {
                Some(Entity::Point(p)) => *p,
                _ => *entity,
            };
            let Some(p) = view.sketch.point(id) else {
                return Err(lost_in_sketch(*feature, "point", r));
            };
            Ok(Point {
                at: view.plane.point(DVec2::new(p.x, p.y)),
                normal: Some(view.plane.z_axis()),
            })
        }
        Ref::Topo(name) if name.kind == TopoKind::Vertex => {
            let body = locate(kernel, scope, r, name)?;
            let at = kernel
                .vertex_point(body, name)
                .map_err(|e| kernel_failure(&e, r))?;
            Ok(Point { at, normal: None })
        }
        other => Err(error(
            "input.wrong-kind",
            "a point input takes a sketch point or a vertex",
        )
        .with_refs([other.clone()])
        .into()),
    }
}

/// An axis input: a sketch line from its start along its length, or a
/// straight edge by name, in the world with a unit direction.
pub(super) fn axis(kernel: &mut Kernel, scope: &Scope, r: &Ref) -> Result<Axis, Failure> {
    match r {
        Ref::Sketch { feature, entity } => {
            let view = sketch_view(scope.doc, scope.states, *feature, r)?;
            let Some(Entity::Line { start, end }) = view.sketch.entity(*entity) else {
                return Err(lost_in_sketch(*feature, "line", r));
            };
            let at = |id| {
                view.sketch
                    .point(id)
                    .map(|p| view.plane.point(DVec2::new(p.x, p.y)))
            };
            let (Some(a), Some(b)) = (at(*start), at(*end)) else {
                return Err(lost_in_sketch(*feature, "line", r));
            };
            let along: DVec3 = b - a;
            if along.length() < arrix_core::LENGTH_TOLERANCE {
                return Err(error(
                    "input.degenerate",
                    format!(
                        "line {entity} of feature {feature}'s sketch has no length to be an axis"
                    ),
                )
                .with_refs([r.clone()])
                .into());
            }
            Ok(Axis {
                origin: a,
                direction: along.normalize(),
            })
        }
        Ref::Topo(name) if name.kind == TopoKind::Edge => {
            let body = locate(kernel, scope, r, name)?;
            kernel.edge_axis(body, name).map_err(|e| {
                // A curved edge is the wrong kind of thing for an axis.
                let mut f = kernel_failure(&e, r);
                if e.code().as_str() == "kernel.invalid-input" {
                    f.diagnostic = error(
                        "input.wrong-kind",
                        format!("{name} is not a straight edge, where an axis is wanted"),
                    )
                    .with_refs([r.clone()]);
                }
                f
            })
        }
        other => Err(error(
            "input.wrong-kind",
            "an axis input takes a sketch line or a straight edge",
        )
        .with_refs([other.clone()])
        .into()),
    }
}

fn lost_in_sketch(feature: FeatureId, what: &str, r: &Ref) -> Failure {
    error(
        "ref.lost",
        format!("feature {feature}'s sketch has no {what} the reference names"),
    )
    .with_refs([r.clone()])
    .into()
}

fn kernel_failure(e: &KernelError, r: &Ref) -> Failure {
    Failure {
        diagnostic: kernel_diagnostic(e).with_refs([r.clone()]),
        call: e.call(),
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
    let body = locate(kernel, scope, r, name)?;
    kernel.face_frame(body, name).map_err(|e| Failure {
        diagnostic: kernel_diagnostic(&e).with_refs([r.clone()]),
        call: e.call(),
    })
}

/// The one body, among those of the features `r` names, that has an entity
/// called `name`. None is a lost reference with the nearest names as
/// candidates, and so is more than one: a name never picks between bodies.
fn locate(
    kernel: &mut Kernel,
    scope: &Scope,
    r: &Ref,
    name: &PersistentName,
) -> Result<KernelBody, Failure> {
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
        return Ok(body);
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
                    index: _,
                    feature: a,
                    part: p,
                },
                NameRoot::Sweep {
                    index: _,
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
