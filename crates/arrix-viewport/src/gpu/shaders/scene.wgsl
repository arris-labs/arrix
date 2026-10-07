// A body's faces, lit by a light fixed in the world (Z-up), so turning the
// camera turns the shading with the part.

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

// The body's model matrix, at its dynamic offset. Bodies are placed by
// translations, so the normal takes the same matrix.
@group(1) @binding(0) var<uniform> model: mat4x4<f32>;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * model * vec4<f32>(in.position, 1.0);
    out.normal = (model * vec4<f32>(in.normal, 0.0)).xyz;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let light_dir = normalize(vec3<f32>(0.4, -0.6, 0.8));
    let n = normalize(in.normal);
    let diffuse = max(dot(n, light_dir), 0.0);
    let ambient = 0.3;
    let base_color = vec3<f32>(0.55, 0.65, 0.78);
    return vec4<f32>(base_color * (ambient + diffuse * 0.7), 1.0);
}
