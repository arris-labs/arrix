//! The ViewCube: the chamfered cube's geometry, its projection, and the
//! widget that paints it in the viewport's top-right corner.
//!
//! The cube is interactive chrome that *asks* for camera changes: it reads
//! the pointer over its own rect and returns a [`ViewCubeAction`], where
//! the app owns the camera and applies it. It is painter-drawn rather than
//! a wgpu pass: picking a facet needs a screen-space hit test regardless.
//! It reads the viewport's `ViewOrientation` and `Projection` (the layer
//! rule lets `arrix-ui` depend on `arrix-viewport`) and takes the camera's
//! orientation as plain angles.

pub mod arrows;
pub mod axes;
pub mod facets;
pub mod projection;
pub mod widget;

#[cfg(test)]
mod tests;

pub use projection::project_viewcube;
pub use widget::{ViewCubeAction, viewcube, viewcube_block, viewcube_rect_in};
