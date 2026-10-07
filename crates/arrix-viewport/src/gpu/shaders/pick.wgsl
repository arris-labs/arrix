// The pick pass: each face, edge and vertex draws its id into an R32Uint
// target instead of a colour (`crate::pick`). The vertex data carries the
// kind and the entity's index; the body's uniform carries its index among
// the visible bodies, ORed in. Edges and vertices are moved toward the eye
// exactly as the edges are drawn (edges.wgsl), so what wins a pixel here is
// what shows there.

struct Camera {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    up: vec4<f32>,
    right: vec4<f32>,
    forward: vec4<f32>,
    params: vec4<f32>, // (is_ortho, aspect, is_dark, 0)
};
@group(0) @binding(0) var<uniform> camera: Camera;

struct Body {
    model: mat4x4<f32>,
    pick: vec4<u32>, // (body bits, 0, 0, 0)
};
@group(1) @binding(0) var<uniform> body: Body;

const NUDGE: f32 = 5e-4;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) @interpolate(flat) id: u32,
};

// An entity past what an id holds draws as 0, nothing, and stays so.
fn pick_id(entity: u32) -> u32 {
    return select(0u, entity | body.pick.x, entity != 0u);
}

@vertex
fn vs_surface(@location(0) position: vec3<f32>, @location(1) entity: u32) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * body.model * vec4<f32>(position, 1.0);
    out.id = pick_id(entity);
    return out;
}

@vertex
fn vs_line(@location(0) position: vec3<f32>, @location(1) entity: u32) -> VertexOutput {
    let world = (body.model * vec4<f32>(position, 1.0)).xyz;
    let forward = camera.forward.xyz;
    let depth = dot(world - camera.eye.xyz, forward);
    var toward_eye = camera.eye.xyz - world;
    if (camera.params.x > 0.5) {
        toward_eye = -forward * depth;
    }
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(world + toward_eye * NUDGE, 1.0);
    out.id = pick_id(entity);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) u32 {
    return in.id;
}
