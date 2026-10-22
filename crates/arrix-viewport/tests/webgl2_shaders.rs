//! The viewport's shaders on WebGL2 (plans/c4 step 9, docs/UI-RENDERING.md
//! §The web entry). In a browser without WebGPU, as headless Chromium
//! with `--disable-gpu` is, wgpu draws through WebGL2 and translates each
//! WGSL shader to GLSL ES 3.0 with `naga` when a pipeline is made. This
//! does the same translation natively, for every shader under
//! `src/gpu/shaders` and every entry point, so a shader that only WebGPU
//! takes fails here rather than in a page nobody opened.
#![cfg(not(target_arch = "wasm32"))]

use std::fs;
use std::path::Path;

use naga::back::glsl;
use naga::valid::{Capabilities, ValidationFlags, Validator};

/// GLSL ES 3.0 for WebGL, with the flags wgpu's GL backend always sets.
fn webgl2_options() -> glsl::Options {
    glsl::Options {
        version: glsl::Version::Embedded {
            version: 300,
            is_webgl: true,
        },
        writer_flags: glsl::WriterFlags::ADJUST_COORDINATE_SPACE
            | glsl::WriterFlags::FORCE_POINT_SIZE,
        ..glsl::Options::default()
    }
}

/// Every entry point of the WGSL `source`, as GLSL ES 3.0: the errors.
fn translate(source: &str) -> Vec<String> {
    let module = match naga::front::wgsl::parse_str(source) {
        Ok(module) => module,
        Err(e) => return vec![format!("parse: {}", e.emit_to_string(source))],
    };
    let info = match Validator::new(ValidationFlags::all(), Capabilities::empty()).validate(&module)
    {
        Ok(info) => info,
        Err(e) => return vec![format!("validate: {}", e.emit_to_string(source))],
    };
    let options = webgl2_options();
    module
        .entry_points
        .iter()
        .filter_map(|entry| {
            let pipeline = glsl::PipelineOptions {
                shader_stage: entry.stage,
                entry_point: entry.name.clone(),
                multiview: None,
            };
            let mut out = String::new();
            glsl::Writer::new(
                &mut out,
                &module,
                &info,
                &options,
                &pipeline,
                naga::proc::BoundsCheckPolicies::default(),
            )
            .and_then(|mut w| w.write())
            .err()
            .map(|e| format!("{}: {e}", entry.name))
        })
        .collect()
}

#[test]
fn every_viewport_shader_translates_to_glsl_es_300() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/gpu/shaders");
    let mut shaders: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "wgsl"))
        .collect();
    shaders.sort();
    assert!(shaders.len() >= 6, "the shaders moved: {shaders:?}");
    let failures: Vec<String> = shaders
        .iter()
        .flat_map(|path| {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            translate(&fs::read_to_string(path).unwrap())
                .into_iter()
                .map(move |e| format!("{name}: {e}"))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_shader_webgl2_cannot_take_fails() {
    // A storage buffer: WebGL2 has none.
    let source = "@group(0) @binding(0) var<storage, read_write> data: array<u32>;\n\
                  @compute @workgroup_size(1) fn main() { data[0] = 1u; }";
    assert!(!translate(source).is_empty());
}
