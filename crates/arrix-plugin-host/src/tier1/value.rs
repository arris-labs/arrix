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
    object, run_f32_from, run_f32_to, run_u8_from, run_u8_to, run_u32_from, run_u32_to, s64_from,
    s64_to, str_from, string_from, string_to, u32_from, u32_to, unknown_case, variant,
    variant_payload, variant_to,
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

/// What the guest handed the host, a value of type `ty`, as the wire
/// carries it. The type decides what a list is: the world's only lists of
/// `u8`, `f32` and `u32` are runs (a file, a mesh), base64 on the wire
/// whether empty or not.
pub(super) fn to_json(v: &Val, ty: &Type, store: &mut impl AsContextMut) -> Wire<Value> {
    Ok(match (v, ty) {
        (Val::Bool(b), _) => bool_to(b)?,
        (Val::U32(n), _) => u32_to(n)?,
        (Val::S64(n), _) => s64_to(n)?,
        (Val::U64(n), _) => id_to(&Id(*n))?,
        (Val::Float64(x), _) => f64_to(x)?,
        (Val::String(s), _) => string_to(s)?,
        (Val::List(xs), Type::List(l)) => match l.ty() {
            Type::U8 => run_u8_to(&run(xs, |x| match x {
                Val::U8(n) => Some(*n),
                _ => None,
            })?)?,
            Type::Float32 => run_f32_to(&run(xs, |x| match x {
                Val::Float32(n) => Some(*n),
                _ => None,
            })?)?,
            Type::U32 => run_u32_to(&run(xs, |x| match x {
                Val::U32(n) => Some(*n),
                _ => None,
            })?)?,
            elem => Value::Array(
                xs.iter()
                    .enumerate()
                    .map(|(i, x)| to_json(x, &elem, store).map_err(|e| e.at(format!("[{i}]"))))
                    .collect::<Wire<_>>()?,
            ),
        },
        (Val::Tuple(xs), Type::Tuple(t)) => {
            let tys: Vec<_> = t.types().collect();
            Value::Array(
                xs.iter()
                    .zip(&tys)
                    .enumerate()
                    .map(|(i, (x, ty))| to_json(x, ty, store).map_err(|e| e.at(format!("[{i}]"))))
                    .collect::<Wire<_>>()?,
            )
        }
        (Val::Record(fields), Type::Record(r)) => {
            let tys: Vec<_> = r.fields().collect();
            let mut m = Map::new();
            for (name, x) in fields {
                let Some(f) = tys.iter().find(|f| f.name == name) else {
                    return Err(outside(&format!("the field `{name}`")));
                };
                m.insert(
                    name.clone(),
                    to_json(x, &f.ty, store).map_err(|e| e.at(name))?,
                );
            }
            Value::Object(m)
        }
        (Val::Variant(case, payload), Type::Variant(t)) => {
            let ty = t
                .cases()
                .find(|c| c.name == case.as_str())
                .and_then(|c| c.ty);
            variant_to(case, payload_to_json(payload, ty.as_ref(), store))?
        }
        (Val::Enum(case), _) => Value::String(case.clone()),
        (Val::Option(None), _) => Value::Null,
        (Val::Option(Some(x)), Type::Option(o)) => to_json(x, &o.ty(), store)?,
        (Val::Result(Ok(x)), Type::Result(r)) => {
            variant_to("ok", payload_to_json(x, r.ok().as_ref(), store))?
        }
        (Val::Result(Err(x)), Type::Result(r)) => {
            variant_to("err", payload_to_json(x, r.err().as_ref(), store))?
        }
        (Val::Resource(r), _) => {
            let body = Resource::<BodyRep>::try_from_resource_any(*r, &mut *store);
            Value::from(body.map_err(resource)?.rep())
        }
        (other, _) => return Err(outside(&format!("{other:?}"))),
    })
}

/// The numbers of a run, each picked out of its `Val`.
fn run<T>(xs: &[Val], pick: impl Fn(&Val) -> Option<T>) -> Wire<Vec<T>> {
    xs.iter()
        .map(|x| pick(x).ok_or_else(|| WireError::new("a run holds one kind of number")))
        .collect()
}

fn payload_to_json(
    payload: &Option<Box<Val>>,
    ty: Option<&Type>,
    store: &mut impl AsContextMut,
) -> Wire<Value> {
    match (payload.as_deref(), ty) {
        (Some(x), Some(ty)) => to_json(x, ty, store),
        _ => Ok(Value::Null),
    }
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
        Type::List(l) => match l.ty() {
            Type::U8 => Val::List(run_u8_from(v)?.into_iter().map(Val::U8).collect()),
            Type::Float32 => Val::List(run_f32_from(v)?.into_iter().map(Val::Float32).collect()),
            Type::U32 => Val::List(run_u32_from(v)?.into_iter().map(Val::U32).collect()),
            elem => Val::List(items(v, |_| elem.clone(), store)?),
        },
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
