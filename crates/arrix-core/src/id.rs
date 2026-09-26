//! Ids of addressable records: 64 bits, written as 13 Crockford base32
//! characters, minted by the author of the command that creates the record
//! (docs/DATA-MODEL.md §Identifiers).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Crockford's alphabet: no I, L, O or U.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// The length of an id's text form: 13 × 5 bits covers 64, so the first
/// character carries only the top 4 bits (`0`–`F`).
pub const ID_LEN: usize = 13;

/// One 64-bit id. The typed ids wrap it; this is their shared form.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id(pub u64);

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum IdError {
    #[error("an id is {ID_LEN} characters, not {0}")]
    Length(usize),
    #[error("{0:?} is not a Crockford base32 character")]
    Character(char),
    #[error("{0} is past the largest id, 64 bits (its first character is at most F)")]
    Overflow(String),
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = [0u8; ID_LEN];
        for (i, c) in out.iter_mut().enumerate() {
            let shift = 5 * (ID_LEN - 1 - i);
            *c = ALPHABET[((self.0 >> shift) & 31) as usize];
        }
        f.write_str(std::str::from_utf8(&out).expect("the alphabet is ASCII"))
    }
}

/// A character's value: case-insensitive, with Crockford's aliases (`I`
/// and `L` read as 1, `O` as 0).
fn digit(c: char) -> Option<u64> {
    let c = match c.to_ascii_uppercase() {
        'I' | 'L' => '1',
        'O' => '0',
        c => c,
    };
    ALPHABET
        .iter()
        .position(|&a| char::from(a) == c)
        .map(|d| d as u64)
}

impl FromStr for Id {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, IdError> {
        let n = s.chars().count();
        if n != ID_LEN {
            return Err(IdError::Length(n));
        }
        let mut value = 0u64;
        for (i, c) in s.chars().enumerate() {
            let d = digit(c).ok_or(IdError::Character(c))?;
            if i == 0 && d > 0xF {
                return Err(IdError::Overflow(s.to_owned()));
            }
            value = (value << 5) | d;
        }
        Ok(Id(value))
    }
}

impl Serialize for Id {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Id {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = <std::borrow::Cow<'de, str>>::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// Typed ids: one kind of record each, so a part's id cannot be passed where
/// a feature's is wanted. Same text and serde form as [`Id`].
macro_rules! typed_ids {
    ($($(#[$doc:meta])* $name:ident),* $(,)?) => {$(
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Id);

        impl From<Id> for $name {
            fn from(id: Id) -> Self {
                Self(id)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl FromStr for $name {
            type Err = IdError;

            fn from_str(s: &str) -> Result<Self, IdError> {
                s.parse().map(Self)
            }
        }
    )*};
}

typed_ids! {
    /// A part.
    PartId,
    /// A feature record in a part's history.
    FeatureId,
    /// A parameter.
    ParamId,
    /// A point, curve or constraint in a sketch.
    SketchEntityId,
    /// A plugin's record in its own section.
    RecordId,
    /// A curve of a profile: the id of the sketch entity it came from, or
    /// a key a plugin chooses, stable across evaluations for the same
    /// logical curve. Side faces are rooted at it (docs/DATA-MODEL.md
    /// §Persistent naming).
    CurveKey,
}

/// Mints ids from a seed the caller chooses, so `arrix-core` reads no
/// entropy source and the same seed gives the same ids on every target.
/// Where a client's seed comes from is the client's (M1).
///
/// SplitMix64: every seed gives a full-period sequence of well-mixed
/// 64-bit values, in integer arithmetic alone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdMinter {
    state: u64,
}

impl IdMinter {
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// The next id, as any typed id.
    pub fn mint<T: From<Id>>(&mut self) -> T {
        T::from(self.next_id())
    }

    pub const fn next_id(&mut self) -> Id {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        Id(z ^ (z >> 31))
    }
}

/// The first ids of seed 1, pinned. Evaluated by the compiler for every
/// target it builds, the wasm build included, so a minter that drifted
/// between targets would not compile.
pub(crate) const SEED_1_PINNED: [u64; 3] = [
    0x910A_2DEC_8902_5CC1,
    0xBEEB_8DA1_658E_EC67,
    0xF893_A2EE_FB32_555E,
];

const _: () = {
    let mut minter = IdMinter::new(1);
    let mut i = 0;
    while i < SEED_1_PINNED.len() {
        assert!(minter.next_id().0 == SEED_1_PINNED[i]);
        i += 1;
    }
};

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn writes_thirteen_crockford_characters() {
        assert_eq!(Id(0).to_string(), "0000000000000");
        assert_eq!(Id(u64::MAX).to_string(), "FZZZZZZZZZZZZ");
        assert_eq!(Id(31).to_string(), "000000000000Z");
        assert_eq!(Id(1 << 60).to_string(), "1000000000000");
    }

    #[test]
    fn reads_any_case_and_the_aliases() {
        let id: Id = "7ZZZZZZZZZZZZ".parse().unwrap();
        assert_eq!("7zzzzzzzzzzzz".parse(), Ok(id));
        assert_eq!("0000000000001".parse(), "OOOOOOOOOOOOI".parse::<Id>());
        assert_eq!("0000000000001".parse(), "000000000000l".parse::<Id>());
    }

    #[test]
    fn refuses_what_is_not_an_id() {
        assert_eq!("".parse::<Id>(), Err(IdError::Length(0)));
        assert_eq!("00000000000000".parse::<Id>(), Err(IdError::Length(14)));
        assert_eq!("000000000000U".parse::<Id>(), Err(IdError::Character('U')));
        assert_eq!("000000000000-".parse::<Id>(), Err(IdError::Character('-')));
        assert!(matches!(
            "G000000000000".parse::<Id>(),
            Err(IdError::Overflow(_))
        ));
        assert!(serde_json::from_str::<PartId>("\"G000000000000\"").is_err());
        assert!(serde_json::from_str::<PartId>("12").is_err());
    }

    #[test]
    fn the_same_seed_mints_the_same_ids() {
        let mut minter = IdMinter::new(1);
        let ids: Vec<u64> = (0..3).map(|_| minter.next_id().0).collect();
        assert_eq!(ids, SEED_1_PINNED);
        let mut a = IdMinter::new(7);
        let mut b = IdMinter::new(7);
        let part: PartId = a.mint();
        assert_eq!(part, b.mint());
        assert_eq!(part.to_string(), "67JZ1WHCK43EQ");
        assert_ne!(a.mint::<FeatureId>().0, part.0);
    }

    proptest! {
        #[test]
        fn id_round_trips_through_text_and_json(n: u64) {
            let id = FeatureId(Id(n));
            let text = id.to_string();
            prop_assert_eq!(text.len(), ID_LEN);
            prop_assert_eq!(text.parse::<FeatureId>(), Ok(id));
            prop_assert_eq!(text.to_lowercase().parse::<FeatureId>(), Ok(id));
            let json = serde_json::to_string(&id).unwrap();
            prop_assert_eq!(&json, &format!("\"{text}\""));
            prop_assert_eq!(serde_json::from_str::<FeatureId>(&json).unwrap(), id);
        }
    }
}
