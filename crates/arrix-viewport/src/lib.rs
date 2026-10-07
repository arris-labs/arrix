//! The wgpu scene, ID-buffer picking, overlays and the camera
//! (docs/UI-RENDERING.md §The viewport).

mod camera;
mod gpu;
mod pick;
mod scene;

pub use camera::{
    CameraAnimation, CameraSample, OrbitCamera, Projection, StandardView, ViewOrientation,
    shortest_angular_delta,
};
pub use gpu::{GpuMesh, Highlights, PendingPick, Renderer};
pub use pick::{
    MAX_PICK_BODIES, MAX_PICK_ENTITIES, PICK_REGION, PickHit, PickRegion, decode as decode_pick,
    encode as encode_pick, resolve_region,
};
pub use scene::{
    Aabb, BodyEntry, BodyKey, BodyPayload, EntityNames, Located, PickBodies, Scene, Upload,
};
