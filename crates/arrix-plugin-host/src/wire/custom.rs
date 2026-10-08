//! The codec of the types whose Rust form checks what it holds or is shaped
//! otherwise than the WIT type (`tests/wire.rs` of `arrix-plugin-api` lists
//! them): a frame, a persistent name, a reference, a profile.
//!
//! Each destructures its Rust type exhaustively, so a field or case added
//! to either side fails to compile here, as in `wit_equality`.

use arrix_core::{
    CurveKey, DVec2, FeatureId, Frame, PersistentName, PluginId, Profile, ProfileLoop,
    ProfileSegment, Ref, RegionKey, SlotName,
};
use serde_json::{Map, Value};

use super::generated::{vec2_from_json, vec2_to_json, vec3_from_json, vec3_to_json};
use super::prim::*;

// ---- frame ----------------------------------------------------------------

pub fn frame_to_json(v: &Frame) -> Wire<Value> {
    let mut m = Map::new();
    put(&mut m, "origin", vec3_to_json(&v.origin()))?;
    put(&mut m, "x-axis", vec3_to_json(&v.x_axis()))?;
    put(&mut m, "z-axis", vec3_to_json(&v.z_axis()))?;
    Ok(Value::Object(m))
}

pub fn frame_from_json(v: &Value) -> Wire<Frame> {
    let m = object(v)?;
    deny_unknown(m, &["origin", "x-axis", "z-axis"])?;
    let origin = field(m, "origin", vec3_from_json)?;
    let x = field(m, "x-axis", vec3_from_json)?;
    let z = field(m, "z-axis", vec3_from_json)?;
    Frame::new(origin, x, z).map_err(|e| WireError::new(e.to_string()))
}

// ---- persistent-ref -------------------------------------------------------

pub fn persistent_ref_to_json(v: &PersistentName) -> Wire<Value> {
    let mut m = Map::new();
    put(&mut m, "encoded", string_to(&v.to_string()))?;
    Ok(Value::Object(m))
}

pub fn persistent_ref_from_json(v: &Value) -> Wire<PersistentName> {
    let m = object(v)?;
    deny_unknown(m, &["encoded"])?;
    field(m, "encoded", |v| {
        newtype_from(v, |s| s.parse::<PersistentName>())
    })
}

// ---- reference ------------------------------------------------------------

fn slot_ref_to_json(feature: &FeatureId, slot: &SlotName) -> Wire<Value> {
    let mut m = Map::new();
    put(&mut m, "feature", id_to(feature))?;
    put(&mut m, "slot", string_to(slot.as_str()))?;
    Ok(Value::Object(m))
}

pub fn reference_to_json(v: &Ref) -> Wire<Value> {
    match v {
        Ref::Param(id) => variant_to("param", id_to(id)),
        Ref::Feature(id) => variant_to("feature", id_to(id)),
        Ref::Slot { feature, slot } => variant_to("slot", slot_ref_to_json(feature, slot)),
        Ref::Topo(name) => variant_to("topo", persistent_ref_to_json(name)),
        Ref::Sketch { feature, entity } => {
            let mut m = Map::new();
            put(&mut m, "feature", id_to(feature))?;
            put(&mut m, "entity", id_to(entity))?;
            variant_to("sketch", Ok(Value::Object(m)))
        }
        Ref::Region {
            feature,
            key: RegionKey { entities, sample },
        } => {
            let mut m = Map::new();
            put(&mut m, "feature", id_to(feature))?;
            put(&mut m, "entities", list_to(entities, id_to))?;
            put(&mut m, "sample", array_to(sample, s64_to))?;
            variant_to("region", Ok(Value::Object(m)))
        }
        Ref::Plugin { plugin, record } => {
            let mut m = Map::new();
            put(&mut m, "plugin", string_to(plugin.as_str()))?;
            put(&mut m, "record", id_to(record))?;
            variant_to("plugin", Ok(Value::Object(m)))
        }
    }
}

pub fn reference_from_json(v: &Value) -> Wire<Ref> {
    let (case, payload) = variant(v)?;
    let decoded = match case {
        "param" => id_from(payload).map(Ref::Param),
        "feature" => id_from(payload).map(Ref::Feature),
        "slot" => slot_ref_from_json(payload),
        "topo" => persistent_ref_from_json(payload).map(Ref::Topo),
        "sketch" => sketch_ref_from_json(payload),
        "region" => region_ref_from_json(payload),
        "plugin" => plugin_ref_from_json(payload),
        other => {
            return Err(unknown_case(
                "reference",
                other,
                &[
                    "param", "feature", "slot", "topo", "sketch", "region", "plugin",
                ],
            ));
        }
    };
    variant_payload(case, decoded)
}

fn sketch_ref_from_json(v: &Value) -> Wire<Ref> {
    let m = object(v)?;
    deny_unknown(m, &["feature", "entity"])?;
    Ok(Ref::Sketch {
        feature: field(m, "feature", id_from)?,
        entity: field(m, "entity", id_from)?,
    })
}

fn region_ref_from_json(v: &Value) -> Wire<Ref> {
    let m = object(v)?;
    deny_unknown(m, &["feature", "entities", "sample"])?;
    Ok(Ref::Region {
        feature: field(m, "feature", id_from)?,
        key: RegionKey {
            entities: field(m, "entities", |v| list_from(v, id_from))?,
            sample: field(m, "sample", |v| array_from::<_, 2>(v, s64_from))?,
        },
    })
}

fn plugin_ref_from_json(v: &Value) -> Wire<Ref> {
    let m = object(v)?;
    deny_unknown(m, &["plugin", "record"])?;
    Ok(Ref::Plugin {
        plugin: field(m, "plugin", |v| newtype_from(v, PluginId::new))?,
        record: field(m, "record", id_from)?,
    })
}

fn slot_ref_from_json(v: &Value) -> Wire<Ref> {
    let m = object(v)?;
    deny_unknown(m, &["feature", "slot"])?;
    Ok(Ref::Slot {
        feature: field(m, "feature", id_from)?,
        slot: field(m, "slot", |v| newtype_from(v, SlotName::new))?,
    })
}

// ---- profile --------------------------------------------------------------

fn line_to_json(key: &CurveKey, to: &DVec2) -> Wire<Value> {
    let mut m = Map::new();
    put(&mut m, "key", id_to(key))?;
    put(&mut m, "to", vec2_to_json(to))?;
    Ok(Value::Object(m))
}

pub fn profile_segment_to_json(v: &ProfileSegment) -> Wire<Value> {
    match v {
        ProfileSegment::Line { key, to } => variant_to("line", line_to_json(key, to)),
        ProfileSegment::Arc { key, to, via } => {
            let mut m = Map::new();
            put(&mut m, "key", id_to(key))?;
            put(&mut m, "to", vec2_to_json(to))?;
            put(&mut m, "via", vec2_to_json(via))?;
            variant_to("arc", Ok(Value::Object(m)))
        }
    }
}

fn line_from_json(v: &Value) -> Wire<ProfileSegment> {
    let m = object(v)?;
    deny_unknown(m, &["key", "to"])?;
    Ok(ProfileSegment::Line {
        key: field(m, "key", id_from)?,
        to: field(m, "to", vec2_from_json)?,
    })
}

fn arc_from_json(v: &Value) -> Wire<ProfileSegment> {
    let m = object(v)?;
    deny_unknown(m, &["key", "to", "via"])?;
    Ok(ProfileSegment::Arc {
        key: field(m, "key", id_from)?,
        to: field(m, "to", vec2_from_json)?,
        via: field(m, "via", vec2_from_json)?,
    })
}

pub fn profile_segment_from_json(v: &Value) -> Wire<ProfileSegment> {
    let (case, payload) = variant(v)?;
    let decoded = match case {
        "line" => line_from_json(payload),
        "arc" => arc_from_json(payload),
        other => return Err(unknown_case("profile-segment", other, &["line", "arc"])),
    };
    variant_payload(case, decoded)
}

pub fn profile_loop_to_json(v: &ProfileLoop) -> Wire<Value> {
    match v {
        ProfileLoop::Circle {
            key,
            center,
            radius,
        } => {
            let mut m = Map::new();
            put(&mut m, "key", id_to(key))?;
            put(&mut m, "center", vec2_to_json(center))?;
            put(&mut m, "radius", f64_to(radius))?;
            variant_to("circle", Ok(Value::Object(m)))
        }
        ProfileLoop::Path { start, segments } => {
            let mut m = Map::new();
            put(&mut m, "start", vec2_to_json(start))?;
            put(
                &mut m,
                "segments",
                list_to(segments, profile_segment_to_json),
            )?;
            variant_to("path", Ok(Value::Object(m)))
        }
    }
}

fn circle_from_json(v: &Value) -> Wire<ProfileLoop> {
    let m = object(v)?;
    deny_unknown(m, &["key", "center", "radius"])?;
    Ok(ProfileLoop::Circle {
        key: field(m, "key", id_from)?,
        center: field(m, "center", vec2_from_json)?,
        radius: field(m, "radius", f64_from)?,
    })
}

fn path_from_json(v: &Value) -> Wire<ProfileLoop> {
    let m = object(v)?;
    deny_unknown(m, &["start", "segments"])?;
    Ok(ProfileLoop::Path {
        start: field(m, "start", vec2_from_json)?,
        segments: field(m, "segments", |v| list_from(v, profile_segment_from_json))?,
    })
}

pub fn profile_loop_from_json(v: &Value) -> Wire<ProfileLoop> {
    let (case, payload) = variant(v)?;
    let decoded = match case {
        "circle" => circle_from_json(payload),
        "path" => path_from_json(payload),
        other => return Err(unknown_case("profile-loop", other, &["circle", "path"])),
    };
    variant_payload(case, decoded)
}

pub fn profile_to_json(v: &Profile) -> Wire<Value> {
    let mut m = Map::new();
    put(&mut m, "plane", frame_to_json(v.plane()))?;
    put(&mut m, "outer", profile_loop_to_json(v.outer()))?;
    put(&mut m, "holes", list_to(v.holes(), profile_loop_to_json))?;
    Ok(Value::Object(m))
}

pub fn profile_from_json(v: &Value) -> Wire<Profile> {
    let m = object(v)?;
    deny_unknown(m, &["plane", "outer", "holes"])?;
    let plane = field(m, "plane", frame_from_json)?;
    let outer = field(m, "outer", profile_loop_from_json)?;
    let holes = field(m, "holes", |v| list_from(v, profile_loop_from_json))?;
    Profile::new(plane, outer, holes).map_err(|e| WireError::new(e.to_string()))
}
