//! A checker for the subset of JSON Schema the generated wire uses
//! (`arrix-plugin-api/wire/`), and the record of which branches it saw, so
//! a test can demand that its samples cover every case of the world.

#![allow(dead_code)]

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde_json::Value;

pub struct Schemas {
    pub types: Value,
    pub methods: Value,
}

fn wire_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../arrix-plugin-api/wire")
}

impl Schemas {
    pub fn load() -> Schemas {
        let read = |f: &str| {
            serde_json::from_str(&std::fs::read_to_string(wire_dir().join(f)).unwrap()).unwrap()
        };
        Schemas {
            types: read("schema.json"),
            methods: read("methods.json"),
        }
    }

    pub fn def(&self, name: &str) -> &Value {
        &self.types["$defs"][name]
    }

    pub fn def_names(&self) -> Vec<String> {
        self.types["$defs"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect()
    }

    /// Every branch of every type the samples must reach: each case of a
    /// variant (`name#index`), each word of an enum (`name=word`), each
    /// type at all (`name`).
    pub fn expected_coverage(&self) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for (name, def) in self.types["$defs"].as_object().unwrap() {
            out.insert(name.clone());
            if let Some(branches) = def["oneOf"].as_array() {
                out.extend((0..branches.len()).map(|i| format!("{name}#{i}")));
            }
            if let Some(words) = def["enum"].as_array() {
                out.extend(
                    words
                        .iter()
                        .map(|w| format!("{name}={}", w.as_str().unwrap())),
                );
            }
        }
        out
    }
}

pub struct Check<'a> {
    pub schemas: &'a Schemas,
    pub seen: BTreeSet<String>,
}

impl<'a> Check<'a> {
    pub fn new(schemas: &'a Schemas) -> Self {
        Check {
            schemas,
            seen: BTreeSet::new(),
        }
    }

    /// `value` against the schema of the type `name`.
    pub fn type_ok(&mut self, name: &str, value: &Value) -> Result<(), String> {
        let schema = self.schemas.def(name).clone();
        assert!(!schema.is_null(), "no type {name} in schema.json");
        self.seen.insert(name.to_owned());
        if let Some(branches) = schema["oneOf"].as_array() {
            let matching: Vec<usize> = (0..branches.len())
                .filter(|&i| self.valid(&branches[i], value))
                .collect();
            if let [i] = matching[..] {
                self.seen.insert(format!("{name}#{i}"));
                return self.check(&branches[i], value);
            }
            return Err(format!("{name}: {} branches match {value}", matching.len()));
        }
        if let (Some(words), Some(w)) = (schema["enum"].as_array(), value.as_str())
            && words.iter().any(|x| x == w)
        {
            self.seen.insert(format!("{name}={w}"));
        }
        self.check(&schema, value)
            .map_err(|e| format!("{name}: {e}"))
    }

    /// `value` against the params of `method`, or its result.
    pub fn method_ok(&mut self, method: &str, part: &str, value: &Value) -> Result<(), String> {
        let m = self.schemas.methods["methods"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["method"] == method)
            .unwrap_or_else(|| panic!("no method {method}"))
            .clone();
        self.check(&m[part], value)
    }

    fn valid(&mut self, schema: &Value, value: &Value) -> bool {
        let before = self.seen.clone();
        let ok = self.check(schema, value).is_ok();
        self.seen = before;
        ok
    }

    pub fn check(&mut self, schema: &Value, value: &Value) -> Result<(), String> {
        if let Some(r) = schema["$ref"].as_str() {
            let name = r.rsplit('/').next().unwrap();
            return self.type_ok(name, value);
        }
        if let Some(ty) = schema["type"].as_str() {
            let ok = match ty {
                "object" => value.is_object(),
                "array" => value.is_array(),
                "string" => value.is_string(),
                "boolean" => value.is_boolean(),
                "null" => value.is_null(),
                "number" => value.is_number(),
                "integer" => {
                    value.is_i64()
                        || value.is_u64()
                        || value.as_f64().is_some_and(|f| f.fract() == 0.0)
                }
                other => panic!("unsupported type {other}"),
            };
            if !ok {
                return Err(format!("{value} is no {ty}"));
            }
        }
        if let Some(words) = schema["enum"].as_array()
            && !words.contains(value)
        {
            return Err(format!("{value} is none of {words:?}"));
        }
        if let (Some(p), Some(s)) = (schema["pattern"].as_str(), value.as_str())
            && !regex::Regex::new(p).unwrap().is_match(s)
        {
            return Err(format!("{s:?} does not match {p}"));
        }
        if let Some(n) = value.as_f64().filter(|_| value.is_number())
            && (schema["minimum"].as_f64().is_some_and(|min| n < min)
                || schema["maximum"].as_f64().is_some_and(|max| n > max))
        {
            return Err(format!("{n} is out of range"));
        }
        if let Value::Object(m) = value {
            let props = schema["properties"].as_object();
            for r in schema["required"].as_array().into_iter().flatten() {
                if !m.contains_key(r.as_str().unwrap()) {
                    return Err(format!("missing {r}"));
                }
            }
            for (k, v) in m {
                match props.and_then(|p| p.get(k)) {
                    Some(s) => self.check(s, v).map_err(|e| format!("{k}: {e}"))?,
                    None if schema["additionalProperties"] == false => {
                        return Err(format!("unexpected key {k}"));
                    }
                    None => {}
                }
            }
        }
        if let Value::Array(xs) = value {
            if schema["minItems"]
                .as_u64()
                .is_some_and(|n| (xs.len() as u64) < n)
                || schema["maxItems"]
                    .as_u64()
                    .is_some_and(|n| xs.len() as u64 > n)
            {
                return Err(format!("{} items", xs.len()));
            }
            for (i, x) in xs.iter().enumerate() {
                let s = schema["prefixItems"]
                    .get(i)
                    .or_else(|| schema.get("items").filter(|s| s.is_object()));
                if let Some(s) = s {
                    self.check(s, x).map_err(|e| format!("[{i}]: {e}"))?;
                }
            }
        }
        if let Some(branches) = schema["anyOf"].as_array() {
            if !branches.iter().any(|b| self.valid(b, value)) {
                return Err(format!("{value} matches none of the alternatives"));
            }
            // Record what the matching branch reached.
            let first = branches
                .iter()
                .find(|b| self.valid(b, value))
                .unwrap()
                .clone();
            self.check(&first, value)?;
        }
        if let Some(branches) = schema["oneOf"].as_array() {
            let n = branches.iter().filter(|b| self.valid(b, value)).count();
            if n != 1 {
                return Err(format!(
                    "{n} of {} alternatives match {value}",
                    branches.len()
                ));
            }
            let one = branches
                .iter()
                .find(|b| self.valid(b, value))
                .unwrap()
                .clone();
            self.check(&one, value)?;
        }
        Ok(())
    }
}
