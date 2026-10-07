// A body's edges, as lines over its faces. A line lies on the faces it
// bounds, and Vulkan's depth bias applies to polygons only, so each point is
// moved toward the camera along its own view ray, which leaves it where it
// was on screen, by a fraction of its depth. A fixed offset in clip depth
// would not do: perspective depth is so uneven that one value is microns
// near the camera and decimetres a few metres out, and lets hidden edges
// poke out past the corners that hide them.

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
    pick: vec4<u32>,
};
@group(1) @binding(0) var<uniform> body: Body;

// Of the point's depth: 0.5 mm at a metre.
const NUDGE: f32 = 5e-4;

@vertex
fn vs_main(@location(0) position: vec3<f32>) -> @builtin(position) vec4<f32> {
    let world = (body.model * vec4<f32>(position, 1.0)).xyz;
    let forward = camera.forward.xyz;
    let depth = dot(world - camera.eye.xyz, forward);
    // Orthographic rays are parallel to `forward`; perspective ones meet
    // at the eye.
    var toward_eye = camera.eye.xyz - world;
    if (camera.params.x > 0.5) {
        toward_eye = -forward * depth;
    }
    return camera.view_proj * vec4<f32>(world + toward_eye * NUDGE, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return vec4<f32>(0.08, 0.09, 0.11, 1.0);
}
