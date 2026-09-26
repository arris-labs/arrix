//! Parameter expressions (docs/DATA-MODEL.md §Parameters and expressions):
//! numbers with unit suffixes, `+ - * / ^`, parentheses, functions and
//! parameter names. An [`Expr`] is its source text, parsed once; the text
//! is the recipe, so a diff shows what the user typed.
//!
//! Three passes, each pure: [`Expr::parse`] (syntax), [`Expr::check`]
//! (dimensions, against the parameters' kinds) and [`Expr::evaluate`] (the
//! value in SI, against the parameters' values). [`Expr::quantity`] runs the
//! last two for a slot of a known kind.

mod check;
mod eval;
mod parse;
mod print;
#[cfg(test)]
mod tests;

use std::collections::BTreeSet;
use std::fmt;

use arrix_core::{Diagnostic, Quantity, QuantityKind, Severity, Unit};
use serde::{Deserialize, Serialize};

pub use check::{Dim, FUNCTIONS};

/// An expression: its text as typed, and the tree parsed from it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Expr {
    text: String,
    ast: Ast,
}

/// The parsed tree. Numbers are never negative here: `-2` is a negation.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Ast {
    Num(f64, Option<Unit>),
    Name(String),
    Neg(Box<Ast>),
    Bin(BinOp, Box<Ast>, Box<Ast>),
    Call(String, Vec<Ast>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

/// Why an expression cannot be used. Each maps to a diagnostic code
/// (`expr.<kind>`).
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum ExprError {
    #[error("{message} at byte {at}")]
    Syntax { at: usize, message: String },
    #[error("`{0}` is not a unit")]
    UnknownUnit(String),
    #[error("no parameter is named `{0}`")]
    UnknownName(String),
    #[error("`{0}` is not a function")]
    UnknownFunction(String),
    #[error("`{function}` takes {expected}, not {got}")]
    Arity {
        function: String,
        expected: &'static str,
        got: usize,
    },
    #[error("{0}")]
    Dimension(String),
    #[error(
        "the expression is {}, where {} is wanted",
        have.with_article(),
        kind_with_article(*want)
    )]
    UnitMismatch { have: Dim, want: QuantityKind },
    #[error("the expression has no finite value")]
    NotFinite,
    #[error("a count is a whole number, and this evaluates to {0}")]
    NotWhole(f64),
}

/// `a length`, `an angle`, `a count`.
fn kind_with_article(kind: QuantityKind) -> String {
    let name = match kind {
        QuantityKind::Length => "a length",
        QuantityKind::Angle => "an angle",
        QuantityKind::Count => "a count",
        QuantityKind::Ratio => "a ratio",
        QuantityKind::Mass => "a mass",
    };
    name.into()
}

impl ExprError {
    /// The stable code of this failure.
    pub fn code(&self) -> &'static str {
        match self {
            ExprError::Syntax { .. } => "expr.syntax",
            ExprError::UnknownUnit(_) => "expr.unknown-unit",
            ExprError::UnknownName(_) => "expr.unknown-name",
            ExprError::UnknownFunction(_) => "expr.unknown-function",
            ExprError::Arity { .. } => "expr.arity",
            ExprError::Dimension(_) => "expr.dimension",
            ExprError::UnitMismatch { .. } => "expr.unit-mismatch",
            ExprError::NotFinite => "expr.not-finite",
            ExprError::NotWhole(_) => "expr.not-whole",
        }
    }

    /// As the user sees it; the caller adds what it is about (`with_refs`).
    pub fn diagnostic(&self) -> Diagnostic {
        Diagnostic::new(Severity::Error, self.code(), self.to_string())
            .expect("every expr code is well-formed")
    }
}

impl Expr {
    /// Parses `text`, keeping it as typed.
    pub fn parse(text: &str) -> Result<Self, ExprError> {
        Ok(Self {
            ast: parse::parse(text)?,
            text: text.to_owned(),
        })
    }

    /// The text as typed.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The parameter names it reads: its edges in the dependency DAG.
    pub fn names(&self) -> BTreeSet<&str> {
        let mut out = BTreeSet::new();
        self.ast.names(&mut out);
        out
    }

    /// Its dimension, given each parameter's kind. Dimensional analysis
    /// happens here, before any value is known.
    pub fn check(&self, kinds: &dyn Fn(&str) -> Option<QuantityKind>) -> Result<Dim, ExprError> {
        check::dim(&self.ast, kinds)
    }

    /// Its value in SI, given each parameter's value in SI. Assumes a
    /// successful [`check`](Self::check); a non-finite result is an error.
    pub fn evaluate(&self, values: &dyn Fn(&str) -> Option<f64>) -> Result<f64, ExprError> {
        let v = eval::value(&self.ast, values)?;
        if v.is_finite() {
            Ok(v)
        } else {
            Err(ExprError::NotFinite)
        }
    }

    /// Its value as a quantity of `kind`: checked against the kind, then
    /// evaluated. A count must be whole.
    pub fn quantity(
        &self,
        kind: QuantityKind,
        kinds: &dyn Fn(&str) -> Option<QuantityKind>,
        values: &dyn Fn(&str) -> Option<f64>,
    ) -> Result<Quantity, ExprError> {
        let have = self.check(kinds)?;
        if have != Dim::of(kind) {
            return Err(ExprError::UnitMismatch { have, want: kind });
        }
        let si = self.evaluate(values)?;
        if kind == QuantityKind::Count && si.fract() != 0.0 {
            return Err(ExprError::NotWhole(si));
        }
        Ok(Quantity::si(kind, si))
    }
}

impl Ast {
    fn names<'a>(&'a self, out: &mut BTreeSet<&'a str>) {
        match self {
            Ast::Num(..) => {}
            Ast::Name(n) => {
                out.insert(n);
            }
            Ast::Neg(a) => a.names(out),
            Ast::Bin(_, a, b) => {
                a.names(out);
                b.names(out);
            }
            Ast::Call(_, args) => args.iter().for_each(|a| a.names(out)),
        }
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl std::str::FromStr for Expr {
    type Err = ExprError;

    fn from_str(s: &str) -> Result<Self, ExprError> {
        Self::parse(s)
    }
}

impl TryFrom<String> for Expr {
    type Error = ExprError;

    fn try_from(text: String) -> Result<Self, ExprError> {
        Self::parse(&text)
    }
}

impl From<Expr> for String {
    fn from(e: Expr) -> String {
        e.text
    }
}
