//! A component's values and the wire's JSON (ADR-0022 §3), both ways, so a
//! Tier 1 plugin speaks the world through the codec Tier 2 speaks it
//! through: one decoder checks what any plugin hands the host. The shapes
//! are the wire's: a record an object by WIT name, a variant `{"<case>":
//! payload}`, an enum its case's name, an option null or the value, a
//! result `{"ok": …}` or `{"err": …}`, a `u64` (the world's only one is
//! `types.id`) the id's text, and a `body` its handle, the evaluation's
//! kernel's own handle being the resource's representation.

use arrix_core::Id;
use serde_json::{Map, Value};
use wasmtime::AsContextMut;
use wasmtime::component::{Resource, Type, Val};

use crate::wire::{
    Wire, WireError, bool_from, bool_to, f64_from, f64_to, handle_from, id_from, id_to, no_payload,
    object, s64_from, s64_to, str_from, string_from, string_to, u32_from, u32_to, unknown_case,
    variant, variant_payload, variant_to,
};

/// The host's `body` resource: its representation is the handle the
/// evaluating feature's kernel gave.
pub(crate) struct BodyRep;

fn outside(what: &str) -> WireError {
    WireError::new(format!("{what} is no type of the world"))
}

fn resource(e: wasmtime::Error) -> WireError {
    WireError::new(format!("a body that is not the host's: {e}"))
}

/// What the guest handed the host, as the wire carries it.
pub(super) fn to_json(v: &Val, store: &mut impl AsContextMut) -> Wire<Value> {
    Ok(match v {
        Val::Bool(b) => bool_to(b)?,
        Val::U32(n) => u32_to(n)?,
        Val::S64(n) => s64_to(n)?,
        Val::U64(n) => id_to(&Id(*n))?,
        Val::Float64(x) => f64_to(x)?,
        Val::String(s) => string_to(s)?,
        Val::List(xs) | Val::Tuple(xs) => Value::Array(
            xs.iter()
                .enumerate()
                .map(|(i, x)| to_json(x, store).map_err(|e| e.at(format!("[{i}]"))))
                .collect::<Wire<_>>()?,
        ),
        Val::Record(fields) => {
            let mut m = Map::new();
            for (name, x) in fields {
                m.insert(name.clone(), to_json(x, store).map_err(|e| e.at(name))?);
            }
            Value::Object(m)
        }
        Val::Variant(case, payload) => variant_to(case, payload_to_json(payload, store))?,
        Val::Enum(case) => Value::String(case.clone()),
        Val::Option(None) => Value::Null,
        Val::Option(Some(x)) => to_json(x, store)?,
        Val::Result(Ok(x)) => variant_to("ok", payload_to_json(x, store))?,
        Val::Result(Err(x)) => variant_to("err", payload_to_json(x, store))?,
        Val::Resource(r) => {
            let body = Resource::<BodyRep>::try_from_resource_any(*r, &mut *store);
            Value::from(body.map_err(resource)?.rep())
        }
        other => return Err(outside(&format!("{other:?}"))),
    })
}

fn payload_to_json(payload: &Option<Box<Val>>, store: &mut impl AsContextMut) -> Wire<Value> {
    payload
        .as_deref()
        .map_or(Ok(Value::Null), |x| to_json(x, store))
}

/// What the host hands the guest, a value of type `ty`, from the wire.
pub(super) fn from_json(v: &Value, ty: &Type, store: &mut impl AsContextMut) -> Wire<Val> {
    Ok(match ty {
        Type::Bool => Val::Bool(bool_from(v)?),
        Type::U32 => Val::U32(u32_from(v)?),
        Type::S64 => Val::S64(s64_from(v)?),
        Type::U64 => Val::U64(id_from::<Id>(v)?.0),
        Type::Float64 => Val::Float64(f64_from(v)?),
        Type::String => Val::String(string_from(v)?),
        Type::List(l) => Val::List(items(v, |_| l.ty(), store)?),
        Type::Tuple(t) => {
            let tys: Vec<_> = t.types().collect();
            let xs = items(v, |i| tys.get(i).cloned().unwrap_or(Type::Bool), store)?;
            if xs.len() != tys.len() {
                return Err(WireError::new(format!("expected {} elements", tys.len())));
            }
            Val::Tuple(xs)
        }
        Type::Record(r) => {
            let m = object(v)?;
            let mut fields = Vec::new();
            for f in r.fields() {
                let x = match (m.get(f.name), &f.ty) {
                    (None, Type::Option(_)) => Val::Option(None),
                    (None, _) => return Err(WireError::new(format!("missing field `{}`", f.name))),
                    (Some(x), ty) => from_json(x, ty, store).map_err(|e| e.at(f.name))?,
                };
                fields.push((f.name.to_owned(), x));
            }
            Val::Record(fields)
        }
        Type::Variant(t) => {
            let (case, payload) = variant(v)?;
            let Some(c) = t.cases().find(|c| c.name == case) else {
                let cases: Vec<_> = t.cases().map(|c| c.name).collect();
                return Err(unknown_case("the variant", case, &cases));
            };
            let x = payload_from_json(payload, c.ty.as_ref(), store);
            Val::Variant(case.to_owned(), variant_payload(case, x)?)
        }
        Type::Enum(t) => {
            let case = str_from(v)?;
            if !t.names().any(|n| n == case) {
                let cases: Vec<_> = t.names().collect();
                return Err(unknown_case("the enum", case, &cases));
            }
            Val::Enum(case.to_owned())
        }
        Type::Option(o) => match v {
            Value::Null => Val::Option(None),
            v => Val::Option(Some(Box::new(from_json(v, &o.ty(), store)?))),
        },
        Type::Result(r) => {
            let (case, payload) = variant(v)?;
            let x = |ty: Option<Type>, store: &mut _| {
                variant_payload(case, payload_from_json(payload, ty.as_ref(), store))
            };
            match case {
                "ok" => Val::Result(Ok(x(r.ok(), store)?)),
                "err" => Val::Result(Err(x(r.err(), store)?)),
                other => return Err(unknown_case("result", other, &["ok", "err"])),
            }
        }
        Type::Own(_) => {
            let body = Resource::<BodyRep>::new_own(handle_from(v)?);
            Val::Resource(body.try_into_resource_any(&mut *store).map_err(resource)?)
        }
        other => return Err(outside(&format!("{other:?}"))),
    })
}

fn items(v: &Value, ty: impl Fn(usize) -> Type, store: &mut impl AsContextMut) -> Wire<Vec<Val>> {
    let Value::Array(xs) = v else {
        return Err(WireError::new(format!("expected an array, found {v}")));
    };
    xs.iter()
        .enumerate()
        .map(|(i, x)| from_json(x, &ty(i), store).map_err(|e| e.at(format!("[{i}]"))))
        .collect()
}

fn payload_from_json(
    payload: &Value,
    ty: Option<&Type>,
    store: &mut impl AsContextMut,
) -> Wire<Option<Box<Val>>> {
    match ty {
        Some(ty) => Ok(Some(Box::new(from_json(payload, ty, store)?))),
        None => no_payload(payload).map(|()| None),
    }
}
