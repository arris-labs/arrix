use arrix_core::QuantityKind::{self, Angle, Count, Length, Mass, Ratio};
use arrix_core::units::UNITS;
use proptest::prelude::*;

use super::*;

/// The acceptance scenario's parameters: `teeth = 20`, `m = 1 mm`,
/// `w = 8 mm`, and an angle.
fn kinds(name: &str) -> Option<QuantityKind> {
    match name {
        "teeth" => Some(Count),
        "m" | "w" => Some(Length),
        "pa" => Some(Angle),
        _ => None,
    }
}

fn values(name: &str) -> Option<f64> {
    match name {
        "teeth" => Some(20.0),
        "m" => Some(0.001),
        "w" => Some(0.008),
        "pa" => Some(20f64.to_radians()),
        _ => None,
    }
}

fn q(text: &str, kind: QuantityKind) -> Result<f64, ExprError> {
    Expr::parse(text)?
        .quantity(kind, &kinds, &values)
        .map(|q| q.si)
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-12 * b.abs().max(1.0)
}

#[test]
fn evaluates_to_si() {
    assert_eq!(q("12 mm", Length), Ok(0.012));
    assert_eq!(q("12mm", Length), Ok(0.012));
    assert_eq!(q("1 in + 1.6 mm", Length), Ok(0.027));
    assert_eq!(q("teeth", Count), Ok(20.0));
    assert_eq!(q("teeth * m", Length), Ok(0.02));
    assert_eq!(q("2 m", Length), Ok(2.0), "a unit after a number");
    assert_eq!(q("2 * m", Length), Ok(0.002), "the parameter otherwise");
    assert_eq!(q("w / 2 + 1 mm", Length), Ok(0.005));
    assert_eq!(q("-(w - 10 mm)", Length), Ok(0.002));
    assert_eq!(q("2^3^2", Count), Ok(512.0), "right-associative");
    assert_eq!(q("-2^2", Ratio), Ok(-4.0), "a minus binds looser than ^");
    assert_eq!(q("1.5e-3 m", Length), Ok(0.0015));
    assert!(close(q("180 deg", Angle).unwrap(), std::f64::consts::PI));
    assert!(close(q("sin(30 deg)", Ratio).unwrap(), 0.5));
    assert!(close(
        q("atan2(w, w)", Angle).unwrap(),
        std::f64::consts::FRAC_PI_4
    ));
    assert_eq!(q("sqrt(w * w)", Length), Ok(0.008));
    assert_eq!(q("min(w, m, 3 mm)", Length), Ok(0.001));
    assert_eq!(q("max(w, m)", Length), Ok(0.008));
    assert_eq!(q("abs(-w)", Length), Ok(0.008));
    assert_eq!(q("round(teeth / 3)", Count), Ok(7.0));
    assert_eq!(q("floor(2.5) + ceil(2.5)", Count), Ok(5.0));
    assert_eq!(q("w^2 / m", Length), Ok(0.064));
    assert_eq!(q("250 g + 1 kg", Mass), Ok(1.25));
    assert_eq!(q("w / m", Ratio), Ok(8.0), "count and ratio are both plain");
}

#[test]
fn refuses_a_unit_mismatch_at_check_time() {
    let e = Expr::parse("30 deg").unwrap();
    let err = e.quantity(Length, &kinds, &|_| None).unwrap_err();
    assert_eq!(
        err,
        ExprError::UnitMismatch {
            have: Dim::of(Angle),
            want: Length
        }
    );
    assert_eq!(err.code(), "expr.unit-mismatch");
    assert_eq!(err.diagnostic().code.as_str(), "expr.unit-mismatch");
    assert_eq!(
        err.to_string(),
        "the expression is an angle, where a length is wanted"
    );
    let err = |t: &str| Expr::parse(t).unwrap().check(&kinds).unwrap_err();
    assert_eq!(err("w + pa").code(), "expr.dimension");
    assert_eq!(
        err("w + pa").to_string(),
        "cannot add a length and an angle"
    );
    assert_eq!(err("w ^ w").code(), "expr.dimension");
    assert_eq!(
        err("w ^ teeth").code(),
        "expr.dimension",
        "a whole constant only"
    );
    assert_eq!(err("w ^ 0.5").code(), "expr.dimension");
    assert_eq!(err("sqrt(w)").code(), "expr.dimension");
    assert_eq!(err("sin(w)").code(), "expr.dimension");
    assert_eq!(err("round(w)").code(), "expr.dimension");
    assert_eq!(err("min(w, pa)").code(), "expr.dimension");
    assert_eq!(err("x + 1").code(), "expr.unknown-name");
    assert_eq!(err("frob(1)").code(), "expr.unknown-function");
    assert_eq!(err("sin(1, 2)").code(), "expr.arity");
    assert_eq!(err("atan2(1)").code(), "expr.arity");
    assert_eq!(
        Expr::parse("w * w").unwrap().check(&kinds),
        Ok(Dim {
            length: 2,
            angle: 0,
            mass: 0
        })
    );
    assert_eq!(
        Expr::parse("m / w * pa")
            .unwrap()
            .check(&kinds)
            .unwrap()
            .to_string(),
        "angle"
    );
}

#[test]
fn refuses_what_does_not_evaluate() {
    assert_eq!(q("1 mm / 0", Length), Err(ExprError::NotFinite));
    assert_eq!(q("sqrt(-1)", Ratio), Err(ExprError::NotFinite));
    assert_eq!(q("teeth / 3", Count), Err(ExprError::NotWhole(20.0 / 3.0)));
    assert_eq!(q("teeth / 3", Ratio).map(|v| v > 6.6), Ok(true));
}

#[test]
fn reports_syntax_errors_where_they_are() {
    let err = |t: &str| Expr::parse(t).unwrap_err();
    for (text, at) in [
        ("", 0),
        ("1 +", 3),
        ("(1", 2),
        ("1)", 1),
        ("1 2", 2),
        ("sin(1,)", 6),
        ("w $ 2", 2),
        ("1e999", 0),
        ("..", 0),
    ] {
        match err(text) {
            ExprError::Syntax { at: got, .. } => assert_eq!(got, at, "{text:?}"),
            other => panic!("{text:?}: {other:?}"),
        }
    }
    assert_eq!(err("2 ft"), ExprError::UnknownUnit("ft".into()));
    assert_eq!(err("2 MM"), ExprError::UnknownUnit("MM".into()));
    let deep = format!("{}1{}", "(".repeat(100), ")".repeat(100));
    assert!(matches!(err(&deep), ExprError::Syntax { .. }));
    assert!(matches!(err(&"-".repeat(10_000)), ExprError::Syntax { .. }));
}

#[test]
fn keeps_the_text_as_typed_and_names_what_it_reads() {
    let e = Expr::parse("  teeth*m +w ").unwrap();
    assert_eq!(e.text(), "  teeth*m +w ");
    assert_eq!(e.to_string(), "  teeth*m +w ");
    assert_eq!(e.ast.to_string(), "teeth * m + w");
    assert_eq!(
        e.names().into_iter().collect::<Vec<_>>(),
        ["m", "teeth", "w"]
    );
    let json = serde_json::to_string(&e).unwrap();
    assert_eq!(json, r#""  teeth*m +w ""#);
    assert_eq!(serde_json::from_str::<Expr>(&json).unwrap(), e);
    assert!(serde_json::from_str::<Expr>(r#""1 +""#).is_err());
    assert_eq!(FUNCTIONS.len(), 14);
}

fn arb_ast() -> impl Strategy<Value = Ast> {
    let num = (
        prop_oneof![Just(0.0), 0.0..1e6f64, (0u32..1000).prop_map(f64::from)],
        proptest::option::of(0..UNITS.len()),
    )
        .prop_map(|(n, u)| Ast::Num(n, u.map(|i| UNITS[i])));
    let name = prop_oneof![
        Just("w"),
        Just("m"),
        Just("teeth"),
        Just("x_1"),
        Just("sin")
    ]
    .prop_map(|n| Ast::Name(n.into()));
    prop_oneof![num, name].prop_recursive(6, 48, 3, |inner| {
        let op = prop_oneof![
            Just(BinOp::Add),
            Just(BinOp::Sub),
            Just(BinOp::Mul),
            Just(BinOp::Div),
            Just(BinOp::Pow),
        ];
        prop_oneof![
            inner.clone().prop_map(|a| Ast::Neg(Box::new(a))),
            (op, inner.clone(), inner.clone()).prop_map(|(op, a, b)| Ast::Bin(
                op,
                Box::new(a),
                Box::new(b)
            )),
            (
                proptest::sample::select(FUNCTIONS),
                proptest::collection::vec(inner, 1..3)
            )
                .prop_map(|(f, args)| Ast::Call(f.into(), args)),
        ]
    })
}

proptest! {
    #[test]
    fn print_then_parse_is_the_identity(ast in arb_ast()) {
        let text = ast.to_string();
        let back = Expr::parse(&text).map_err(|e| TestCaseError::fail(format!("{text:?}: {e}")))?;
        prop_assert_eq!(&back.ast, &ast, "{}", text);
        prop_assert_eq!(back.ast.to_string(), text);
    }

    #[test]
    fn nothing_typed_panics(text in "[ -~]{0,40}") {
        if let Ok(e) = Expr::parse(&text) {
            let _ = e.check(&kinds);
            let _ = e.evaluate(&values);
            prop_assert_eq!(Expr::parse(&e.ast.to_string()).map(|b| b.ast).ok(), Some(e.ast));
        }
    }
}
