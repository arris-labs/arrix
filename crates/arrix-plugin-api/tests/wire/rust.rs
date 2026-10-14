//! The Rust codec, from the model: `arrix-plugin-host`'s
//! `wire/generated.rs`, a pair of functions per type between the plugin
//! API's Rust types and `serde_json::Value`, in the shapes of schema.json.
//!
//! A type is in one of four classes, and a type of the world in none of
//! them fails the generator (a WIT addition the codec would otherwise
//! skip):
//!
//! - **auto**: the Rust type mirrors the WIT type field for field, case for
//!   case; the codec is written here;
//! - **custom**: the Rust type is validated on construction or shaped
//!   otherwise (a frame, a name, a reference, a profile); the codec is
//!   `wire/custom.rs`'s, by hand, and checked by the host's tests against
//!   the schema;
//! - **inline**: a type that exists only as a custom type's payload;
//! - the primitives `id` and the resource `body`, which the helpers carry.

use std::collections::BTreeSet;
use std::fmt::Write;

use super::model::{Case, Def, Direction, Field, Ty, World};

/// WIT type -> the plugin API's Rust type.
const AUTO: &[(&str, &str)] = &[
    ("vec2", "DVec2"),
    ("vec3", "DVec3"),
    ("quantity-kind", "QuantityKind"),
    ("quantity", "Quantity"),
    ("severity", "Severity"),
    ("diagnostic", "Diagnostic"),
    ("mass-properties", "MassProperties"),
    ("axis", "Axis"),
    ("point", "Point"),
    ("motion", "Motion"),
    ("shown-when", "ShownWhen"),
    ("param-spec", "ParamSpec"),
    ("choice-option", "ChoiceOption"),
    ("choice-spec", "ChoiceSpec"),
    ("input-kind", "InputKind"),
    ("input-spec", "InputSpec"),
    ("slot-kind", "SlotKind"),
    ("slot-spec", "SlotSpec"),
    ("feature-type-spec", "FeatureTypeSpec"),
    ("param-value", "ParamValue"),
    ("choice-value", "ChoiceValue"),
    ("tool-op", "ToolOp"),
    ("tool", "Tool"),
    ("input-value", "InputValue"),
    ("resolved-input", "ResolvedInput"),
    ("output-value", "OutputValue"),
    ("slot-output", "SlotOutput"),
    ("feature-output", "FeatureOutput"),
    ("rgb", "Rgb"),
    ("material-info", "MaterialInfo"),
    ("body-ref", "BodyRef"),
    ("body-info", "BodyInfo"),
    ("part-info", "PartInfo"),
    ("triangles", "Triangles"),
    ("inertia-tensor", "InertiaTensor"),
    ("inertial", "Inertial"),
    ("joint-kind", "JointKind"),
    ("component-of", "ComponentOf"),
    ("component-info", "ComponentInfo"),
    ("joint-placement", "JointPlacement"),
    ("joint-state", "JointState"),
    ("joint-info", "JointInfo"),
    ("assembly-info", "AssemblyInfo"),
    ("subject-kind", "SubjectKind"),
    ("subject", "Subject"),
    ("exporter-spec", "ExporterSpec"),
    ("file", "ExportFile"),
    ("feature-info", "FeatureInfo"),
    ("command-spec", "CommandSpec"),
    ("context", "Context"),
    ("circle-edge", "CircleEdge"),
    ("line-edge", "LineEdge"),
    ("edge-curve", "EdgeCurve"),
    ("resolved-edge", "ResolvedEdge"),
    ("command-input-value", "CommandInputValue"),
    ("command-input", "CommandInput"),
    ("expression", "Expression"),
    ("named-expression", "NamedExpression"),
    ("named-input", "NamedInput"),
    ("feature-draft", "FeatureDraft"),
    ("part-draft", "PartDraft"),
    ("add-feature", "AddFeature"),
    ("pose", "Pose"),
    ("component-draft", "ComponentDraft"),
    ("component-change", "ComponentChange"),
    ("anchor", "Anchor"),
    ("joint-draft", "JointDraft"),
    ("joint-change", "JointChange"),
    ("material-draft", "MaterialDraft"),
    ("assign-material", "AssignMaterial"),
    ("plugin-command", "PluginCommand"),
    ("command-group", "CommandGroup"),
];

/// Codec by hand, in `wire/custom.rs`.
const CUSTOM: &[(&str, &str)] = &[
    ("frame", "Frame"),
    ("persistent-ref", "PersistentName"),
    ("reference", "Ref"),
    ("profile", "Profile"),
    ("profile-segment", "ProfileSegment"),
    ("profile-loop", "ProfileLoop"),
];

/// Payloads of the custom types, with no Rust type of their own.
const INLINE: &[&str] = &[
    "slot-ref",
    "sketch-ref",
    "region-ref",
    "plugin-ref",
    "line-segment",
    "arc-segment",
    "circle-loop",
    "path-loop",
];

/// A string field whose Rust type checks its text: (type, field, Rust).
const NEWTYPE: &[(&str, &str, &str)] = &[
    ("diagnostic", "code", "DiagnosticCode"),
    ("slot-spec", "name", "SlotName"),
    ("slot-output", "name", "SlotName"),
    ("body-ref", "slot", "SlotName"),
];

/// A resource -> the Rust handle type.
const HANDLES: &[(&str, &str)] = &[("body", "Body")];

/// The interface whose types get a file of their own, so no generated file
/// passes the size lint.
const SPLIT: &str = "commands";

/// The codec of the types outside [`SPLIT`], and the method table.
pub fn generate(world: &World) -> String {
    check_tables(world);
    file(world, false)
}

/// The codec of the types of [`SPLIT`].
pub fn generate_commands(world: &World) -> String {
    file(world, true)
}

fn file(world: &World, commands: bool) -> String {
    let mut body = String::new();
    let mut custom_used = BTreeSet::new();
    let mut rust_used: BTreeSet<&str> = BTreeSet::new();
    for t in world
        .types
        .iter()
        .filter(|t| (t.interface == SPLIT) == commands)
    {
        let Some(rust) = AUTO.iter().find(|(w, _)| *w == t.name).map(|(_, r)| *r) else {
            continue;
        };
        rust_used.insert(rust);
        let mut cx = Cx {
            custom_used: &mut custom_used,
            rust_used: &mut rust_used,
            type_name: &t.name,
        };
        match &t.def {
            Def::Record(fields) => record(&mut body, &mut cx, &t.name, rust, fields),
            Def::Enum(cases) => enumeration(&mut body, &t.name, rust, cases),
            Def::Variant(cases) => variant(&mut body, &mut cx, &t.name, rust, cases),
            Def::Id | Def::Resource => unreachable!("{} is a primitive", t.name),
        }
    }

    let mut out = String::new();
    out.push_str(
        "// @generated by crates/arrix-plugin-api/tests/wire.rs from \
         wit/arrix-plugin.wit.\n// Do not edit; regenerate with \
         `UPDATE_SNAPSHOTS=1 cargo test -p arrix-plugin-api --test wire`.\n\n",
    );
    if commands {
        out.push_str("//! The codec of the `commands` interface's types (ADR-0022, ADR-0038).\n\n");
    } else {
        out.push_str(
            "//! The codec of every plugin-API type that mirrors its WIT type (ADR-0022).\n\n",
        );
    }
    if !rust_used.is_empty() {
        let _ = writeln!(
            out,
            "use arrix_plugin_api::{{{}}};",
            rust_used.iter().copied().collect::<Vec<_>>().join(", ")
        );
    }
    out.push_str("use serde_json::{Map, Value};\n\n");
    out.push_str("use super::prim::*;\n");
    if commands {
        out.push_str("use super::generated::*;\n");
        out.push_str("use super::WireError;\n");
    } else {
        out.push_str("use super::{Direction, Method, WireError};\n");
    }
    if !custom_used.is_empty() {
        let names: Vec<String> = custom_used
            .iter()
            .flat_map(|n: &String| [format!("{n}_from_json"), format!("{n}_to_json")])
            .collect();
        let _ = writeln!(out, "use super::custom::{{{}}};\n", names.join(", "));
    }
    out.push_str(&body);
    if !commands {
        methods(&mut out, world);
    }
    rustfmt(&out)
}

fn check_tables(world: &World) {
    let named: BTreeSet<&str> = world.types.iter().map(|t| t.name.as_str()).collect();
    let covered: BTreeSet<&str> = AUTO
        .iter()
        .map(|(w, _)| *w)
        .chain(CUSTOM.iter().map(|(w, _)| *w))
        .chain(INLINE.iter().copied())
        .chain(HANDLES.iter().map(|(w, _)| *w))
        .chain(["id"])
        .collect();
    let uncovered: Vec<_> = named.difference(&covered).collect();
    let gone: Vec<_> = covered.difference(&named).collect();
    assert!(
        uncovered.is_empty() && gone.is_empty(),
        "the Rust codec's tables (tests/wire/rust.rs) and the world disagree: in the world \
         but in no table: {uncovered:?}; in a table but not in the world: {gone:?}"
    );
    for (ty, field, _) in NEWTYPE {
        let Def::Record(fields) = &world.find(ty).def else {
            panic!("{ty} is no record");
        };
        assert!(
            fields
                .iter()
                .any(|f| f.name == *field && f.ty == Ty::String),
            "{ty}.{field} is no string field"
        );
    }
}

struct Cx<'a> {
    custom_used: &'a mut BTreeSet<String>,
    rust_used: &'a mut BTreeSet<&'static str>,
    type_name: &'a str,
}

fn snake(kebab: &str) -> String {
    kebab.replace('-', "_")
}

fn pascal(kebab: &str) -> String {
    kebab
        .split('-')
        .map(|w| {
            let mut cs = w.chars();
            cs.next()
                .map(|c| c.to_ascii_uppercase().to_string() + cs.as_str())
                .unwrap_or_default()
        })
        .collect()
}

fn newtype(type_name: &str, field: &str) -> Option<&'static str> {
    NEWTYPE
        .iter()
        .find(|(t, f, _)| *t == type_name && *f == field)
        .map(|(_, _, r)| *r)
}

fn handle_rust(resource: &str) -> &'static str {
    HANDLES
        .iter()
        .find(|(w, _)| *w == resource)
        .map(|(_, r)| *r)
        .unwrap_or_else(|| panic!("no Rust handle type for the resource {resource}"))
}

/// The function or closure that encodes a `ty` from a reference to it.
fn to_fn(cx: &mut Cx, ty: &Ty, newtype: Option<&'static str>) -> String {
    if newtype.is_some() {
        return "|x| string_to(x.as_str())".into();
    }
    match ty {
        Ty::Bool => "bool_to".into(),
        Ty::U8 | Ty::F32 => panic!(
            "{} has a bare {ty:?}, which is only a run's element",
            cx.type_name
        ),
        Ty::U32 => "u32_to".into(),
        Ty::S64 => "s64_to".into(),
        Ty::F64 => "f64_to".into(),
        Ty::String => "string_to".into(),
        Ty::Id => "id_to".into(),
        Ty::Named(n) => named_fn(cx, n, "to"),
        Ty::Handle(_) => "handle_to".into(),
        Ty::List(_) if ty.run().is_some() => format!("|x| run_{}_to(x)", ty.run().unwrap()),
        Ty::List(t) => format!("|x| list_to(x, {})", to_fn(cx, t, None)),
        Ty::Option(t) => format!("|x| option_to(x, {})", to_fn(cx, t, None)),
        Ty::Tuple(ts) => {
            assert!(ts.windows(2).all(|w| w[0] == w[1]), "a mixed tuple");
            format!("|x| array_to(x, {})", to_fn(cx, &ts[0], None))
        }
        Ty::Result(..) => panic!("a result is no field"),
    }
}

/// The expression encoding `arg`, a reference to a `ty`.
fn to_call(cx: &mut Cx, ty: &Ty, newtype: Option<&'static str>, arg: &str) -> String {
    if newtype.is_some() {
        return format!("string_to({arg}.as_str())");
    }
    match ty {
        Ty::List(_) if ty.run().is_some() => format!("run_{}_to({arg})", ty.run().unwrap()),
        Ty::List(t) => format!("list_to({arg}, {})", to_fn(cx, t, None)),
        Ty::Option(t) => format!("option_to({arg}, {})", to_fn(cx, t, None)),
        Ty::Tuple(ts) => {
            assert!(ts.windows(2).all(|w| w[0] == w[1]), "a mixed tuple");
            format!("array_to({arg}, {})", to_fn(cx, &ts[0], None))
        }
        _ => format!("{}({arg})", to_fn(cx, ty, None)),
    }
}

/// The function or closure decoding a `&Value` into a `ty`.
fn from_fn(cx: &mut Cx, ty: &Ty, newtype: Option<&'static str>) -> String {
    if let Some(rust) = newtype {
        cx.rust_used.insert(rust);
        return format!("|v| newtype_from(v, {rust}::new)");
    }
    match ty {
        Ty::Bool => "bool_from".into(),
        Ty::U8 | Ty::F32 => panic!(
            "{} has a bare {ty:?}, which is only a run's element",
            cx.type_name
        ),
        Ty::U32 => "u32_from".into(),
        Ty::S64 => "s64_from".into(),
        Ty::F64 => "f64_from".into(),
        Ty::String => "string_from".into(),
        Ty::Id => "id_from".into(),
        Ty::Named(n) => named_fn(cx, n, "from"),
        Ty::Handle(r) => {
            let rust = handle_rust(r);
            cx.rust_used.insert(rust);
            format!("|v| handle_from(v).map({rust}::from_handle)")
        }
        Ty::List(_) if ty.run().is_some() => format!("run_{}_from", ty.run().unwrap()),
        Ty::List(t) => format!("|v| list_from(v, {})", from_fn(cx, t, None)),
        Ty::Option(t) => format!("|v| option_from(v, {})", from_fn(cx, t, None)),
        Ty::Tuple(ts) => {
            assert!(ts.windows(2).all(|w| w[0] == w[1]), "a mixed tuple");
            format!(
                "|v| array_from::<_, {}>(v, {})",
                ts.len(),
                from_fn(cx, &ts[0], None)
            )
        }
        Ty::Result(..) => panic!("a result is no field"),
    }
}

/// The expression decoding `arg`, a `&Value`, into a `ty`.
fn from_call(cx: &mut Cx, ty: &Ty, arg: &str) -> String {
    match ty {
        Ty::Handle(r) => {
            let rust = handle_rust(r);
            cx.rust_used.insert(rust);
            format!("handle_from({arg}).map({rust}::from_handle)")
        }
        Ty::List(_) if ty.run().is_some() => format!("run_{}_from({arg})", ty.run().unwrap()),
        Ty::List(t) => format!("list_from({arg}, {})", from_fn(cx, t, None)),
        Ty::Option(t) => format!("option_from({arg}, {})", from_fn(cx, t, None)),
        Ty::Tuple(ts) => {
            assert!(ts.windows(2).all(|w| w[0] == w[1]), "a mixed tuple");
            format!(
                "array_from::<_, {}>({arg}, {})",
                ts.len(),
                from_fn(cx, &ts[0], None)
            )
        }
        _ => format!("{}({arg})", from_fn(cx, ty, None)),
    }
}

fn named_fn(cx: &mut Cx, name: &str, dir: &str) -> String {
    if INLINE.contains(&name) {
        panic!("{} reaches the inline type {name}", cx.type_name);
    }
    if CUSTOM.iter().any(|(w, _)| *w == name) {
        cx.custom_used.insert(snake(name));
    }
    format!("{}_{dir}_json", snake(name))
}

fn record(out: &mut String, cx: &mut Cx, name: &str, rust: &str, fields: &[Field]) {
    let n = snake(name);
    let _ = writeln!(
        out,
        "pub fn {n}_to_json(v: &{rust}) -> Result<Value, WireError> {{\n    let mut m = Map::new();"
    );
    for f in fields {
        let call = to_call(
            cx,
            &f.ty,
            newtype(name, &f.name),
            &format!("&v.{}", snake(&f.name)),
        );
        let _ = writeln!(out, "    put(&mut m, \"{}\", {call})?;", f.name);
    }
    let _ = writeln!(out, "    Ok(Value::Object(m))\n}}\n");

    let allowed = fields
        .iter()
        .map(|f| format!("\"{}\"", f.name))
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(
        out,
        "pub fn {n}_from_json(v: &Value) -> Result<{rust}, WireError> {{\n    let m = object(v)?;\n    deny_unknown(m, &[{allowed}])?;\n    Ok({rust} {{"
    );
    for f in fields {
        let nt = newtype(name, &f.name);
        let expr = match (&f.ty, nt) {
            (Ty::Option(t), None) => {
                format!("opt_field(m, \"{}\", {})?", f.name, from_fn(cx, t, None))
            }
            _ => format!("field(m, \"{}\", {})?", f.name, from_fn(cx, &f.ty, nt)),
        };
        let _ = writeln!(out, "        {}: {expr},", snake(&f.name));
    }
    let _ = writeln!(out, "    }})\n}}\n");
}

fn enumeration(out: &mut String, name: &str, rust: &str, cases: &[String]) {
    let n = snake(name);
    let _ = writeln!(
        out,
        "pub fn {n}_to_json(v: &{rust}) -> Result<Value, WireError> {{\n    Ok(Value::from(match v {{"
    );
    for c in cases {
        let _ = writeln!(out, "        {rust}::{} => \"{c}\",", pascal(c));
    }
    let _ = writeln!(out, "    }}))\n}}\n");

    let _ = writeln!(
        out,
        "pub fn {n}_from_json(v: &Value) -> Result<{rust}, WireError> {{\n    match str_from(v)? {{"
    );
    for c in cases {
        let _ = writeln!(out, "        \"{c}\" => Ok({rust}::{}),", pascal(c));
    }
    let list = cases
        .iter()
        .map(|c| format!("\"{c}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(
        out,
        "        other => Err(unknown_case(\"{name}\", other, &[{list}])),\n    }}\n}}\n"
    );
}

fn variant(out: &mut String, cx: &mut Cx, name: &str, rust: &str, cases: &[Case]) {
    let n = snake(name);
    let _ = writeln!(
        out,
        "pub fn {n}_to_json(v: &{rust}) -> Result<Value, WireError> {{\n    match v {{"
    );
    for c in cases {
        let case = pascal(&c.name);
        match &c.ty {
            Some(t) => {
                let call = to_call(cx, t, None, "x");
                let _ = writeln!(
                    out,
                    "        {rust}::{case}(x) => variant_to(\"{}\", {call}),",
                    c.name
                );
            }
            None => {
                let _ = writeln!(
                    out,
                    "        {rust}::{case} => variant_to(\"{}\", Ok(Value::Null)),",
                    c.name
                );
            }
        }
    }
    let _ = writeln!(out, "    }}\n}}\n");

    let _ = writeln!(
        out,
        "pub fn {n}_from_json(v: &Value) -> Result<{rust}, WireError> {{\n    let (case, payload) = variant(v)?;\n    match case {{"
    );
    for c in cases {
        let case = pascal(&c.name);
        match &c.ty {
            Some(t) => {
                let decode = from_call(cx, t, "payload");
                let _ = writeln!(
                    out,
                    "        \"{}\" => variant_payload(\"{}\", {decode}).map({rust}::{case}),",
                    c.name, c.name
                );
            }
            None => {
                let _ = writeln!(
                    out,
                    "        \"{}\" => no_payload(payload).map(|()| {rust}::{case}),",
                    c.name
                );
            }
        }
    }
    let list = cases
        .iter()
        .map(|c| format!("\"{}\"", c.name))
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(
        out,
        "        other => Err(unknown_case(\"{name}\", other, &[{list}])),\n    }}\n}}\n"
    );
}

fn methods(out: &mut String, world: &World) {
    out.push_str(
        "/// Every function of the world, in its order.\npub const METHODS: &[Method] = &[\n",
    );
    for f in &world.funcs {
        let dir = match f.direction {
            Direction::HostToPlugin => "HostToPlugin",
            Direction::PluginToHost => "PluginToHost",
        };
        let _ = writeln!(
            out,
            "    Method {{ name: \"{}\", direction: Direction::{dir} }},",
            f.method()
        );
    }
    out.push_str("];\n");
}

fn rustfmt(source: &str) -> String {
    use std::io::Write as _;
    use std::process::{Command, Stdio};
    let mut child = Command::new("rustfmt")
        .args(["--edition", "2024", "--emit", "stdout"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("rustfmt is part of the toolchain (rust-toolchain.toml)");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "rustfmt refused the generated codec: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}
