//! Persistent names: a face, edge or vertex named by where it came from,
//! never by where it is (docs/DATA-MODEL.md §Persistent naming).
//! `arrix-kernel` computes them from Arris's provenance; here they are
//! plain data with one canonical text form, which is how they cross the
//! plugin boundary (the WIT world's `persistent-ref`) and how they are
//! written in a document.
//!
//! ```text
//! name  := kind ':' root ('/' step)*
//! kind  := 'face' | 'edge' | 'vertex'
//! root  := 'sweep.' ID '.' part | 'plugin.' ID '.' ID | 'frozen.' ID '.' N
//! part  := 'start-cap' | 'end-cap' | curve-part '.' ID
//! step  := 'mod.' ID '.' N | 'gen.' ID '[' name (',' name)* ']'
//! ```
//!
//! `ID` is an id's 13 Crockford characters, `N` a decimal `u32` without
//! leading zeros: `face:sweep.<feature>.side.<curve>/mod.<feature>.0`.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::{CurveKey, FeatureId, ID_LEN, Id};

/// How deep `gen` steps may nest in a name's text. Real chains are a few
/// levels; the bound keeps a hostile string from exhausting the stack.
pub const MAX_NAME_DEPTH: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TopoKind {
    Face,
    Edge,
    Vertex,
}

/// A face, edge or vertex: a root and what happened to it since.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PersistentName {
    pub kind: TopoKind,
    pub root: NameRoot,
    pub chain: Vec<NameStep>,
}

/// Where a chain starts: what made the entity from nothing. `Primitive`
/// and `Imported` roots join with primitives and import.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NameRoot {
    /// A part of a sweep (extrude or revolve) made by `feature`.
    Sweep {
        feature: FeatureId,
        part: SweepPartName,
    },
    /// Topology a plugin feature built directly, at a key it chose
    /// (Arris ask A1).
    Plugin { feature: FeatureId, key: Id },
    /// An entity of a frozen result, by its index in the stored name table.
    Frozen { feature: FeatureId, index: u32 },
}

/// A part of a sweep, in ArriX's words: the caps, and the rest by the key
/// of the profile curve it came from. A vertex-derived part (`rise`,
/// `start-vertex`, `end-vertex`) is named by the curve that starts there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SweepPartName {
    StartCap,
    EndCap,
    Side(CurveKey),
    StartEdge(CurveKey),
    EndEdge(CurveKey),
    Rise(CurveKey),
    StartVertex(CurveKey),
    EndVertex(CurveKey),
    Cavity(CurveKey),
}

/// What a later feature did to an entity.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NameStep {
    /// A piece of it; `split` is 0 when it was kept whole but changed.
    Modified { feature: FeatureId, split: u32 },
    /// Made from these (an edge from two faces).
    Generated {
        feature: FeatureId,
        from: Vec<PersistentName>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{text:?} is not a persistent name: {reason} at byte {at}")]
pub struct NameError {
    pub text: String,
    pub at: usize,
    pub reason: &'static str,
}

impl TopoKind {
    fn as_str(self) -> &'static str {
        match self {
            TopoKind::Face => "face",
            TopoKind::Edge => "edge",
            TopoKind::Vertex => "vertex",
        }
    }
}

impl SweepPartName {
    fn word(self) -> (&'static str, Option<CurveKey>) {
        match self {
            SweepPartName::StartCap => ("start-cap", None),
            SweepPartName::EndCap => ("end-cap", None),
            SweepPartName::Side(k) => ("side", Some(k)),
            SweepPartName::StartEdge(k) => ("start-edge", Some(k)),
            SweepPartName::EndEdge(k) => ("end-edge", Some(k)),
            SweepPartName::Rise(k) => ("rise", Some(k)),
            SweepPartName::StartVertex(k) => ("start-vertex", Some(k)),
            SweepPartName::EndVertex(k) => ("end-vertex", Some(k)),
            SweepPartName::Cavity(k) => ("cavity", Some(k)),
        }
    }

    fn from_word(word: &str, key: CurveKey) -> Option<Self> {
        Some(match word {
            "side" => SweepPartName::Side(key),
            "start-edge" => SweepPartName::StartEdge(key),
            "end-edge" => SweepPartName::EndEdge(key),
            "rise" => SweepPartName::Rise(key),
            "start-vertex" => SweepPartName::StartVertex(key),
            "end-vertex" => SweepPartName::EndVertex(key),
            "cavity" => SweepPartName::Cavity(key),
            _ => return None,
        })
    }
}

impl fmt::Display for PersistentName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:", self.kind.as_str())?;
        match &self.root {
            NameRoot::Sweep { feature, part } => {
                write!(f, "sweep.{feature}.")?;
                match part.word() {
                    (word, Some(key)) => write!(f, "{word}.{key}")?,
                    (word, None) => f.write_str(word)?,
                }
            }
            NameRoot::Plugin { feature, key } => write!(f, "plugin.{feature}.{key}")?,
            NameRoot::Frozen { feature, index } => write!(f, "frozen.{feature}.{index}")?,
        }
        for step in &self.chain {
            match step {
                NameStep::Modified { feature, split } => write!(f, "/mod.{feature}.{split}")?,
                NameStep::Generated { feature, from } => {
                    write!(f, "/gen.{feature}[")?;
                    for (i, name) in from.iter().enumerate() {
                        if i > 0 {
                            f.write_str(",")?;
                        }
                        name.fmt(f)?;
                    }
                    f.write_str("]")?;
                }
            }
        }
        Ok(())
    }
}

/// A recursive-descent reader over the name grammar.
struct Reader<'a> {
    text: &'a str,
    at: usize,
}

type Read<T> = Result<T, (usize, &'static str)>;

impl<'a> Reader<'a> {
    fn rest(&self) -> &'a str {
        &self.text[self.at..]
    }

    fn fail<T>(&self, reason: &'static str) -> Read<T> {
        Err((self.at, reason))
    }

    fn eat(&mut self, lit: &str) -> bool {
        let found = self.rest().starts_with(lit);
        if found {
            self.at += lit.len();
        }
        found
    }

    fn expect(&mut self, lit: &str, reason: &'static str) -> Read<()> {
        if self.eat(lit) {
            Ok(())
        } else {
            self.fail(reason)
        }
    }

    /// Lower-case letters and hyphens, up to the next delimiter.
    fn word(&mut self) -> &'a str {
        let rest = self.rest();
        let n = rest
            .find(|c: char| !(c.is_ascii_lowercase() || c == '-'))
            .unwrap_or(rest.len());
        self.at += n;
        &rest[..n]
    }

    fn id(&mut self) -> Read<Id> {
        let rest = self.rest();
        let Some(text) = rest.get(..ID_LEN) else {
            return self.fail("expected a 13-character id");
        };
        let id = text
            .parse()
            .or_else(|_| self.fail("expected a 13-character id"))?;
        self.at += ID_LEN;
        Ok(id)
    }

    fn number(&mut self) -> Read<u32> {
        let rest = self.rest();
        let n = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        let digits = &rest[..n];
        if digits.is_empty() || (digits.len() > 1 && digits.starts_with('0')) {
            return self.fail("expected a decimal number without leading zeros");
        }
        let value = digits.parse().or_else(|_| self.fail("number past u32"))?;
        self.at += n;
        Ok(value)
    }

    fn name(&mut self, depth: usize) -> Read<PersistentName> {
        if depth > MAX_NAME_DEPTH {
            return self.fail("names nested too deep");
        }
        let start = self.at;
        let kind = match self.word() {
            "face" => TopoKind::Face,
            "edge" => TopoKind::Edge,
            "vertex" => TopoKind::Vertex,
            _ => return Err((start, "expected face, edge or vertex")),
        };
        self.expect(":", "expected ':' after the kind")?;
        let root = self.root()?;
        let mut chain = Vec::new();
        while self.eat("/") {
            chain.push(self.step(depth)?);
        }
        Ok(PersistentName { kind, root, chain })
    }

    fn root(&mut self) -> Read<NameRoot> {
        let start = self.at;
        let word = self.word();
        if !["sweep", "plugin", "frozen"].contains(&word) {
            return Err((start, "expected sweep, plugin or frozen"));
        }
        self.expect(".", "expected '.' after the root's kind")?;
        let feature = FeatureId(self.id()?);
        self.expect(".", "expected '.' after the root's feature")?;
        match word {
            "sweep" => {
                let part_at = self.at;
                let part = match self.word() {
                    "start-cap" => SweepPartName::StartCap,
                    "end-cap" => SweepPartName::EndCap,
                    other => {
                        let known = SweepPartName::from_word(other, CurveKey(Id(0))).is_some();
                        if !known {
                            return Err((part_at, "not a sweep part"));
                        }
                        self.expect(".", "expected '.' after the sweep part")?;
                        let key = CurveKey(self.id()?);
                        SweepPartName::from_word(other, key).expect("the word was checked")
                    }
                };
                Ok(NameRoot::Sweep { feature, part })
            }
            "plugin" => Ok(NameRoot::Plugin {
                feature,
                key: self.id()?,
            }),
            "frozen" => Ok(NameRoot::Frozen {
                feature,
                index: self.number()?,
            }),
            _ => unreachable!("the root's kind was checked"),
        }
    }

    fn step(&mut self, depth: usize) -> Read<NameStep> {
        let start = self.at;
        let word = self.word();
        if !["mod", "gen"].contains(&word) {
            return Err((start, "expected mod or gen"));
        }
        self.expect(".", "expected '.' after the step's kind")?;
        let feature = FeatureId(self.id()?);
        match word {
            "mod" => {
                self.expect(".", "expected '.' before the split")?;
                Ok(NameStep::Modified {
                    feature,
                    split: self.number()?,
                })
            }
            "gen" => {
                self.expect("[", "expected '[' after gen's feature")?;
                let mut from = vec![self.name(depth + 1)?];
                while self.eat(",") {
                    from.push(self.name(depth + 1)?);
                }
                self.expect("]", "expected ',' or ']'")?;
                Ok(NameStep::Generated { feature, from })
            }
            _ => unreachable!("the step's kind was checked"),
        }
    }
}

impl FromStr for PersistentName {
    type Err = NameError;

    fn from_str(text: &str) -> Result<Self, NameError> {
        let mut r = Reader { text, at: 0 };
        let parsed = r.name(0).and_then(|n| {
            if r.rest().is_empty() {
                Ok(n)
            } else {
                r.fail("trailing text")
            }
        });
        parsed.map_err(|(at, reason)| NameError {
            text: text.to_owned(),
            at,
            reason,
        })
    }
}

impl TryFrom<String> for PersistentName {
    type Error = NameError;

    fn try_from(text: String) -> Result<Self, NameError> {
        text.parse()
    }
}

impl From<PersistentName> for String {
    fn from(name: PersistentName) -> String {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn fid(n: u64) -> FeatureId {
        FeatureId(Id(n))
    }

    fn side(feature: u64, curve: u64) -> PersistentName {
        PersistentName {
            kind: TopoKind::Face,
            root: NameRoot::Sweep {
                feature: fid(feature),
                part: SweepPartName::Side(CurveKey(Id(curve))),
            },
            chain: vec![],
        }
    }

    #[test]
    fn writes_the_documented_form() {
        let mut face = side(1, 2);
        face.chain.push(NameStep::Modified {
            feature: fid(3),
            split: 0,
        });
        assert_eq!(
            face.to_string(),
            "face:sweep.0000000000001.side.0000000000002/mod.0000000000003.0"
        );
        let cap = PersistentName {
            kind: TopoKind::Face,
            root: NameRoot::Sweep {
                feature: fid(1),
                part: SweepPartName::EndCap,
            },
            chain: vec![],
        };
        let rim = PersistentName {
            kind: TopoKind::Edge,
            root: NameRoot::Frozen {
                feature: fid(4),
                index: 17,
            },
            chain: vec![NameStep::Generated {
                feature: fid(5),
                from: vec![face, cap],
            }],
        };
        let text = rim.to_string();
        assert_eq!(
            text,
            "edge:frozen.0000000000004.17/gen.0000000000005[\
             face:sweep.0000000000001.side.0000000000002/mod.0000000000003.0,\
             face:sweep.0000000000001.end-cap]"
        );
        assert_eq!(text.parse::<PersistentName>(), Ok(rim.clone()));
        let json = serde_json::to_string(&rim).unwrap();
        assert_eq!(json, format!("\"{text}\""));
        assert_eq!(serde_json::from_str::<PersistentName>(&json).unwrap(), rim);
    }

    #[test]
    fn refuses_what_breaks_the_grammar() {
        let good = "face:sweep.0000000000001.side.0000000000002";
        assert!(good.parse::<PersistentName>().is_ok());
        for (bad, at) in [
            ("", 0),
            ("solid:sweep.0000000000001.end-cap", 0),
            ("face;sweep.0000000000001.end-cap", 4),
            ("face:swept.0000000000001.end-cap", 5),
            ("face:sweep.000000000001.end-cap", 11),
            ("face:sweep.0000000000001.top", 25),
            ("face:sweep.0000000000001.side", 29),
            ("face:sweep.0000000000001.end-cap/", 33),
            ("face:sweep.0000000000001.end-cap/mod.0000000000001.01", 51),
            (
                "face:sweep.0000000000001.end-cap/mod.0000000000001.4294967296",
                51,
            ),
            ("face:sweep.0000000000001.end-cap/gen.0000000000001[]", 51),
            (
                "face:sweep.0000000000001.end-cap/gen.0000000000001[face:x",
                56,
            ),
            ("face:sweep.0000000000001.end-cap ", 32),
            ("face:frozen.0000000000001.-1", 26),
        ] {
            let err = bad.parse::<PersistentName>().unwrap_err();
            assert_eq!(err.at, at, "{bad}: {}", err.reason);
        }
        assert!(serde_json::from_str::<PersistentName>("\"face:\"").is_err());
    }

    #[test]
    fn refuses_names_nested_past_the_bound() {
        let mut text = String::from("face:sweep.0000000000001.end-cap");
        for _ in 0..=MAX_NAME_DEPTH {
            text = format!("edge:sweep.0000000000001.end-cap/gen.0000000000002[{text}]");
        }
        let err = text.parse::<PersistentName>().unwrap_err();
        assert_eq!(err.reason, "names nested too deep");
    }

    fn arb_id() -> BoxedStrategy<Id> {
        any::<u64>().prop_map(Id).boxed()
    }

    fn arb_part() -> impl Strategy<Value = SweepPartName> {
        let key = arb_id().prop_map(CurveKey);
        prop_oneof![
            Just(SweepPartName::StartCap),
            Just(SweepPartName::EndCap),
            key.clone().prop_map(SweepPartName::Side),
            key.clone().prop_map(SweepPartName::StartEdge),
            key.clone().prop_map(SweepPartName::EndEdge),
            key.clone().prop_map(SweepPartName::Rise),
            key.clone().prop_map(SweepPartName::StartVertex),
            key.clone().prop_map(SweepPartName::EndVertex),
            key.prop_map(SweepPartName::Cavity),
        ]
    }

    fn arb_root() -> impl Strategy<Value = NameRoot> {
        let feature = arb_id().prop_map(FeatureId);
        prop_oneof![
            (feature.clone(), arb_part())
                .prop_map(|(feature, part)| NameRoot::Sweep { feature, part }),
            (feature.clone(), arb_id())
                .prop_map(|(feature, key)| NameRoot::Plugin { feature, key }),
            (feature, any::<u32>())
                .prop_map(|(feature, index)| NameRoot::Frozen { feature, index }),
        ]
    }

    fn arb_kind() -> impl Strategy<Value = TopoKind> {
        prop_oneof![
            Just(TopoKind::Face),
            Just(TopoKind::Edge),
            Just(TopoKind::Vertex)
        ]
    }

    fn arb_name() -> impl Strategy<Value = PersistentName> {
        let leaf = (
            arb_kind(),
            arb_root(),
            prop::collection::vec((arb_id(), any::<u32>()), 0..3),
        )
            .prop_map(|(kind, root, mods)| PersistentName {
                kind,
                root,
                chain: mods
                    .into_iter()
                    .map(|(f, split)| NameStep::Modified {
                        feature: FeatureId(f),
                        split,
                    })
                    .collect(),
            });
        leaf.prop_recursive(3, 16, 3, |inner| {
            (
                arb_kind(),
                arb_root(),
                arb_id(),
                prop::collection::vec(inner, 1..3),
            )
                .prop_map(|(kind, root, f, from)| PersistentName {
                    kind,
                    root,
                    chain: vec![NameStep::Generated {
                        feature: FeatureId(f),
                        from,
                    }],
                })
        })
    }

    proptest! {
        #[test]
        fn a_name_round_trips_through_text_and_json(name in arb_name()) {
            let text = name.to_string();
            prop_assert_eq!(text.parse::<PersistentName>(), Ok(name.clone()));
            let json = serde_json::to_string(&name).unwrap();
            prop_assert_eq!(serde_json::from_str::<PersistentName>(&json).unwrap(), name);
        }

        #[test]
        fn reading_any_text_never_panics(text in "[a-z:./\\[\\],0-9A-Z-]{0,80}") {
            let _ = text.parse::<PersistentName>();
        }
    }
}
