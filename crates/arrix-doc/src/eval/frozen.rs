//! A plugin feature that evaluates from its stored result
//! (docs/DATA-MODEL.md §Frozen results): when it does, how its staleness is
//! decided, how the result is loaded, and what downstream features are told.

use std::collections::{BTreeMap, BTreeSet};

use arrix_core::{Diagnostic, FeatureId, Ref, Severity, SlotName};
use arrix_kernel::{KernelError, NameTable};

use super::resolve::{self, Params, Scope};
use super::{
    CacheEntry, Computed, Evaluator, Failure, FrozenInfo, FrozenReason, InputHash, SlotOut, State,
    error, host,
};
use crate::dag::{Dag, Node};
use crate::document::{Document, FeatureRecord};
use crate::frozen::{BlobRef, FrozenResult};
use crate::registry::FeatureType;

/// Why `record` evaluates frozen, or `None` when it evaluates live (or
/// fails, as a core feature of an unknown type does). A plugin's feature
/// is frozen when its type is not registered, when the record is a newer
/// version of the type than the plugin knows, or when the plugin's version
/// is not compatible (semver caret) with the one that stored the result.
pub(super) fn reason(record: &FeatureRecord, ty: Option<&dyn FeatureType>) -> Option<FrozenReason> {
    record.type_id.plugin()?;
    let Some(ty) = ty else {
        return Some(FrozenReason::Missing);
    };
    if record.type_version > ty.spec().version {
        return Some(FrozenReason::Version);
    }
    let stored = &record.frozen.as_ref()?.plugin_version;
    let have = semver::Version::parse(ty.plugin_version()?).ok()?;
    let compatible = semver::VersionReq::parse(&format!("^{stored}")).ok()?;
    (!compatible.matches(&have)).then_some(FrozenReason::Version)
}

/// The stale frozen features whose results `feature` reads, directly or
/// through what it reads.
pub(super) fn stale_upstream(
    dag: &Dag,
    states: &BTreeMap<FeatureId, State>,
    feature: FeatureId,
) -> BTreeSet<FeatureId> {
    let mut out = BTreeSet::new();
    for node in dag.reads(Node::Feature(feature)) {
        if let Node::Feature(read) = node
            && let Some(State::Ok { stale, .. }) = states.get(&read)
        {
            out.extend(stale);
        }
    }
    out
}

/// A frozen feature's reason as an informational diagnostic, then a
/// `plugin.frozen.stale` warning for each stale feature `stale` holds,
/// the feature's own or one it reads.
pub(super) fn diagnostics(
    record: &FeatureRecord,
    frozen: Option<FrozenInfo>,
    stale: &BTreeSet<FeatureId>,
) -> Vec<Diagnostic> {
    let diagnostic = |severity, code: &str, message: String, of: FeatureId| {
        Diagnostic::new(severity, code, message)
            .expect("a literal code is well-formed")
            .with_refs([Ref::Feature(of)])
    };
    let mut out = Vec::new();
    if let Some(info) = frozen {
        let plugin = record.type_id.plugin().expect("only a plugin's is frozen");
        let why = match info.reason {
            FrozenReason::Missing => format!("plugin {plugin} is not present"),
            FrozenReason::Version => format!("plugin {plugin} is not the version of its result"),
        };
        out.push(diagnostic(
            Severity::Info,
            info.reason.code(),
            format!("{why}; the stored result is used"),
            record.id,
        ));
    }
    for of in stale {
        let message = if *of == record.id {
            "its inputs changed since its result was stored; it is re-frozen when its plugin is \
             present"
                .to_owned()
        } else {
            format!("it reads the stored result of feature {of}, whose inputs have changed")
        };
        out.push(diagnostic(
            Severity::Warning,
            "plugin.frozen.stale",
            message,
            *of,
        ));
    }
    out
}

fn kernel_failure(e: KernelError) -> Failure {
    Failure {
        diagnostic: host::kernel_diagnostic(&e),
        call: e.call(),
    }
}

impl Evaluator {
    /// `record` from its stored result. Its inputs are resolved as they are
    /// now, without the type's form, and hashed as a live evaluation would
    /// hash them: unequal to the stored hash makes it stale. The outputs
    /// are the stored ones either way, under the stored hash, so what reads
    /// them hashes as it did when they were live.
    pub(super) fn compute_frozen(
        &mut self,
        scope: &Scope,
        record: &FeatureRecord,
        params: &Params,
        reason: FrozenReason,
    ) -> Result<Computed, Failure> {
        let Some(stored) = record.frozen.as_deref() else {
            return Err(error(
                reason.code(),
                format!(
                    "{} cannot be evaluated here and has no stored result",
                    record.type_id
                ),
            )
            .into());
        };
        let mut inputs = Vec::new();
        for (name, input) in &record.inputs {
            for r in input.refs() {
                inputs.push((name.clone(), resolve::blind(&mut self.kernel, scope, r)?));
            }
        }
        let mut values = Vec::new();
        for (name, expr) in &record.params {
            let si = params.si(expr).map_err(|mut d| {
                d.message = format!("`{name}`: {}", d.message);
                d
            })?;
            values.push((name.as_str(), si));
        }
        let plugin_version = stored.plugin_version.to_string();
        let current = InputHash::of_record(record, &plugin_version, &values, &inputs);
        let info = FrozenInfo {
            reason,
            stale: current != stored.input_hash,
        };
        let hash = stored.input_hash;
        let (cached, slots) = if let Some(entry) = self.cache.get_mut(&hash) {
            entry.used = self.run;
            (true, entry.slots.clone())
        } else {
            let slots = self.load(scope.doc, stored)?;
            self.cache.insert(
                hash,
                CacheEntry {
                    slots: slots.clone(),
                    frozen: None,
                    used: self.run,
                },
            );
            (false, slots)
        };
        Ok(Computed {
            hash,
            cached,
            slots,
            frozen: Some(info),
        })
    }

    /// The bodies of a stored result, named by its name table.
    fn load(
        &mut self,
        doc: &Document,
        stored: &FrozenResult,
    ) -> Result<BTreeMap<SlotName, SlotOut>, Failure> {
        let blob = |r: &BlobRef| -> Result<&[u8], Failure> {
            let bytes = doc
                .blob(r)
                .ok_or_else(|| error("blob.missing", format!("the document holds no blob {r}")))?;
            if r.matches(bytes) {
                Ok(bytes)
            } else {
                Err(error(
                    "blob.corrupt",
                    format!("blob {r} is not what its name says"),
                )
                .into())
            }
        };
        let corrupt = |why: String| Failure::from(error("blob.corrupt", why));
        let tables: BTreeMap<SlotName, NameTable> = serde_json::from_slice(blob(&stored.names)?)
            .map_err(|e| corrupt(format!("name table {}: {e}", stored.names)))?;
        let mut slots = BTreeMap::new();
        for (name, r) in &stored.slots {
            let table = tables.get(name).ok_or_else(|| {
                corrupt(format!("name table {} has no slot `{name}`", stored.names))
            })?;
            let body = self
                .kernel
                .from_bytes(blob(r)?, table)
                .map_err(kernel_failure)?;
            let measures = self.measure(body)?;
            slots.insert(name.clone(), SlotOut::Body { body, measures });
        }
        Ok(slots)
    }
}
