//! The host's side of the `kernel` interface on the wire: a plugin's
//! `kernel.*` request decoded, made on the evaluating feature's kernel, and
//! its result encoded. A kernel failure is the result's `err`; only a
//! request the world does not have, or params not of its shape, is the
//! protocol's error (ADR-0022 §4).

use arrix_core::PartId;
use arrix_plugin_api::{Body, Diagnostic, Kernel, Snapshot};
use serde_json::{Map, Value};

use super::process::{CallError, Served};
use crate::wire::rpc::{Request, RpcError};
use crate::wire::*;

/// Answers one request of the plugin's on `kernel`. A plugin without the
/// manifest's `kernel` capability is refused, and the call ends; so does
/// one whose call the evaluator's interrupt stopped, which no plugin is to
/// see answered and carry on from.
pub(crate) fn kernel_call(kernel: &mut dyn Kernel, allowed: bool, request: &Request) -> Served {
    if !allowed {
        return Served::End(CallError::Capability(format!(
            "it called `{}`, and its manifest does not grant `kernel`",
            request.method
        )));
    }
    let answered = method(&request.method, Direction::PluginToHost).and_then(|method| {
        let params = object(&request.params).map_err(|e| RpcError::invalid_params(&e))?;
        call(kernel, method.name, params).map_err(|e| RpcError::invalid_params(&e))
    });
    if let Ok(v) = &answered
        && let Ok(Err(d)) = result_from(v, |_| Ok(()))
        && d.code.as_str() == "kernel.interrupted"
    {
        return Served::End(CallError::Interrupted(d));
    }
    Served::Answer(answered)
}

/// Answers one `document.*` request of an exporter's on the export's
/// snapshot (ADR-0037 §5). A plugin whose manifest does not grant
/// `document-read`, or one that asks for anything but the `document`
/// interface (a kernel call during an export), is refused and the call ends.
pub(crate) fn document_call(
    snapshot: &mut dyn Snapshot,
    allowed: bool,
    request: &Request,
) -> Served {
    if !allowed {
        return Served::End(CallError::Capability(format!(
            "it called `{}`, and its manifest does not grant `document-read`",
            request.method
        )));
    }
    if !request.method.starts_with("document.") {
        return Served::End(CallError::Capability(format!(
            "it called `{}` during an export, which reads the document only",
            request.method
        )));
    }
    let answered = method(&request.method, Direction::PluginToHost).and_then(|method| {
        let params = object(&request.params).map_err(|e| RpcError::invalid_params(&e))?;
        document(snapshot, method.name, params).map_err(|e| RpcError::invalid_params(&e))
    });
    Served::Answer(answered)
}

fn document(snapshot: &mut dyn Snapshot, method: &str, p: &Map<String, Value>) -> Wire<Value> {
    match method {
        "document.parts" => {
            deny_unknown(p, &[])?;
            list_to(&snapshot.parts(), part_info_to_json)
        }
        "document.mesh" => {
            deny_unknown(p, &["part", "body"])?;
            let part = field(p, "part", id_from::<PartId>)?;
            let body = field(p, "body", body_ref_from_json)?;
            answer(snapshot.mesh(part, &body), triangles_to_json)
        }
        "document.mass" => {
            deny_unknown(p, &["part"])?;
            let part = field(p, "part", id_from::<PartId>)?;
            answer(snapshot.mass(part), inertial_to_json)
        }
        "document.assemblies" => {
            deny_unknown(p, &[])?;
            list_to(&snapshot.assemblies(), assembly_info_to_json)
        }
        other => unreachable!("`{other}` is a document method of the table this match covers"),
    }
}

fn call(kernel: &mut dyn Kernel, method: &str, p: &Map<String, Value>) -> Wire<Value> {
    let body = |name: &str| field(p, name, handle_from).map(Body::from_handle);
    let names = |name: &str| field(p, name, |v| list_from(v, persistent_ref_from_json));
    let num = |name: &str| field(p, name, f64_from);
    match method {
        "kernel.extrude" => {
            deny_unknown(p, &["p", "distance"])?;
            let profile = field(p, "p", profile_from_json)?;
            answer(kernel.extrude(&profile, num("distance")?), handle_to)
        }
        "kernel.revolve" => {
            deny_unknown(p, &["p", "a", "angle"])?;
            let profile = field(p, "p", profile_from_json)?;
            let axis = field(p, "a", axis_from_json)?;
            answer(kernel.revolve(&profile, &axis, num("angle")?), handle_to)
        }
        "kernel.names" => {
            deny_unknown(p, &["b"])?;
            answer(kernel.names(&body("b")?), |names| {
                list_to(names, persistent_ref_to_json)
            })
        }
        "kernel.face-frame" => {
            deny_unknown(p, &["b", "face"])?;
            let face = field(p, "face", persistent_ref_from_json)?;
            answer(kernel.face_frame(&body("b")?, &face), frame_to_json)
        }
        "kernel.measure" => {
            deny_unknown(p, &["b"])?;
            answer(kernel.measure(&body("b")?), mass_properties_to_json)
        }
        "kernel.fuse" | "kernel.cut" | "kernel.common" => {
            deny_unknown(p, &["target", "tool"])?;
            let (target, tool) = (body("target")?, body("tool")?);
            let made = match method {
                "kernel.fuse" => kernel.fuse(&target, &tool),
                "kernel.cut" => kernel.cut(&target, &tool),
                _ => kernel.common(&target, &tool),
            };
            answer(made, handle_to)
        }
        "kernel.fillet" => {
            deny_unknown(p, &["b", "edges", "radius"])?;
            let made = kernel.fillet(&body("b")?, &names("edges")?, num("radius")?);
            answer(made, handle_to)
        }
        "kernel.chamfer" => {
            deny_unknown(p, &["b", "edges", "distance"])?;
            let made = kernel.chamfer(&body("b")?, &names("edges")?, num("distance")?);
            answer(made, handle_to)
        }
        "kernel.copy" => {
            deny_unknown(p, &["index", "b", "m"])?;
            let index = field(p, "index", u32_from)?;
            let motion = field(p, "m", motion_from_json)?;
            answer(kernel.copy(index, &body("b")?, &motion), handle_to)
        }
        "kernel.mirror" => {
            deny_unknown(p, &["b", "origin", "normal"])?;
            let origin = field(p, "origin", vec3_from_json)?;
            let normal = field(p, "normal", vec3_from_json)?;
            answer(kernel.mirror(&body("b")?, origin, normal), handle_to)
        }
        other => unreachable!("`{other}` is a kernel method of the table this match covers"),
    }
}

/// A WIT `result` as the wire carries it: `{"ok": …}` or `{"err": …}`.
pub(crate) fn answer<T>(
    made: Result<T, Diagnostic>,
    ok: impl FnOnce(&T) -> Wire<Value>,
) -> Wire<Value> {
    match made {
        Ok(v) => variant_to("ok", ok(&v)),
        Err(d) => variant_to("err", diagnostic_to_json(&d)),
    }
}

/// The other way: the `ok` decoded, or the `err`'s diagnostic.
pub(crate) fn result_from<T>(
    v: &Value,
    ok: impl FnOnce(&Value) -> Wire<T>,
) -> Wire<Result<T, Diagnostic>> {
    let (case, payload) = variant(v)?;
    match case {
        "ok" => variant_payload(case, ok(payload)).map(Ok),
        "err" => variant_payload(case, diagnostic_from_json(payload)).map(Err),
        other => Err(unknown_case("result", other, &["ok", "err"])),
    }
}
