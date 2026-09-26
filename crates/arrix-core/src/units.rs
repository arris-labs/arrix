//! Quantities and units. SI inside: a value is stored in metres, radians or
//! kilograms, and a unit is only how it is typed or shown
//! (docs/DATA-MODEL.md §Parameters and expressions).

use serde::{Deserialize, Serialize};

/// The one absolute length tolerance upstream of the kernel, in metres: the
/// micrometre `arrix-kernel` hands Arris as `default_tolerance`
/// (`Kernel::DEFAULT_TOLERANCE`, docs/ARCHITECTURE.md §The kernel choke point), so a crate that names no
/// kernel (the sketcher) compares lengths the way the kernel does.
pub const LENGTH_TOLERANCE: f64 = 1.0e-6;

/// What a number measures. Count and ratio are dimensionless.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuantityKind {
    Length,
    Angle,
    Count,
    Ratio,
    Mass,
}

/// A unit a value can be typed or shown in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Unit {
    pub symbol: &'static str,
    pub kind: QuantityKind,
    /// One of this unit in SI, as `num / den`: kept a ratio so that
    /// `12 mm` is `12 / 1000`, rounded once.
    num: f64,
    den: f64,
}

const fn unit(symbol: &'static str, kind: QuantityKind, num: f64, den: f64) -> Unit {
    Unit {
        symbol,
        kind,
        num,
        den,
    }
}

/// Every unit a value can carry.
pub const UNITS: &[Unit] = &[
    unit("mm", QuantityKind::Length, 1.0, 1000.0),
    unit("cm", QuantityKind::Length, 1.0, 100.0),
    unit("m", QuantityKind::Length, 1.0, 1.0),
    // 25.4 mm exactly.
    unit("in", QuantityKind::Length, 254.0, 10_000.0),
    unit("deg", QuantityKind::Angle, std::f64::consts::PI, 180.0),
    unit("rad", QuantityKind::Angle, 1.0, 1.0),
    unit("g", QuantityKind::Mass, 1.0, 1000.0),
    unit("kg", QuantityKind::Mass, 1.0, 1.0),
];

impl Unit {
    /// The unit written `symbol`, exactly as the table spells it.
    pub fn parse(symbol: &str) -> Option<Unit> {
        UNITS.iter().find(|u| u.symbol == symbol).copied()
    }

    pub fn to_si(&self, value: f64) -> f64 {
        value * self.num / self.den
    }

    pub fn from_si(&self, si: f64) -> f64 {
        si * self.den / self.num
    }
}

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
#[error("a {have:?} cannot be expressed in {unit}, a unit of {want:?}")]
pub struct UnitMismatch {
    pub have: QuantityKind,
    pub unit: &'static str,
    pub want: QuantityKind,
}

/// A value of a kind, in SI.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Quantity {
    pub kind: QuantityKind,
    pub si: f64,
}

impl Quantity {
    /// `value` in `unit`, e.g. `Quantity::new(12.0, mm)` is 0.012 m.
    pub fn new(value: f64, unit: Unit) -> Self {
        Self {
            kind: unit.kind,
            si: unit.to_si(value),
        }
    }

    /// A value already in SI, or a dimensionless count or ratio.
    pub fn si(kind: QuantityKind, si: f64) -> Self {
        Self { kind, si }
    }

    /// The value in `unit`, which must measure the same kind.
    pub fn in_unit(&self, unit: Unit) -> Result<f64, UnitMismatch> {
        if unit.kind != self.kind {
            return Err(UnitMismatch {
                have: self.kind,
                unit: unit.symbol,
                want: unit.kind,
            });
        }
        Ok(unit.from_si(self.si))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn u(symbol: &str) -> Unit {
        Unit::parse(symbol).unwrap()
    }

    #[test]
    fn converts_to_si() {
        assert_eq!(Quantity::new(12.0, u("mm")).si, 0.012);
        assert_eq!(Quantity::new(3.0, u("cm")).si, 0.03);
        assert_eq!(Quantity::new(1.0, u("in")).si, 0.0254);
        assert_eq!(Quantity::new(180.0, u("deg")).si, std::f64::consts::PI);
        assert_eq!(Quantity::new(250.0, u("g")).si, 0.25);
        assert_eq!(Quantity::new(2.0, u("kg")).kind, QuantityKind::Mass);
        assert_eq!(Quantity::new(1.0, u("in")).in_unit(u("mm")), Ok(25.4));
    }

    #[test]
    fn refuses_a_unit_of_another_kind() {
        let angle = Quantity::new(30.0, u("deg"));
        assert_eq!(
            angle.in_unit(u("mm")),
            Err(UnitMismatch {
                have: QuantityKind::Angle,
                unit: "mm",
                want: QuantityKind::Length
            })
        );
        assert_eq!(Unit::parse("MM"), None);
        assert_eq!(Unit::parse("ft"), None);
    }

    proptest! {
        #[test]
        fn unit_conversion_round_trips(value in -1e9f64..1e9, i in 0..UNITS.len()) {
            let unit = UNITS[i];
            let back = Quantity::new(value, unit).in_unit(unit).unwrap();
            prop_assert!((back - value).abs() <= 1e-12 * value.abs().max(1.0), "{} {} -> {}", value, unit.symbol, back);
        }

        #[test]
        fn quantity_round_trips_through_json(si in proptest::num::f64::NORMAL, i in 0..UNITS.len()) {
            let q = Quantity::si(UNITS[i].kind, si);
            let json = serde_json::to_string(&q).unwrap();
            prop_assert_eq!(serde_json::from_str::<Quantity>(&json).unwrap(), q);
        }
    }
}
