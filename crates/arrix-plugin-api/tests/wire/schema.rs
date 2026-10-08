//! The JSON Schemas and the method table, from the model (ADR-0022 §3).
//!
//! `schema.json` holds one schema per type under `$defs`; `methods.json`
//! one entry per function, its params and result as schemas that point at
//! the types. Keys are sorted (serde_json's map), so the files are stable.

use serde_json::{Map, Value, json};

use super::model::{Def, Field, Func, Ty, TypeDef, World};

/// A JSON number a reader and a writer in any language hold exactly.
const SAFE_INT: i64 = 1 << 53;

/// The canonical text of an id: 13 Crockford base32 characters, the first
/// at most `F` (docs/DATA-MODEL.md §Identifiers).
const ID_PATTERN: &str = "^[0-9A-F][0-9A-HJKMNP-TV-Z]{12}$";

pub fn schema_json(world: &World) -> String {
    let defs: Map<String, Value> = world
        .types
        .iter()
        .map(|t| (t.name.clone(), type_schema(t)))
        .collect();
    let doc = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": format!("arrix:plugin@{}/schema", world.version),
        "title": format!("The wire's types, arrix:plugin@{}", world.version),
        "description": "Generated from wit/arrix-plugin.wit (ADR-0022). Do not edit.",
        "$defs": defs,
    });
    pretty(&doc)
}

pub fn methods_json(world: &World) -> String {
    let methods: Vec<Value> = world.funcs.iter().map(method).collect();
    let doc = json!({
        "api": world.version,
        "description": "Generated from wit/arrix-plugin.wit (ADR-0022). Do not edit. \
            Each method's params and result are schemas over schema.json's $defs.",
        "methods": methods,
    });
    pretty(&doc)
}

fn pretty(v: &Value) -> String {
    let mut s = serde_json::to_string_pretty(v).unwrap();
    s.push('\n');
    s
}

fn with_docs(mut schema: Value, docs: &str) -> Value {
    if !docs.is_empty() {
        schema["description"] = docs.into();
    }
    schema
}

fn type_schema(t: &TypeDef) -> Value {
    let s = match &t.def {
        Def::Id => json!({ "type": "string", "pattern": ID_PATTERN }),
        Def::Resource => json!({
            "type": "integer",
            "minimum": 0,
            "maximum": u32::MAX,
            "x-handle": t.name,
        }),
        Def::Record(fields) => record(fields),
        Def::Variant(cases) => json!({
            "oneOf": cases
                .iter()
                .map(|c| with_docs(
                    one_key(&c.name, c.ty.as_ref().map_or(json!({ "type": "null" }), ty)),
                    &c.docs,
                ))
                .collect::<Vec<_>>(),
        }),
        Def::Enum(cases) => json!({ "type": "string", "enum": cases }),
    };
    with_docs(s, &t.docs)
}

/// An object with exactly the one key `name`.
fn one_key(name: &str, schema: Value) -> Value {
    json!({
        "type": "object",
        "properties": { name: schema },
        "required": [name],
        "additionalProperties": false,
    })
}

/// An option field may be left out as well as null; every other field is
/// required.
fn record(fields: &[Field]) -> Value {
    let properties: Map<String, Value> = fields
        .iter()
        .map(|f| (f.name.clone(), with_docs(ty(&f.ty), &f.docs)))
        .collect();
    let required: Vec<&str> = fields
        .iter()
        .filter(|f| !matches!(f.ty, Ty::Option(_)))
        .map(|f| f.name.as_str())
        .collect();
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
}

fn reference(name: &str) -> Value {
    json!({ "$ref": format!("#/$defs/{name}") })
}

fn ty(t: &Ty) -> Value {
    match t {
        Ty::Bool => json!({ "type": "boolean" }),
        Ty::U32 => json!({ "type": "integer", "minimum": 0, "maximum": u32::MAX }),
        Ty::S64 => json!({ "type": "integer", "minimum": -SAFE_INT, "maximum": SAFE_INT }),
        Ty::F64 => json!({ "type": "number" }),
        Ty::String => json!({ "type": "string" }),
        Ty::Id => reference("id"),
        Ty::Named(name) | Ty::Handle(name) => reference(name),
        Ty::List(t) => json!({ "type": "array", "items": ty(t) }),
        Ty::Option(t) => json!({ "anyOf": [{ "type": "null" }, ty(t)] }),
        Ty::Tuple(ts) => json!({
            "type": "array",
            "prefixItems": ts.iter().map(ty).collect::<Vec<_>>(),
            "minItems": ts.len(),
            "maxItems": ts.len(),
        }),
        Ty::Result(ok, err) => {
            let side = |name: &str, t: &Option<Box<Ty>>| {
                one_key(name, t.as_deref().map_or(json!({ "type": "null" }), ty))
            };
            json!({ "oneOf": [side("ok", ok), side("err", err)] })
        }
    }
}

fn method(f: &Func) -> Value {
    let properties: Map<String, Value> = f
        .params
        .iter()
        .map(|(name, t)| (name.clone(), ty(t)))
        .collect();
    let names: Vec<&String> = f.params.iter().map(|(n, _)| n).collect();
    let mut m = json!({
        "method": f.method(),
        "direction": f.direction.text(),
        "params": {
            "type": "object",
            "properties": properties,
            "required": names,
            "additionalProperties": false,
        },
        "result": f.result.as_ref().map_or(json!({ "type": "null" }), ty),
    });
    if !f.docs.is_empty() {
        m["description"] = f.docs.clone().into();
    }
    // `$ref`s in methods.json point at schema.json.
    point_at_schema(&mut m);
    m
}

fn point_at_schema(v: &mut Value) {
    match v {
        Value::Object(map) => {
            for (k, x) in map.iter_mut() {
                match (k.as_str(), &*x) {
                    ("$ref", Value::String(r)) => *x = format!("schema.json{r}").into(),
                    _ => point_at_schema(x),
                }
            }
        }
        Value::Array(xs) => xs.iter_mut().for_each(point_at_schema),
        _ => {}
    }
}
