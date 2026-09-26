//! A feature's input hash (docs/DATA-MODEL.md §Evaluation, step 2): BLAKE3
//! over the canonical serialisation of everything its outputs are a
//! function of. Equal hashes mean equal outputs, so the cache may reuse
//! them; anything left out here would be a stale result served silently.

use std::fmt;

use arrix_core::{FeatureId, Frame, Quantity};
use arrix_kernel::ARRIS_VERSION;
use arrix_plugin_api::InputValue;
use serde::{Deserialize, Serialize};

use crate::document::FeatureRecord;
use crate::registry::{FeatureArgs, FeatureType};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InputHash([u8; 32]);

/// What is hashed, in a fixed field order. Floats are written in their
/// shortest round-trip form, so equal values give equal bytes.
#[derive(Serialize)]
struct Canonical<'a> {
    /// The Arris release: every kernel result depends on it.
    arris: &'a str,
    /// Names are rooted at the feature, so two features never share a
    /// result.
    feature: FeatureId,
    type_id: &'a str,
    type_version: u32,
    plugin_version: Option<&'a str>,
    params: Vec<(&'a str, Quantity)>,
    choices: &'a std::collections::BTreeMap<String, String>,
    /// Each input as the geometry it resolved to.
    inputs: Vec<(&'a str, Frame)>,
}

impl InputHash {
    pub(crate) fn of(record: &FeatureRecord, ty: &dyn FeatureType, args: &FeatureArgs) -> Self {
        let canonical = Canonical {
            arris: ARRIS_VERSION,
            feature: record.id,
            type_id: &ty.spec().id,
            type_version: ty.spec().version,
            plugin_version: ty.plugin_version(),
            params: args
                .params
                .iter()
                .map(|p| (p.name.as_str(), p.value))
                .collect(),
            choices: &args.choices,
            inputs: args
                .inputs
                .iter()
                .map(|i| match i.value {
                    InputValue::Plane(f) => (i.name.as_str(), f),
                })
                .collect(),
        };
        let bytes = serde_json::to_vec(&canonical).expect("the canonical form serialises");
        Self(*blake3::hash(&bytes).as_bytes())
    }

    fn hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}

impl fmt::Display for InputHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.hex())
    }
}

impl fmt::Debug for InputHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "InputHash({})", self.hex())
    }
}

impl Serialize for InputHash {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.hex())
    }
}

impl<'de> Deserialize<'de> for InputHash {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        let bad = || serde::de::Error::custom(format!("{text:?} is not 64 hex digits"));
        if text.len() != 64 || !text.is_ascii() {
            return Err(bad());
        }
        let mut out = [0u8; 32];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&text[2 * i..2 * i + 2], 16).map_err(|_| bad())?;
        }
        Ok(Self(out))
    }
}
