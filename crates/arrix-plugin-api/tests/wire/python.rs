//! The Python SDK's wire, from the model: `python/arrix/_wire.py`, a
//! dataclass per record and variant with its JSON form, the kernel's stubs
//! (`KernelStubs`) and the plugin side's dispatch (`FeatureStubs`). Pure
//! standard library (ADR-0021 §4). Ids stay the 13-character text of the
//! wire; `arrix.ids` makes them from numbers.

use std::fmt::Write;

use super::model::{Def, Direction, Func, Ty, World};

const KEYWORDS: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue",
    "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
    "with", "yield",
];

fn snake(name: &str) -> String {
    let s = name.replace('-', "_");
    if KEYWORDS.contains(&s.as_str()) {
        format!("{s}_")
    } else {
        s
    }
}

fn pascal(name: &str) -> String {
    name.split('-')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        })
        .collect()
}

fn doc(out: &mut String, indent: &str, text: &str) {
    if !text.is_empty() {
        let text = text.replace('\\', "\\\\").replace("\"\"\"", "\\\"\\\"\\\"");
        let _ = writeln!(out, "{indent}\"\"\"{text}\"\"\"");
    }
}

/// The annotation of a type.
fn annotation(t: &Ty) -> String {
    match t {
        Ty::Bool => "bool".into(),
        Ty::U8 | Ty::U32 | Ty::S64 => "int".into(),
        Ty::Handle(_) => "Body".into(),
        Ty::F32 | Ty::F64 => "float".into(),
        Ty::List(_) if t.run() == Some("u8") => "bytes".into(),
        Ty::List(_) if t.run().is_some() => format!("array[{}]", annotation(inner(t))),
        Ty::String | Ty::Id => "str".into(),
        Ty::Named(n) => named_annotation(n),
        Ty::List(t) => format!("list[{}]", annotation(t)),
        Ty::Option(t) => format!("{} | None", annotation(t)),
        Ty::Tuple(ts) => format!(
            "tuple[{}]",
            ts.iter().map(annotation).collect::<Vec<_>>().join(", ")
        ),
        Ty::Result(ok, _) => ok.as_deref().map(annotation).unwrap_or("None".into()),
    }
}

/// The element of a list type.
fn inner(t: &Ty) -> &Ty {
    match t {
        Ty::List(t) => t,
        other => panic!("{other:?} is no list"),
    }
}

/// Enums are their case's text; every other named type is its class.
fn named_annotation(n: &str) -> String {
    pascal(n)
}

struct Names<'a>(&'a World);

impl Names<'_> {
    fn is_enum(&self, n: &str) -> bool {
        matches!(self.0.find(n).def, Def::Enum(_))
    }

    /// The expression that encodes `v` of type `t`; `d` makes the loop
    /// variables unique.
    fn encode(&self, t: &Ty, v: &str, d: usize) -> String {
        match t {
            Ty::Bool | Ty::U32 | Ty::S64 | Ty::F64 | Ty::String | Ty::Id | Ty::Handle(_) => {
                v.into()
            }
            Ty::U8 | Ty::F32 => panic!("a bare {t:?} is only a run's element"),
            Ty::Named(n) if self.is_enum(n) => v.into(),
            Ty::Named(_) => format!("{v}.to_json()"),
            Ty::List(_) if t.run().is_some() => {
                format!("run_encode(\"{}\", {v})", t.run().unwrap())
            }
            Ty::List(t) => format!(
                "[{} for x{d} in {v}]",
                self.encode(t, &format!("x{d}"), d + 1)
            ),
            Ty::Option(t) => format!("None if {v} is None else {}", self.encode(t, v, d)),
            Ty::Tuple(ts) => format!(
                "[{}]",
                ts.iter()
                    .enumerate()
                    .map(|(i, t)| self.encode(t, &format!("{v}[{i}]"), d))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Ty::Result(..) => {
                panic!("a result is only a function's, and is not encoded as a value")
            }
        }
    }

    fn decode(&self, t: &Ty, v: &str, d: usize) -> String {
        match t {
            Ty::Bool | Ty::U32 | Ty::S64 | Ty::F64 | Ty::String | Ty::Id | Ty::Handle(_) => {
                v.into()
            }
            Ty::U8 | Ty::F32 => panic!("a bare {t:?} is only a run's element"),
            Ty::Named(n) if self.is_enum(n) => v.into(),
            Ty::Named(n) => format!("{}.from_json({v})", pascal(n)),
            Ty::List(_) if t.run().is_some() => {
                format!("run_decode(\"{}\", {v})", t.run().unwrap())
            }
            Ty::List(t) => format!(
                "[{} for x{d} in {v}]",
                self.decode(t, &format!("x{d}"), d + 1)
            ),
            Ty::Option(t) => format!("None if {v} is None else {}", self.decode(t, v, d)),
            Ty::Tuple(ts) => format!(
                "({})",
                ts.iter()
                    .enumerate()
                    .map(|(i, t)| self.decode(t, &format!("{v}[{i}]"), d))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Ty::Result(..) => panic!("a result is only a function's"),
        }
    }
}

pub fn generate(world: &World) -> String {
    let names = Names(world);
    let mut out = String::new();
    out.push_str(
        "# @generated by crates/arrix-plugin-api/tests/wire.rs from wit/arrix-plugin.wit.\n\
         # Do not edit; regenerate with\n\
         # `UPDATE_SNAPSHOTS=1 cargo test -p arrix-plugin-api --test wire`.\n\
         \"\"\"The plugin API's types and stubs on the wire (ADR-0022), pure standard library.\"\"\"\n\n\
         from __future__ import annotations\n\n\
         import base64\n\
         import sys\n\
         from array import array\n\
         from dataclasses import dataclass\n\
         from typing import Any\n\n",
    );
    let _ = writeln!(out, "API_VERSION = \"{}\"\n", world.version);
    out.push_str(HEADER);
    for t in &world.types {
        match &t.def {
            Def::Id | Def::Resource => {}
            Def::Enum(cases) => enumeration(&mut out, &t.name, &t.docs, cases),
            Def::Record(fields) => record(&mut out, &names, &t.name, &t.docs, fields),
            Def::Variant(cases) => variant(&mut out, &names, &t.name, &t.docs, cases),
        }
    }
    for (interface, class, error) in [
        ("kernel", "KernelStubs", "KernelError"),
        ("document", "DocumentStubs", "DocumentError"),
    ] {
        let funcs: Vec<_> = world
            .funcs
            .iter()
            .filter(|f| f.interface == interface)
            .collect();
        assert!(funcs.iter().all(|f| f.direction == Direction::PluginToHost));
        service_stubs(&mut out, &names, interface, class, error, &funcs);
    }
    for (interface, class) in [("feature", "FeatureStubs"), ("exchange", "ExchangeStubs")] {
        let funcs: Vec<_> = world
            .funcs
            .iter()
            .filter(|f| f.interface == interface)
            .collect();
        assert!(funcs.iter().all(|f| f.direction == Direction::HostToPlugin));
        plugin_stubs(&mut out, &names, interface, class, &funcs);
    }
    let _ = writeln!(out, "METHODS = {{");
    for f in &world.funcs {
        let _ = writeln!(out, "    \"{}\": \"{}\",", f.method(), f.direction.text());
    }
    out.push_str("}\n");
    out
}

const HEADER: &str = r#"
Body = int
"""A body of the host's kernel: a number, valid within one evaluation."""


class Failure(Exception):
    """A diagnostic as an exception: raised in an `evaluate`, it is the
    feature's `err`, and the plugin's code may raise one of its own."""

    def __init__(self, diagnostic: Diagnostic):
        super().__init__(f"{diagnostic.code}: {diagnostic.message}")
        self.diagnostic = diagnostic


class KernelError(Failure):
    """The host's kernel refused a call; the diagnostic is its."""


class DocumentError(Failure):
    """The host refused a snapshot read (a mesh it cannot make, a part with
    no mass); the diagnostic is its."""


assert array("f").itemsize == 4 and array("I").itemsize == 4
_RUN_CODES = {"f32": "f", "u32": "I"}


def run_encode(kind: str, values: Any) -> str:
    """A run of `u8`, `f32` or `u32` as base64 of its little-endian bytes
    (ADR-0022 section 5). Any sequence of numbers will do."""
    if kind == "u8":
        data = bytes(values)
    else:
        a = array(_RUN_CODES[kind], values)
        if sys.byteorder == "big":
            a.byteswap()
        data = a.tobytes()
    return base64.b64encode(data).decode("ascii")


def run_decode(kind: str, text: str) -> Any:
    """`bytes` for a `u8` run, an `array.array` for the others."""
    data = base64.b64decode(text, validate=True)
    if kind == "u8":
        return data
    a = array(_RUN_CODES[kind])
    a.frombytes(data)
    if sys.byteorder == "big":
        a.byteswap()
    return a


"#;

fn enumeration(out: &mut String, name: &str, docs: &str, cases: &[String]) {
    let _ = writeln!(out, "class {}:", pascal(name));
    doc(out, "    ", docs);
    for c in cases {
        let _ = writeln!(out, "    {} = \"{c}\"", snake(c).to_uppercase());
    }
    let list = cases
        .iter()
        .map(|c| format!("\"{c}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(out, "    ALL = ({list})\n\n");
}

fn record(out: &mut String, names: &Names, name: &str, docs: &str, fields: &[super::model::Field]) {
    let class = pascal(name);
    let _ = writeln!(out, "@dataclass(frozen=True)\nclass {class}:");
    doc(out, "    ", docs);
    for f in fields {
        let _ = writeln!(out, "    {}: {}", snake(&f.name), annotation(&f.ty));
    }
    let _ = writeln!(
        out,
        "\n    def to_json(self) -> dict[str, Any]:\n        return {{"
    );
    for f in fields {
        let v = format!("self.{}", snake(&f.name));
        let _ = writeln!(
            out,
            "            \"{}\": {},",
            f.name,
            names.encode(&f.ty, &v, 0)
        );
    }
    let _ = writeln!(
        out,
        "        }}\n\n    @staticmethod\n    def from_json(v: Any) -> {class}:\n        return {class}("
    );
    for f in fields {
        let v = format!("v[\"{}\"]", f.name);
        let _ = writeln!(
            out,
            "            {}={},",
            snake(&f.name),
            names.decode(&f.ty, &v, 0)
        );
    }
    let _ = writeln!(out, "        )\n\n");
}

fn variant(out: &mut String, names: &Names, name: &str, docs: &str, cases: &[super::model::Case]) {
    let class = pascal(name);
    let _ = writeln!(
        out,
        "@dataclass(frozen=True)\nclass {class}:\n    \"\"\"One of {}: `case` names it, `value` is its payload.\"\"\"",
        cases
            .iter()
            .map(|c| format!("`{}`", c.name))
            .collect::<Vec<_>>()
            .join(", ")
    );
    if !docs.is_empty() {
        let _ = writeln!(out, "    # {docs}");
    }
    let _ = writeln!(out, "    case: str\n    value: Any = None\n");
    for c in cases {
        let f = snake(&c.name);
        match &c.ty {
            Some(t) => {
                let _ = writeln!(
                    out,
                    "    @staticmethod\n    def {f}(value: {}) -> {class}:\n        return {class}(\"{}\", value)\n",
                    annotation(t),
                    c.name
                );
            }
            None => {
                let _ = writeln!(
                    out,
                    "    @staticmethod\n    def {f}() -> {class}:\n        return {class}(\"{}\")\n",
                    c.name
                );
            }
        }
    }
    let _ = writeln!(out, "    def to_json(self) -> dict[str, Any]:");
    for c in cases {
        let payload = match &c.ty {
            Some(t) => names.encode(t, "self.value", 0),
            None => "None".into(),
        };
        let _ = writeln!(
            out,
            "        if self.case == \"{}\":\n            return {{\"{}\": {payload}}}",
            c.name, c.name
        );
    }
    let _ = writeln!(
        out,
        "        raise ValueError(f\"{name} has no case {{self.case!r}}\")\n"
    );
    let _ = writeln!(
        out,
        "    @staticmethod\n    def from_json(v: Any) -> {class}:\n        (case, payload), = v.items()"
    );
    for c in cases {
        let payload = match &c.ty {
            Some(t) => names.decode(t, "payload", 0),
            None => "None".into(),
        };
        let _ = writeln!(
            out,
            "        if case == \"{}\":\n            return {class}(case, {payload})",
            c.name
        );
    }
    let _ = writeln!(
        out,
        "        raise ValueError(f\"{name} has no case {{case!r}}\")\n\n"
    );
}

fn result_parts(f: &Func) -> (&Ty, bool) {
    match f
        .result
        .as_ref()
        .expect("every function of the world returns")
    {
        Ty::Result(Some(ok), _) => (ok, true),
        other => (other, false),
    }
}

fn service_stubs(
    out: &mut String,
    names: &Names,
    interface: &str,
    class: &str,
    error: &str,
    funcs: &[&Func],
) {
    let _ = write!(
        out,
        "class {class}:\n    \"\"\"The host's `{interface}` interface. A subclass supplies `_request`; \
         a call the host refuses raises `{error}`.\"\"\"\n\n\
         \x20   def _request(self, method: str, params: dict[str, Any]) -> Any:\n\
         \x20       raise NotImplementedError\n\n",
    );
    for f in funcs {
        let args: String = f
            .params
            .iter()
            .map(|(n, t)| format!(", {}: {}", snake(n), annotation(t)))
            .collect();
        let (ok, is_result) = result_parts(f);
        let _ = writeln!(
            out,
            "    def {}(self{args}) -> {}:",
            snake(&f.name),
            annotation(ok)
        );
        doc(out, "        ", &f.docs);
        let params = f
            .params
            .iter()
            .map(|(n, t)| format!("\"{n}\": {}", names.encode(t, &snake(n), 0)))
            .collect::<Vec<_>>()
            .join(", ");
        if is_result {
            let _ = writeln!(
                out,
                "        r = self._request(\"{}\", {{{params}}})\n        if \"err\" in r:\n            raise {error}(Diagnostic.from_json(r[\"err\"]))\n        return {}\n",
                f.method(),
                names.decode(ok, "r[\"ok\"]", 0)
            );
        } else {
            let _ = writeln!(
                out,
                "        r = self._request(\"{}\", {{{params}}})\n        return {}\n",
                f.method(),
                names.decode(ok, "r", 0)
            );
        }
    }
    out.push('\n');
}

fn plugin_stubs(out: &mut String, names: &Names, interface: &str, class: &str, funcs: &[&Func]) {
    let _ = write!(
        out,
        "class {class}:\n    \"\"\"The plugin's `{interface}` interface: a subclass implements the \
         methods, and `dispatch` serves a host request with them.\"\"\"\n\n",
    );
    for f in funcs {
        let args: String = f
            .params
            .iter()
            .map(|(n, t)| format!(", {}: {}", snake(n), annotation(t)))
            .collect();
        let (ok, _) = result_parts(f);
        let _ = writeln!(
            out,
            "    def {}(self{args}) -> {}:",
            snake(&f.name),
            annotation(ok)
        );
        doc(out, "        ", &f.docs);
        out.push_str("        raise NotImplementedError\n\n");
    }
    out.push_str("    def dispatch(self, method: str, params: dict[str, Any]) -> Any:\n");
    for f in funcs {
        let (ok, is_result) = result_parts(f);
        let _ = writeln!(out, "        if method == \"{}\":", f.method());
        let args = f
            .params
            .iter()
            .map(|(n, t)| names.decode(t, &format!("params[\"{n}\"]"), 0))
            .collect::<Vec<_>>()
            .join(", ");
        let call = format!("self.{}({args})", snake(&f.name));
        if is_result {
            let _ = writeln!(
                out,
                "            try:\n                r = {call}\n            except Failure as e:\n                return {{\"err\": e.diagnostic.to_json()}}\n            return {{\"ok\": {}}}",
                names.encode(ok, "r", 0)
            );
        } else {
            let _ = writeln!(
                out,
                "            r = {call}\n            return {}",
                names.encode(ok, "r", 0)
            );
        }
    }
    out.push_str("        raise KeyError(method)\n\n\n");
}
