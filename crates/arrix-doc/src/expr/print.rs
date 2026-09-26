//! Tree to canonical text, with the fewest parentheses that parse back to
//! the same tree. The stored text is what the user typed; this form is
//! what the round-trip property holds against.

use std::fmt;

use super::{Ast, BinOp};

impl Ast {
    /// Binding strength: sums, products, negation, powers, atoms.
    fn precedence(&self) -> u8 {
        match self {
            Ast::Bin(BinOp::Add | BinOp::Sub, ..) => 1,
            Ast::Bin(BinOp::Mul | BinOp::Div, ..) => 2,
            Ast::Neg(_) => 3,
            Ast::Bin(BinOp::Pow, ..) => 4,
            Ast::Num(..) | Ast::Name(_) | Ast::Call(..) => 5,
        }
    }
}

fn operand(f: &mut fmt::Formatter<'_>, a: &Ast, parens: bool) -> fmt::Result {
    if parens {
        write!(f, "({a})")
    } else {
        write!(f, "{a}")
    }
}

impl fmt::Display for Ast {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let p = self.precedence();
        match self {
            Ast::Num(n, None) => write!(f, "{n}"),
            Ast::Num(n, Some(unit)) => write!(f, "{n} {}", unit.symbol),
            Ast::Name(name) => f.write_str(name),
            Ast::Neg(a) => {
                f.write_str("-")?;
                operand(f, a, a.precedence() < p)
            }
            // Right-associative, and its exponent is parsed as a unary.
            Ast::Bin(BinOp::Pow, a, b) => {
                operand(f, a, a.precedence() <= p)?;
                f.write_str("^")?;
                operand(f, b, b.precedence() < 3)
            }
            Ast::Bin(op, a, b) => {
                operand(f, a, a.precedence() < p)?;
                let sym = match op {
                    BinOp::Add => " + ",
                    BinOp::Sub => " - ",
                    BinOp::Mul => " * ",
                    BinOp::Div => " / ",
                    BinOp::Pow => unreachable!("matched above"),
                };
                f.write_str(sym)?;
                operand(f, b, b.precedence() <= p)
            }
            Ast::Call(name, args) => {
                write!(f, "{name}(")?;
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{a}")?;
                }
                f.write_str(")")
            }
        }
    }
}
