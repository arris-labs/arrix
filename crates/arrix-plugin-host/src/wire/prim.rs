//! What the generated codec is made of: the shapes of ADR-0022 §3 for the
//! primitives, lists, options, variants and ids, and the error that says
//! where in a value a shape was wrong.

use std::fmt;

use arrix_core::{
    AssemblyId, ComponentId, CurveKey, FeatureId, Id, JointId, ParamId, PartId, RecordId,
    SketchEntityId,
};
use arrix_plugin_api::Body;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Map, Number, Value};

/// Past this a JSON number is not exact in every reader (a double).
const SAFE_INT: i64 = 1 << 53;

/// A value that is not the shape the schema gives, or that a Rust type
/// refuses; `path` says where, from the value the codec was handed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WireError {
    path: Vec<String>,
    pub message: String,
}

pub type Wire<T> = Result<T, WireError>;

impl WireError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            path: Vec::new(),
            message: message.into(),
        }
    }

    /// The same error one level further out: inside field, case or index
    /// `segment` of the value that holds it.
    pub fn at(mut self, segment: impl Into<String>) -> Self {
        self.path.insert(0, segment.into());
        self
    }

    /// Where the error is, `outer.holes[1].radius`, or empty at the root.
    pub fn path(&self) -> String {
        let mut out = String::new();
        for s in &self.path {
            if !out.is_empty() && !s.starts_with('[') {
                out.push('.');
            }
            out.push_str(s);
        }
        out
    }
}

impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.path().as_str() {
            "" => f.write_str(&self.message),
            path => write!(f, "{path}: {}", self.message),
        }
    }
}

impl std::error::Error for WireError {}

fn kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

fn expected(what: &str, v: &Value) -> WireError {
    WireError::new(format!("expected {what}, found {}", kind(v)))
}

// ---- ids ------------------------------------------------------------------

/// The typed ids of `arrix-core`, which all cross as `types.id`.
pub trait WireId: Sized {
    fn to_id(&self) -> Id;
    fn from_id(id: Id) -> Self;
}

impl WireId for Id {
    fn to_id(&self) -> Id {
        *self
    }
    fn from_id(id: Id) -> Self {
        id
    }
}

macro_rules! typed_wire_ids {
    ($($t:ty),*) => {$(
        impl WireId for $t {
            fn to_id(&self) -> Id {
                self.0
            }
            fn from_id(id: Id) -> Self {
                Self(id)
            }
        }
    )*};
}

typed_wire_ids!(
    FeatureId,
    ParamId,
    SketchEntityId,
    RecordId,
    CurveKey,
    PartId,
    AssemblyId,
    ComponentId,
    JointId
);

pub fn id_to<T: WireId>(x: &T) -> Wire<Value> {
    Ok(Value::String(x.to_id().to_string()))
}

pub fn id_from<T: WireId>(v: &Value) -> Wire<T> {
    str_from(v)?
        .parse::<Id>()
        .map(T::from_id)
        .map_err(|e| WireError::new(e.to_string()))
}

// ---- primitives -----------------------------------------------------------

pub fn bool_to(x: &bool) -> Wire<Value> {
    Ok(Value::Bool(*x))
}

pub fn bool_from(v: &Value) -> Wire<bool> {
    v.as_bool().ok_or_else(|| expected("a boolean", v))
}

pub fn f64_to(x: &f64) -> Wire<Value> {
    Number::from_f64(*x)
        .map(Value::Number)
        .ok_or_else(|| WireError::new(format!("{x} has no JSON number")))
}

pub fn f64_from(v: &Value) -> Wire<f64> {
    v.as_f64().ok_or_else(|| expected("a number", v))
}

/// An integer, written `3` or `3.0` (a reader in a language whose numbers
/// are doubles may write either).
fn integer_from(v: &Value) -> Wire<i64> {
    if let Some(n) = v.as_i64() {
        return Ok(n);
    }
    match v.as_f64() {
        Some(f) if f.fract() == 0.0 && f.abs() <= SAFE_INT as f64 => Ok(f as i64),
        Some(f) => Err(WireError::new(format!("{f} is not an integer in range"))),
        None => Err(expected("an integer", v)),
    }
}

pub fn u32_to(x: &u32) -> Wire<Value> {
    Ok(Value::from(*x))
}

pub fn u32_from(v: &Value) -> Wire<u32> {
    let n = integer_from(v)?;
    u32::try_from(n).map_err(|_| WireError::new(format!("{n} is not a u32")))
}

pub fn s64_to(x: &i64) -> Wire<Value> {
    if x.abs() > SAFE_INT {
        return Err(WireError::new(format!(
            "{x} is past ±2^53, which a JSON number holds exactly"
        )));
    }
    Ok(Value::from(*x))
}

pub fn s64_from(v: &Value) -> Wire<i64> {
    let n = integer_from(v)?;
    if n.abs() > SAFE_INT {
        return Err(WireError::new(format!("{n} is past ±2^53")));
    }
    Ok(n)
}

pub fn string_to<S: AsRef<str> + ?Sized>(x: &S) -> Wire<Value> {
    Ok(Value::String(x.as_ref().to_owned()))
}

pub fn str_from(v: &Value) -> Wire<&str> {
    v.as_str().ok_or_else(|| expected("a string", v))
}

pub fn string_from(v: &Value) -> Wire<String> {
    str_from(v).map(str::to_owned)
}

/// A string a Rust type checks: its constructor's refusal is the error.
pub fn newtype_from<T, E: fmt::Display>(
    v: &Value,
    make: impl FnOnce(String) -> Result<T, E>,
) -> Wire<T> {
    make(string_from(v)?).map_err(|e| WireError::new(e.to_string()))
}

/// A resource crosses as its handle, a number.
pub fn handle_to(body: &Body) -> Wire<Value> {
    u32_to(&body.handle())
}

pub fn handle_from(v: &Value) -> Wire<u32> {
    u32_from(v)
}

// ---- runs -----------------------------------------------------------------

/// A run of `u8`, `f32` or `u32` is one base64 string of its little-endian
/// bytes (ADR-0022 §5), where a long array of numbers would be a line of
/// text per number.
fn run_bytes(v: &Value) -> Wire<Vec<u8>> {
    STANDARD
        .decode(str_from(v)?)
        .map_err(|e| WireError::new(format!("not base64: {e}")))
}

fn run_of<const N: usize, T>(v: &Value, what: &str, f: fn([u8; N]) -> T) -> Wire<Vec<T>> {
    let bytes = run_bytes(v)?;
    if bytes.len() % N != 0 {
        return Err(WireError::new(format!(
            "{} bytes are no run of {what} ({N} bytes each)",
            bytes.len()
        )));
    }
    Ok(bytes.as_chunks::<N>().0.iter().map(|c| f(*c)).collect())
}

pub fn run_u8_to(x: &[u8]) -> Wire<Value> {
    Ok(Value::String(STANDARD.encode(x)))
}

pub fn run_u8_from(v: &Value) -> Wire<Vec<u8>> {
    run_bytes(v)
}

pub fn run_f32_to(x: &[f32]) -> Wire<Value> {
    let bytes: Vec<u8> = x.iter().flat_map(|f| f.to_le_bytes()).collect();
    Ok(Value::String(STANDARD.encode(bytes)))
}

pub fn run_f32_from(v: &Value) -> Wire<Vec<f32>> {
    run_of(v, "f32", f32::from_le_bytes)
}

pub fn run_u32_to(x: &[u32]) -> Wire<Value> {
    let bytes: Vec<u8> = x.iter().flat_map(|n| n.to_le_bytes()).collect();
    Ok(Value::String(STANDARD.encode(bytes)))
}

pub fn run_u32_from(v: &Value) -> Wire<Vec<u32>> {
    run_of(v, "u32", u32::from_le_bytes)
}

// ---- containers -----------------------------------------------------------

pub fn list_to<T>(xs: &[T], f: impl Fn(&T) -> Wire<Value>) -> Wire<Value> {
    xs.iter()
        .enumerate()
        .map(|(i, x)| f(x).map_err(|e| e.at(format!("[{i}]"))))
        .collect::<Wire<Vec<_>>>()
        .map(Value::Array)
}

/// A tuple of equal types, as the array it is (ADR-0022 §3).
pub use list_to as array_to;

pub fn list_from<T>(v: &Value, f: impl Fn(&Value) -> Wire<T>) -> Wire<Vec<T>> {
    let Value::Array(xs) = v else {
        return Err(expected("an array", v));
    };
    xs.iter()
        .enumerate()
        .map(|(i, x)| f(x).map_err(|e| e.at(format!("[{i}]"))))
        .collect()
}

pub fn array_from<T, const N: usize>(v: &Value, f: impl Fn(&Value) -> Wire<T>) -> Wire<[T; N]> {
    let items = list_from(v, f)?;
    let n = items.len();
    items
        .try_into()
        .map_err(|_| WireError::new(format!("expected {N} elements, found {n}")))
}

pub fn option_to<T>(x: &Option<T>, f: impl Fn(&T) -> Wire<Value>) -> Wire<Value> {
    x.as_ref().map_or(Ok(Value::Null), f)
}

pub fn option_from<T>(v: &Value, f: impl Fn(&Value) -> Wire<T>) -> Wire<Option<T>> {
    match v {
        Value::Null => Ok(None),
        v => f(v).map(Some),
    }
}

// ---- records --------------------------------------------------------------

pub fn object(v: &Value) -> Wire<&Map<String, Value>> {
    v.as_object().ok_or_else(|| expected("an object", v))
}

/// Refuses a key the schema does not name (`additionalProperties: false`).
pub fn deny_unknown(m: &Map<String, Value>, allowed: &[&str]) -> Wire<()> {
    match m.keys().find(|k| !allowed.contains(&k.as_str())) {
        Some(k) => Err(WireError::new(format!(
            "`{k}` is no field here (fields: {})",
            allowed.join(", ")
        ))),
        None => Ok(()),
    }
}

pub fn put(m: &mut Map<String, Value>, name: &str, v: Wire<Value>) -> Wire<()> {
    m.insert(name.to_owned(), v.map_err(|e| e.at(name))?);
    Ok(())
}

pub fn field<T>(m: &Map<String, Value>, name: &str, f: impl FnOnce(&Value) -> Wire<T>) -> Wire<T> {
    let v = m
        .get(name)
        .ok_or_else(|| WireError::new(format!("missing field `{name}`")))?;
    f(v).map_err(|e| e.at(name))
}

/// An option field: left out or null is none.
pub fn opt_field<T>(
    m: &Map<String, Value>,
    name: &str,
    f: impl FnOnce(&Value) -> Wire<T>,
) -> Wire<Option<T>> {
    match m.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => f(v).map(Some).map_err(|e| e.at(name)),
    }
}

// ---- variants and enums ---------------------------------------------------

/// `{"<case>": payload}`.
pub fn variant_to(case: &str, payload: Wire<Value>) -> Wire<Value> {
    let mut m = Map::new();
    put(&mut m, case, payload)?;
    Ok(Value::Object(m))
}

/// The case and payload of a one-key object.
pub fn variant(v: &Value) -> Wire<(&str, &Value)> {
    let m = object(v)?;
    let mut entries = m.iter();
    match (entries.next(), entries.next()) {
        (Some((case, payload)), None) => Ok((case, payload)),
        _ => Err(WireError::new(format!(
            "a variant is an object with exactly one key, found {}",
            m.len()
        ))),
    }
}

pub fn variant_payload<T>(case: &str, decoded: Wire<T>) -> Wire<T> {
    decoded.map_err(|e| e.at(case))
}

/// A case with no payload carries null.
pub fn no_payload(v: &Value) -> Wire<()> {
    match v {
        Value::Null => Ok(()),
        v => Err(expected("null", v)),
    }
}

pub fn unknown_case(of: &str, case: &str, cases: &[&str]) -> WireError {
    WireError::new(format!(
        "`{case}` is no case of {of} (cases: {})",
        cases.join(", ")
    ))
}
