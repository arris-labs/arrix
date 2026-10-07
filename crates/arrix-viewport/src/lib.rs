//! The wgpu scene, ID-buffer picking, overlays and the camera
//! (docs/UI-RENDERING.md §The viewport).

mod camera;
mod gpu;
mod scene;

pub use camera::{
    CameraAnimation, CameraSample, OrbitCamera, Projection, StandardView, ViewOrientation,
    shortest_angular_delta,
};
pub use gpu::{GpuMesh, Renderer};
pub use scene::{Aabb, BodyEntry, BodyKey, BodyPayload, Scene, Upload};
