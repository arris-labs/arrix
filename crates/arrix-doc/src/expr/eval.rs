//! Values in SI. Units were applied when the number was read, so a value
//! here is metres, radians or kilograms, and angles feed the trigonometric
//! functions as radians.

use super::{Ast, BinOp, ExprError};

pub(crate) fn value(ast: &Ast, values: &dyn Fn(&str) -> Option<f64>) -> Result<f64, ExprError> {
    Ok(match ast {
        Ast::Num(n, unit) => unit.map_or(*n, |u| u.to_si(*n)),
        Ast::Name(n) => values(n).ok_or_else(|| ExprError::UnknownName(n.clone()))?,
        Ast::Neg(a) => -value(a, values)?,
        Ast::Bin(op, a, b) => {
            let (x, y) = (value(a, values)?, value(b, values)?);
            match op {
                BinOp::Add => x + y,
                BinOp::Sub => x - y,
                BinOp::Mul => x * y,
                BinOp::Div => x / y,
                BinOp::Pow => x.powf(y),
            }
        }
        Ast::Call(name, args) => {
            let xs = args
                .iter()
                .map(|a| value(a, values))
                .collect::<Result<Vec<_>, _>>()?;
            call(name, &xs)?
        }
    })
}

fn call(name: &str, xs: &[f64]) -> Result<f64, ExprError> {
    let x = xs.first().copied().unwrap_or(f64::NAN);
    Ok(match name {
        "sin" => x.sin(),
        "cos" => x.cos(),
        "tan" => x.tan(),
        "asin" => x.asin(),
        "acos" => x.acos(),
        "atan" => x.atan(),
        "atan2" => x.atan2(xs.get(1).copied().unwrap_or(f64::NAN)),
        "sqrt" => x.sqrt(),
        "abs" => x.abs(),
        "round" => x.round(),
        "floor" => x.floor(),
        "ceil" => x.ceil(),
        "min" => xs.iter().copied().fold(f64::INFINITY, f64::min),
        "max" => xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        _ => return Err(ExprError::UnknownFunction(name.into())),
    })
}
