//! Text to tree. A unit is a suffix of a number and nothing else, so `2 m`
//! is two metres while a bare `m` is the parameter named `m`.
//!
//! ```text
//! sum     := product (('+' | '-') product)*
//! product := unary (('*' | '/') unary)*
//! unary   := '-' unary | power
//! power   := atom ('^' unary)?          right-associative
//! atom    := NUMBER UNIT? | NAME '(' sum (',' sum)* ')' | NAME | '(' sum ')'
//! ```

use arrix_core::Unit;

use super::{Ast, BinOp, ExprError};

/// How deep parentheses and prefix minuses may nest: far past anything
/// typed, and short of exhausting the stack on a hostile string.
const MAX_DEPTH: usize = 64;

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Ident(String),
    Sym(char),
    End,
}

fn lex(text: &str) -> Result<Vec<(usize, Tok)>, ExprError> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        let start = i;
        if c.is_ascii_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() || c == b'.' {
            i = number_end(bytes, i);
            let value: f64 = text[start..i].parse().map_err(|_| ExprError::Syntax {
                at: start,
                message: format!("`{}` is not a number", &text[start..i]),
            })?;
            if !value.is_finite() {
                return Err(ExprError::Syntax {
                    at: start,
                    message: "the number is out of range".into(),
                });
            }
            out.push((start, Tok::Num(value)));
        } else if c.is_ascii_alphabetic() || c == b'_' {
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            out.push((start, Tok::Ident(text[start..i].to_owned())));
        } else if b"+-*/^(),".contains(&c) {
            out.push((start, Tok::Sym(c.into())));
            i += 1;
        } else {
            let ch = text[i..].chars().next().expect("i is on a char boundary");
            return Err(ExprError::Syntax {
                at: start,
                message: format!("unexpected {ch:?}"),
            });
        }
    }
    out.push((text.len(), Tok::End));
    Ok(out)
}

/// The end of the number starting at `i`: digits, a fraction, and an
/// exponent only when digits follow the `e`, so `2em` stays `2` `em`.
fn number_end(bytes: &[u8], mut i: usize) -> usize {
    let digits = |mut i: usize| {
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        i
    };
    i = digits(i);
    if bytes.get(i) == Some(&b'.') {
        i = digits(i + 1);
    }
    if matches!(bytes.get(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        if matches!(bytes.get(j), Some(b'+' | b'-')) {
            j += 1;
        }
        if bytes.get(j).is_some_and(u8::is_ascii_digit) {
            i = digits(j);
        }
    }
    i
}

struct Parser {
    toks: Vec<(usize, Tok)>,
    pos: usize,
    depth: usize,
}

pub(crate) fn parse(text: &str) -> Result<Ast, ExprError> {
    let mut p = Parser {
        toks: lex(text)?,
        pos: 0,
        depth: 0,
    };
    let ast = p.sum()?;
    match p.peek() {
        Tok::End => Ok(ast),
        _ => Err(p.unexpected()),
    }
}

impl Parser {
    fn peek(&self) -> &Tok {
        &self.toks[self.pos].1
    }

    fn at(&self) -> usize {
        self.toks[self.pos].0
    }

    fn bump(&mut self) -> Tok {
        let t = self.toks[self.pos].1.clone();
        if t != Tok::End {
            self.pos += 1;
        }
        t
    }

    fn eat(&mut self, c: char) -> bool {
        if *self.peek() == Tok::Sym(c) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn unexpected(&self) -> ExprError {
        let message = match self.peek() {
            Tok::End => "the expression ends too soon".to_owned(),
            Tok::Num(n) => format!("unexpected number {n}"),
            Tok::Ident(s) => format!("unexpected `{s}`"),
            Tok::Sym(c) => format!("unexpected `{c}`"),
        };
        ExprError::Syntax {
            at: self.at(),
            message,
        }
    }

    fn deeper(&mut self) -> Result<(), ExprError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(ExprError::Syntax {
                at: self.at(),
                message: format!("nested more than {MAX_DEPTH} deep"),
            });
        }
        Ok(())
    }

    fn sum(&mut self) -> Result<Ast, ExprError> {
        let mut lhs = self.product()?;
        loop {
            let op = if self.eat('+') {
                BinOp::Add
            } else if self.eat('-') {
                BinOp::Sub
            } else {
                return Ok(lhs);
            };
            lhs = Ast::Bin(op, Box::new(lhs), Box::new(self.product()?));
        }
    }

    fn product(&mut self) -> Result<Ast, ExprError> {
        let mut lhs = self.unary()?;
        loop {
            let op = if self.eat('*') {
                BinOp::Mul
            } else if self.eat('/') {
                BinOp::Div
            } else {
                return Ok(lhs);
            };
            lhs = Ast::Bin(op, Box::new(lhs), Box::new(self.unary()?));
        }
    }

    fn unary(&mut self) -> Result<Ast, ExprError> {
        self.deeper()?;
        let ast = if self.eat('-') {
            Ast::Neg(Box::new(self.unary()?))
        } else {
            self.power()?
        };
        self.depth -= 1;
        Ok(ast)
    }

    fn power(&mut self) -> Result<Ast, ExprError> {
        let base = self.atom()?;
        if self.eat('^') {
            let exp = self.unary()?;
            return Ok(Ast::Bin(BinOp::Pow, Box::new(base), Box::new(exp)));
        }
        Ok(base)
    }

    fn atom(&mut self) -> Result<Ast, ExprError> {
        match self.peek().clone() {
            Tok::Num(n) => {
                self.bump();
                let unit = match self.peek().clone() {
                    Tok::Ident(symbol) => {
                        self.bump();
                        Some(Unit::parse(&symbol).ok_or(ExprError::UnknownUnit(symbol))?)
                    }
                    _ => None,
                };
                Ok(Ast::Num(n, unit))
            }
            Tok::Ident(name) => {
                self.bump();
                if self.eat('(') {
                    self.call(name)
                } else {
                    Ok(Ast::Name(name))
                }
            }
            Tok::Sym('(') => {
                self.bump();
                let inner = self.sum()?;
                if !self.eat(')') {
                    return Err(self.unexpected());
                }
                Ok(inner)
            }
            _ => Err(self.unexpected()),
        }
    }

    fn call(&mut self, name: String) -> Result<Ast, ExprError> {
        let mut args = vec![self.sum()?];
        while self.eat(',') {
            args.push(self.sum()?);
        }
        if !self.eat(')') {
            return Err(self.unexpected());
        }
        Ok(Ast::Call(name, args))
    }
}
