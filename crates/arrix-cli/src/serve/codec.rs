//! `serve`'s bulk codec and the schemas of what it writes (ADR-0023 §4,
//! ADR-0022 §5). A mesh's runs of `f32` and `u32`, an `Opened` blob and an
//! export's bytes cross as base64 of their little-endian elements; the
//! in-process serde form is unchanged. The transform works on the event as
//! JSON, and `wire_schemas` rewrites the derived schemas to match, so a test
//! can hold the one to the other.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::{Value, json};

/// The mesh runs and their element type: `f32` triples or `u32` triples.
const F32_RUNS: [&str; 3] = ["positions", "normals", "edge_points"];
const U32_RUNS: [&str; 1] = ["triangles"];

/// An event as `serve` writes it: bulk arrays as base64.
pub fn encode_event(event: &mut Value) {
    match event["kind"].as_str() {
        Some("opened") => {
            if let Some(blobs) = event.get_mut("blobs").and_then(Value::as_object_mut) {
                blobs.values_mut().for_each(bytes_to_base64);
            }
        }
        Some("meshed" | "preview_meshed") => {
            if let Some(mesh) = event.pointer_mut("/mesh/Ok") {
                encode_mesh(mesh);
            }
        }
        Some("exported") => {
            if let Some(bytes) = event.pointer_mut("/bytes/Ok") {
                bytes_to_base64(bytes);
            }
        }
        _ => {}
    }
}

fn encode_mesh(mesh: &mut Value) {
    for key in F32_RUNS {
        run_to_base64(mesh, key, |n| {
            (n.as_f64().unwrap_or(0.0) as f32).to_le_bytes().to_vec()
        });
    }
    for key in U32_RUNS {
        run_to_base64(mesh, key, |n| {
            (n.as_u64().unwrap_or(0) as u32).to_le_bytes().to_vec()
        });
    }
}

/// `mesh[key]`, an array of triples, as the base64 of its flattened elements.
fn run_to_base64(mesh: &mut Value, key: &str, element: impl Fn(&Value) -> Vec<u8>) {
    let Some(run) = mesh.get_mut(key) else { return };
    let bytes: Vec<u8> = run
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|triple| triple.as_array().into_iter().flatten().flat_map(&element))
        .collect();
    *run = Value::String(B64.encode(bytes));
}

fn bytes_to_base64(value: &mut Value) {
    let bytes: Vec<u8> = value
        .as_array()
        .into_iter()
        .flatten()
        .map(|b| b.as_u64().unwrap_or(0) as u8)
        .collect();
    *value = Value::String(B64.encode(bytes));
}

/// A saved document as `session.save` answers: the JSON files as text and
/// the blobs (`blobs/…`) as base64, as `Opened` carries them.
pub fn saved_files(files: &std::collections::BTreeMap<String, Vec<u8>>) -> Value {
    let (mut text, mut blobs) = (serde_json::Map::new(), serde_json::Map::new());
    for (path, bytes) in files {
        if path.starts_with("blobs/") {
            blobs.insert(path.clone(), Value::String(B64.encode(bytes)));
        } else {
            let content = String::from_utf8_lossy(bytes).into_owned();
            text.insert(path.clone(), Value::String(content));
        }
    }
    json!({"files": text, "blobs": blobs})
}

fn base64_of(what: &str) -> Value {
    json!({"type": "string", "contentEncoding": "base64", "description": what})
}

/// `protocol_schemas()` as `serve` speaks it: the event schema describes the
/// base64 form, and the methods and the result envelope are named.
pub fn wire_schemas() -> Value {
    let mut schemas = arrix_doc::protocol_schemas();
    let event = &mut schemas["event"];
    let f32s = base64_of("base64 of the little-endian f32 elements, three to a vertex");
    let u32s = base64_of("base64 of the little-endian u32 indices, three to a triangle");
    for key in F32_RUNS {
        event["$defs"]["RenderMesh"]["properties"][key] = f32s.clone();
    }
    for key in U32_RUNS {
        event["$defs"]["RenderMesh"]["properties"][key] = u32s.clone();
    }
    let bytes = base64_of("base64 of the bytes");
    event["$defs"]["Result_of_Array_of_uint8_or_Diagnostic"]["oneOf"][0]["properties"]["Ok"] =
        bytes.clone();
    if let Some(variants) = event["oneOf"].as_array_mut() {
        for variant in variants {
            if variant["properties"]["kind"]["const"] == "opened" {
                variant["properties"]["blobs"]["additionalProperties"] = bytes.clone();
            }
        }
    }
    schemas["methods"] = methods();
    schemas["result"] = json!(
        "a request's result is {\"ok\": outcome} or {\"err\": rejected}; session.save's is \
         {\"files\": {path: text}, \"blobs\": {path: base64}}"
    );
    schemas
}

/// Each method and the request it is: its params are that request's fields
/// as an object (`request`'s schema is the enum, `{"<request>": params}`).
pub fn methods() -> Value {
    let mut methods = serde_json::Map::new();
    for (method, request) in REQUESTS {
        methods.insert((*method).to_owned(), json!({"request": request}));
    }
    methods.insert("session.save".into(), json!({"result": "saved"}));
    Value::Object(methods)
}

/// `(method, request variant)`.
pub const REQUESTS: &[(&str, &str)] = &[
    ("session.submit", "submit"),
    ("session.undo", "undo"),
    ("session.redo", "redo"),
    ("session.preview", "preview"),
    ("session.clear_preview", "clear_preview"),
    ("session.export", "export"),
];
