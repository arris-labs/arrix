//! Turntable orbit camera, canonical orientations and view transitions,
//! Z-up and right-handed (SEED.md §8.2).

pub mod animation;
pub mod orbit;
pub mod orientation;

#[cfg(test)]
mod tests;

pub use animation::{CameraAnimation, CameraSample, shortest_angular_delta};
pub use orbit::OrbitCamera;
pub use orientation::{Projection, StandardView, ViewOrientation};
