//! A plugin feature's stored result and the blobs it points at
//! (docs/DATA-MODEL.md §Frozen results, §File format). Blobs are
//! content-addressed: the file name is the BLAKE3 of the bytes.

use std::fmt;

use arrix_core::SlotName;
use serde::{Deserialize, Serialize};

use crate::eval::InputHash;

/// What a plugin feature last evaluated to, kept so the document opens
/// without the plugin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenResult {
    /// The feature's input hash when the result was stored.
    pub input_hash: InputHash,
    pub plugin_version: semver::Version,
    /// One blob of Arris body bytes per body slot.
    pub slots: std::collections::BTreeMap<SlotName, BlobRef>,
    /// The persistent-name table of every stored entity.
    pub names: BlobRef,
}

/// A blob's file name under `blobs/`: 64 lower-case hex digits of the
/// BLAKE3 of its bytes, a dot, and an extension of lower-case letters
/// and digits (`arrisbody`, `json`).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct BlobRef(String);

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0:?} is not a blob name: 64 hex digits of BLAKE3, a dot, then an extension")]
pub struct InvalidBlobRef(pub String);

impl BlobRef {
    /// The reference to `bytes` stored with extension `ext`.
    pub fn of(bytes: &[u8], ext: &str) -> Self {
        Self(format!("{}.{ext}", blake3::hash(bytes).to_hex()))
    }

    pub fn new(name: impl Into<String>) -> Result<Self, InvalidBlobRef> {
        let name = name.into();
        let ok = name.split_once('.').is_some_and(|(hash, ext)| {
            hash.len() == 64
                && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
                && !ext.is_empty()
                && ext
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        });
        if ok {
            Ok(Self(name))
        } else {
            Err(InvalidBlobRef(name))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Where the blob lives in a document: `blobs/<name>`.
    pub fn path(&self) -> String {
        format!("blobs/{}", self.0)
    }

    /// Whether `bytes` are what this name says they are.
    pub fn matches(&self, bytes: &[u8]) -> bool {
        self.0.starts_with(blake3::hash(bytes).to_hex().as_str())
    }
}

impl TryFrom<String> for BlobRef {
    type Error = InvalidBlobRef;

    fn try_from(name: String) -> Result<Self, InvalidBlobRef> {
        Self::new(name)
    }
}

impl From<BlobRef> for String {
    fn from(r: BlobRef) -> String {
        r.0
    }
}

impl fmt::Display for BlobRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for BlobRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BlobRef({})", self.0)
    }
}

impl FrozenResult {
    /// Every blob this result points at.
    pub fn blobs(&self) -> impl Iterator<Item = &BlobRef> {
        self.slots.values().chain(std::iter::once(&self.names))
    }
}
