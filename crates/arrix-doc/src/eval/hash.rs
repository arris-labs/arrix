//! A feature's input hash (docs/DATA-MODEL.md §Evaluation, step 2): BLAKE3
//! over the canonical serialisation of everything its outputs are a
//! function of. Equal hashes mean equal outputs, so the cache may reuse
//! them; anything left out here would be a stale result served silently.

use std::fmt;

use arrix_core::{Axis, DVec3, FeatureId, Frame, PersistentName, Profile, SlotName};
use arrix_kernel::ARRIS_VERSION;
use arrix_plugin_api::{InputValue, Point};
use serde::{Deserialize, Serialize};

use super::BodyRead;
use crate::document::FeatureRecord;
use crate::registry::{FeatureArgs, FeatureType};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InputHash([u8; 32]);

/// What is hashed, in a fixed field order. Floats are written in their
/// shortest round-trip form, so equal values give equal bytes.
///
/// It is built from the record, not the type's form, so a feature whose
/// plugin is absent hashes as it did when its plugin was there: that is how
/// a frozen result's staleness is decided (docs/DATA-MODEL.md §Frozen
/// results). Parameters are the record's own expressions' values, by name,
/// since a default is the type's and the type's version is hashed.
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
    /// In SI, sorted by name.
    params: Vec<(&'a str, f64)>,
    choices: &'a std::collections::BTreeMap<String, String>,
    /// Each input as the geometry it resolved to, sorted by name.
    inputs: Vec<(&'a str, Input<'a>)>,
    /// A sketch as its solve reads it: stored positions and resolved
    /// dimensions. Left out when there is none, so no other hash moves.
    #[serde(skip_serializing_if = "Option::is_none")]
    sketch: Option<&'a arrix_sketch::Sketch>,
}

/// An input's geometry, written bare: a plane as its frame, as before
/// regions were inputs, so no plane's hash moved when they came. A body is
/// the feature that produced its version, by that feature's own input
/// hash, and the slot: evaluation is deterministic, so that identifies it.
#[derive(Serialize)]
#[serde(untagged)]
enum Input<'a> {
    Plane(Frame),
    Region(&'a Profile),
    Body {
        produced_by: InputHash,
        slot: &'a SlotName,
    },
    /// An edge by its name: which body it is on is the call's to check.
    Edge(&'a PersistentName),
    Point {
        at: DVec3,
        normal: Option<DVec3>,
    },
    Axis {
        origin: DVec3,
        direction: DVec3,
    },
    /// The tool a feature kept, by that feature's input hash.
    Tool {
        tool_of: InputHash,
    },
}

/// An input as resolved without its type's form, by the shape of its
/// reference (a frozen feature's plugin may be absent).
#[derive(Clone, Debug)]
pub(crate) enum Resolved {
    Plane(Frame),
    Region(Profile),
    Body(BodyRead),
    Edge(PersistentName),
    Point(Point),
    Axis(Axis),
    Tool(BodyRead),
}

impl InputHash {
    /// `bodies` is what the body inputs resolved to, by handle.
    pub(crate) fn of(
        record: &FeatureRecord,
        ty: &dyn FeatureType,
        args: &FeatureArgs,
        bodies: &[BodyRead],
    ) -> Self {
        let mut params: Vec<(&str, f64)> = args
            .params
            .iter()
            .filter(|p| record.params.contains_key(&p.name))
            .map(|p| (p.name.as_str(), p.value.si))
            .collect();
        params.sort_by(|a, b| a.0.cmp(b.0));
        let mut inputs: Vec<(&str, Input)> = args
            .inputs
            .iter()
            .map(|i| {
                let value = match &i.value {
                    InputValue::Plane(f) => Input::Plane(*f),
                    InputValue::Region(p) => Input::Region(p),
                    InputValue::Body(b) => {
                        let read = &bodies[b.handle() as usize];
                        Input::Body {
                            produced_by: read.hash,
                            slot: &read.slot,
                        }
                    }
                    InputValue::Edge(n) => Input::Edge(n),
                    InputValue::Point(p) => Input::Point {
                        at: p.at,
                        normal: p.normal,
                    },
                    InputValue::Axis(a) => Input::Axis {
                        origin: a.origin,
                        direction: a.direction,
                    },
                    InputValue::Tool(t) => Input::Tool {
                        tool_of: bodies[t.body.handle() as usize].hash,
                    },
                };
                (i.name.as_str(), value)
            })
            .collect();
        inputs.sort_by(|a, b| a.0.cmp(b.0));
        Self::canonical(&Canonical {
            arris: ARRIS_VERSION,
            feature: record.id,
            type_id: &ty.spec().id,
            type_version: ty.spec().version,
            plugin_version: ty.plugin_version(),
            params,
            choices: &args.choices,
            inputs,
            sketch: args.sketch.as_ref().map(|s| &s.sketch),
        })
    }

    /// The hash `of` gives a plugin feature that evaluated under
    /// `plugin_version`, from its record and what its parameters and inputs
    /// resolve to now, without the plugin's form.
    pub(crate) fn of_record(
        record: &FeatureRecord,
        plugin_version: &str,
        params: &[(&str, f64)],
        inputs: &[(String, Resolved)],
    ) -> Self {
        let mut params = params.to_vec();
        params.sort_by(|a, b| a.0.cmp(b.0));
        let mut inputs: Vec<(&str, Input)> = inputs
            .iter()
            .map(|(name, r)| {
                let value = match r {
                    Resolved::Plane(f) => Input::Plane(*f),
                    Resolved::Region(p) => Input::Region(p),
                    Resolved::Body(read) => Input::Body {
                        produced_by: read.hash,
                        slot: &read.slot,
                    },
                    Resolved::Edge(n) => Input::Edge(n),
                    Resolved::Point(p) => Input::Point {
                        at: p.at,
                        normal: p.normal,
                    },
                    Resolved::Axis(a) => Input::Axis {
                        origin: a.origin,
                        direction: a.direction,
                    },
                    Resolved::Tool(read) => Input::Tool { tool_of: read.hash },
                };
                (name.as_str(), value)
            })
            .collect();
        inputs.sort_by(|a, b| a.0.cmp(b.0));
        Self::canonical(&Canonical {
            arris: ARRIS_VERSION,
            feature: record.id,
            type_id: record.type_id.as_str(),
            type_version: record.type_version,
            plugin_version: Some(plugin_version),
            params,
            choices: &record.choices,
            inputs,
            sketch: None,
        })
    }

    fn canonical(canonical: &Canonical) -> Self {
        let bytes = serde_json::to_vec(canonical).expect("the canonical form serialises");
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
