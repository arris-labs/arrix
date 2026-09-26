//! Dimensional analysis, before any value is known: a length where an
//! angle is wanted is refused here, never converted.

use std::fmt;

use arrix_core::QuantityKind;

use super::{Ast, BinOp, ExprError};

/// A dimension: the exponents of length, angle and mass. Count and ratio
/// are dimensionless.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Dim {
    pub length: i32,
    pub angle: i32,
    pub mass: i32,
}

impl Dim {
    pub const NONE: Dim = Dim {
        length: 0,
        angle: 0,
        mass: 0,
    };
    const ANGLE: Dim = Dim {
        length: 0,
        angle: 1,
        mass: 0,
    };

    /// The dimension a quantity of `kind` has.
    pub fn of(kind: QuantityKind) -> Dim {
        let mut d = Dim::NONE;
        match kind {
            QuantityKind::Length => d.length = 1,
            QuantityKind::Angle => d.angle = 1,
            QuantityKind::Mass => d.mass = 1,
            QuantityKind::Count | QuantityKind::Ratio => {}
        }
        d
    }

    fn zip(self, o: Dim, f: impl Fn(i32, i32) -> i32) -> Dim {
        Dim {
            length: f(self.length, o.length),
            angle: f(self.angle, o.angle),
            mass: f(self.mass, o.mass),
        }
    }

    /// `a length`, `an angle`, `a plain number`: for messages.
    pub fn with_article(self) -> String {
        let name = self.to_string();
        let article = if name.starts_with(['a', 'e', 'i', 'o', 'u']) {
            "an"
        } else {
            "a"
        };
        format!("{article} {name}")
    }

    fn scale(self, n: i32) -> Dim {
        self.zip(Dim::NONE, |a, _| a * n)
    }
}

impl fmt::Display for Dim {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == Dim::NONE {
            return f.write_str("plain number");
        }
        let mut first = true;
        for (name, e) in [
            ("length", self.length),
            ("angle", self.angle),
            ("mass", self.mass),
        ] {
            if e == 0 {
                continue;
            }
            if !first {
                f.write_str("·")?;
            }
            first = false;
            f.write_str(name)?;
            if e != 1 {
                write!(f, "^{e}")?;
            }
        }
        Ok(())
    }
}

/// Every function an expression may call.
pub const FUNCTIONS: &[&str] = &[
    "sin", "cos", "tan", "asin", "acos", "atan", "atan2", "sqrt", "abs", "min", "max", "round",
    "floor", "ceil",
];

/// Powers past this are a typo, not a quantity.
const MAX_EXPONENT: i32 = 12;

fn mismatch(what: &str, a: Dim, b: Dim) -> ExprError {
    ExprError::Dimension(format!(
        "cannot {what} {} and {}",
        a.with_article(),
        b.with_article()
    ))
}

fn need(function: &str, want: Dim, have: Dim) -> Result<(), ExprError> {
    if want == have {
        Ok(())
    } else {
        Err(ExprError::Dimension(format!(
            "`{function}` takes {}, not {}",
            want.with_article(),
            have.with_article()
        )))
    }
}

/// An exponent's value when it is a plain number, possibly negated.
fn constant(ast: &Ast) -> Option<f64> {
    match ast {
        Ast::Num(n, None) => Some(*n),
        Ast::Neg(a) => constant(a).map(|n| -n),
        _ => None,
    }
}

pub(crate) fn dim(
    ast: &Ast,
    kinds: &dyn Fn(&str) -> Option<QuantityKind>,
) -> Result<Dim, ExprError> {
    Ok(match ast {
        Ast::Num(_, unit) => unit.map_or(Dim::NONE, |u| Dim::of(u.kind)),
        Ast::Name(n) => Dim::of(kinds(n).ok_or_else(|| ExprError::UnknownName(n.clone()))?),
        Ast::Neg(a) => dim(a, kinds)?,
        Ast::Bin(op, a, b) => {
            let (da, db) = (dim(a, kinds)?, dim(b, kinds)?);
            match op {
                BinOp::Add | BinOp::Sub if da != db => {
                    let what = if *op == BinOp::Add { "add" } else { "subtract" };
                    return Err(mismatch(what, da, db));
                }
                BinOp::Add | BinOp::Sub => da,
                BinOp::Mul => da.zip(db, |x, y| x + y),
                BinOp::Div => da.zip(db, |x, y| x - y),
                BinOp::Pow => power(da, db, b)?,
            }
        }
        Ast::Call(name, args) => {
            let dims = args
                .iter()
                .map(|a| dim(a, kinds))
                .collect::<Result<Vec<_>, _>>()?;
            call(name, &dims)?
        }
    })
}

/// `base ^ exp`: anything dimensionless to a dimensionless power, or a
/// dimension to a whole constant.
fn power(base: Dim, exp: Dim, exp_ast: &Ast) -> Result<Dim, ExprError> {
    need("^", Dim::NONE, exp).map_err(|_| {
        ExprError::Dimension(format!(
            "an exponent is a plain number, not {}",
            exp.with_article()
        ))
    })?;
    if base == Dim::NONE {
        return Ok(Dim::NONE);
    }
    match constant(exp_ast) {
        Some(n) if n.fract() == 0.0 && n.abs() <= MAX_EXPONENT as f64 => Ok(base.scale(n as i32)),
        _ => Err(ExprError::Dimension(format!(
            "{} is raised only to a whole number written out, at most {MAX_EXPONENT}",
            base.with_article()
        ))),
    }
}

fn arity(function: &str, expected: &'static str, ok: bool, got: usize) -> Result<(), ExprError> {
    if ok {
        Ok(())
    } else {
        Err(ExprError::Arity {
            function: function.into(),
            expected,
            got,
        })
    }
}

fn call(name: &str, dims: &[Dim]) -> Result<Dim, ExprError> {
    let n = dims.len();
    let one = |ok_dim: &dyn Fn(Dim) -> Result<Dim, ExprError>| {
        arity(name, "one argument", n == 1, n)?;
        ok_dim(dims[0])
    };
    match name {
        "sin" | "cos" | "tan" => one(&|d| {
            if d == Dim::ANGLE || d == Dim::NONE {
                Ok(Dim::NONE)
            } else {
                need(name, Dim::ANGLE, d).map(|_| Dim::NONE)
            }
        }),
        "asin" | "acos" | "atan" => one(&|d| need(name, Dim::NONE, d).map(|_| Dim::ANGLE)),
        "round" | "floor" | "ceil" => one(&|d| need(name, Dim::NONE, d).map(|_| Dim::NONE)),
        "abs" => one(&Ok),
        "sqrt" => one(&|d| {
            if d.length % 2 == 0 && d.angle % 2 == 0 && d.mass % 2 == 0 {
                Ok(d.zip(Dim::NONE, |a, _| a / 2))
            } else {
                Err(ExprError::Dimension(format!(
                    "the square root of {} has no dimension",
                    d.with_article()
                )))
            }
        }),
        "atan2" => {
            arity(name, "two arguments", n == 2, n)?;
            if dims[0] != dims[1] {
                return Err(mismatch("compare", dims[0], dims[1]));
            }
            Ok(Dim::ANGLE)
        }
        "min" | "max" => {
            arity(name, "at least one argument", n >= 1, n)?;
            if let Some(d) = dims.iter().find(|d| **d != dims[0]) {
                return Err(mismatch("compare", dims[0], *d));
            }
            Ok(dims[0])
        }
        _ => Err(ExprError::UnknownFunction(name.into())),
    }
}
