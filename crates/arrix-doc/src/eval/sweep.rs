//! `arrix eval --sweep`: one document evaluated at `n` values of one
//! number, each point a parameter edit on one evaluator so its cache
//! carries over (docs/CONCURRENCY-WASM.md §Batch evaluation).

use std::str::FromStr;

use arrix_core::FeatureId;

use super::fixtures::{Run, failures};
use super::{Evaluator, SweepPoint};
use crate::document::Document;
use crate::expr::Expr;
use crate::open::{DocumentSource, OpenError, open};
use crate::registry::Registry;

/// What is swept.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Target {
    /// A document parameter, by name.
    Param(String),
    /// A numeric field of a feature, the feature by its name or id.
    Field { feature: String, field: String },
}

/// `<param>=<from>..<to>:<n>` or `<feature>.<field>=<from>..<to>:<n>`:
/// `n` points from `from` to `to` inclusive, spaced evenly. `from` and
/// `to` are constant expressions (`10 mm`, `2 * 5 mm`).
#[derive(Clone, Debug, PartialEq)]
pub struct Sweep {
    target: Target,
    text: String,
    from: Expr,
    to: Expr,
    from_si: f64,
    to_si: f64,
    n: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum SweepError {
    #[error(transparent)]
    Open(#[from] OpenError),
    #[error("{0}")]
    Target(String),
}

impl FromStr for Sweep {
    type Err = String;

    fn from_str(spec: &str) -> Result<Self, String> {
        let bad = |why: &str| format!("--sweep {spec}: {why}");
        let (target, range) = spec
            .split_once('=')
            .ok_or_else(|| bad("expected <target>=<from>..<to>:<n>"))?;
        let (range, n) = range
            .rsplit_once(':')
            .ok_or_else(|| bad("expected <target>=<from>..<to>:<n>"))?;
        let (from, to) = range
            .split_once("..")
            .ok_or_else(|| bad("expected <target>=<from>..<to>:<n>"))?;
        let n: usize = n
            .trim()
            .parse()
            .ok()
            .filter(|n| *n >= 1)
            .ok_or_else(|| bad("the number of points is a whole number of at least 1"))?;
        let constant = |text: &str| -> Result<(Expr, f64), String> {
            let expr = Expr::parse(text).map_err(|e| bad(&format!("{text:?}: {e}")))?;
            if let Some(name) = expr.names().into_iter().next() {
                return Err(bad(&format!(
                    "{text:?} reads {name}; the ends are constants"
                )));
            }
            let si = expr
                .evaluate(&|_| None)
                .map_err(|e| bad(&format!("{text:?}: {e}")))?;
            Ok((expr, si))
        };
        let (from, from_si) = constant(from)?;
        let (to, to_si) = constant(to)?;
        let target = target.trim();
        let target = match target.rsplit_once('.') {
            Some((feature, field)) if !feature.is_empty() && !field.is_empty() => Target::Field {
                feature: feature.to_owned(),
                field: field.to_owned(),
            },
            _ if !target.is_empty() => Target::Param(target.to_owned()),
            _ => return Err(bad("no target")),
        };
        Ok(Sweep {
            target,
            text: target_text(spec),
            from,
            to,
            from_si,
            to_si,
            n,
        })
    }
}

fn target_text(spec: &str) -> String {
    spec.split_once('=')
        .map_or(spec, |(target, _)| target)
        .trim()
        .to_owned()
}

impl Sweep {
    /// Point `i`'s value in SI.
    fn value(&self, i: usize) -> f64 {
        match self.n {
            1 => self.from_si,
            n if i == n - 1 => self.to_si,
            n => self.from_si + (self.to_si - self.from_si) * i as f64 / (n - 1) as f64,
        }
    }

    /// Point `i`'s expression: the ends as typed, the rest interpolated in
    /// the expression language so its units are checked by the target's own
    /// kind, as any edit's are.
    fn expr(&self, i: usize) -> Expr {
        let (f, t) = (self.from.text(), self.to.text());
        let text = match self.n {
            1 => f.to_owned(),
            _ if i == 0 => f.to_owned(),
            n if i == n - 1 => t.to_owned(),
            n => format!("({f}) + (({t}) - ({f})) * {i} / {}", n - 1),
        };
        Expr::parse(&text).expect("an interpolation of two expressions parses")
    }

    /// The document with the target set to point `i`.
    fn edit(&self, base: &Document, feature: Option<FeatureId>, i: usize) -> Document {
        let mut doc = base.clone();
        let expr = self.expr(i);
        match &self.target {
            Target::Param(name) => {
                let id = base.param_by_name(name).expect("resolved").0;
                doc.params.get_mut(&id).expect("resolved").expr = expr;
            }
            Target::Field { field, .. } => {
                let feature = feature.expect("resolved");
                for part in doc.parts.values_mut() {
                    if let Some(record) = part.features.get_mut(&feature) {
                        record.params.insert(field.clone(), expr);
                        break;
                    }
                }
            }
        }
        doc
    }

    /// The feature a field target names, or why there is none.
    fn resolve(&self, doc: &Document) -> Result<Option<FeatureId>, String> {
        match &self.target {
            Target::Param(name) => doc
                .param_by_name(name)
                .map(|_| None)
                .ok_or_else(|| format!("--sweep {}: no parameter named {name}", self.text)),
            Target::Field { feature, field } => {
                let records = || doc.parts().values().flat_map(|p| p.features.values());
                let by_id = feature
                    .parse::<FeatureId>()
                    .ok()
                    .filter(|id| records().any(|r| r.id == *id));
                let mut named = records().filter(|r| r.name == *feature).map(|r| r.id);
                let id = match (by_id, named.next(), named.next()) {
                    (Some(id), _, _) | (None, Some(id), None) => id,
                    (None, Some(_), Some(_)) => {
                        return Err(format!(
                            "--sweep {}: {feature} names several features; use its id",
                            self.text
                        ));
                    }
                    (None, None, _) => {
                        return Err(format!("--sweep {}: no feature {feature}", self.text));
                    }
                };
                let (_, _, record) = doc.feature(id).expect("found");
                if !record.params.contains_key(field) {
                    return Err(format!(
                        "--sweep {}: {feature} has no numeric field {field}",
                        self.text
                    ));
                }
                Ok(Some(id))
            }
        }
    }
}

/// Opens the document and evaluates it at each point of `sweep`, on one
/// evaluator, so what a point did not change is not made again. A point
/// whose value the target refuses (a count that is not whole) is a failed
/// parameter or feature on its own line, not an error here.
pub fn sweep(
    doc: &str,
    source: &impl DocumentSource,
    registry: &Registry,
    sweep: &Sweep,
) -> Result<Vec<Run>, SweepError> {
    let base = open(source)?;
    let feature = sweep.resolve(&base).map_err(SweepError::Target)?;
    let mut evaluator = Evaluator::new(registry.clone());
    let mut runs = Vec::with_capacity(sweep.n);
    for i in 0..sweep.n {
        let document = sweep.edit(&base, feature, i);
        let evaluation = evaluator.evaluate(&document);
        let failures = failures(&evaluation, evaluator.kernel_calls());
        let mut line = evaluation.line(doc, &document);
        line.sweep = Some(SweepPoint {
            target: sweep.text.clone(),
            index: i,
            value: sweep.value(i),
        });
        runs.push(Run { line, failures });
    }
    Ok(runs)
}
