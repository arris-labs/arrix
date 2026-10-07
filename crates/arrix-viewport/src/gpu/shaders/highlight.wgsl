// Hover and selection, drawn over the scene: a face's triangles in a tint,
// and its outline, an edge or a vertex in the style's colour. The style is
// the draw's first instance (`Style` in highlight.rs). Lines and points are
// a pixel thin, so each is drawn LINE_COPIES times, a pixel apart, which
// makes a cross three pixels wide; the instance number says which copy.
// Everything is moved toward the eye along its own view ray as the edges
// are (edges.wgsl), so it shows over the surface it lies on.

struct Camera {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    up: vec4<f32>,
    right: vec4<f32>,
    forward: vec4<f32>,
    params: vec4<f32>, // (is_ortho, aspect, is_dark, 2 / height in pixels)
};
@group(0) @binding(0) var<uniform> camera: Camera;
struct Body {
    model: mat4x4<f32>,
    pick: vec4<u32>,
};
@group(1) @binding(0) var<uniform> body: Body;

const NUDGE: f32 = 5e-4;
const LINE_COPIES: u32 = 5u;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) @interpolate(flat) color: vec4<f32>,
};

fn style_color(style: u32) -> vec4<f32> {
    if (style == 0u) {
        // Hover: amber.
        return vec4<f32>(1.0, 0.72, 0.2, 0.4);
    }
    // Selection: blue.
    return vec4<f32>(0.2, 0.62, 1.0, 0.55);
}

fn nudged(position: vec3<f32>) -> vec4<f32> {
    let world = (body.model * vec4<f32>(position, 1.0)).xyz;
    let forward = camera.forward.xyz;
    let depth = dot(world - camera.eye.xyz, forward);
    var toward_eye = camera.eye.xyz - world;
    if (camera.params.x > 0.5) {
        toward_eye = -forward * depth;
    }
    return camera.view_proj * vec4<f32>(world + toward_eye * NUDGE, 1.0);
}

@vertex
fn vs_fill(@location(0) position: vec3<f32>, @builtin(instance_index) instance: u32) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = nudged(position);
    out.color = style_color(instance);
    return out;
}

@vertex
fn vs_line(@location(0) position: vec3<f32>, @builtin(instance_index) instance: u32) -> VertexOutput {
    // The centre copy, then a pixel left, right, down and up.
    var offsets = array<vec2<f32>, 5>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(-1.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, -1.0),
        vec2<f32>(0.0, 1.0),
    );
    let copy = instance % LINE_COPIES;
    let pixel = camera.params.w;
    var clip = nudged(position);
    clip.x += offsets[copy].x * pixel / camera.params.y * clip.w;
    clip.y += offsets[copy].y * pixel * clip.w;
    var out: VertexOutput;
    out.clip_position = clip;
    out.color = vec4<f32>(style_color(instance / LINE_COPIES).rgb, 1.0);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
