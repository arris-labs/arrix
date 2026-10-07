//! egui widgets over plain view types, and the declarative-UI renderer. Never
//! names a document or sketch type (docs/UI-RENDERING.md §View types).

mod palette;
mod shell;
mod sketch;
mod sketch_field;
mod sketch_preview;
mod sketch_regions;
mod theme;
pub mod view;
pub mod viewcube;

pub use shell::{ShellLayout, ShellResponse, shell, viewcube_rect};
pub use sketch::{
    ConstraintGlyphView, CurveShape, DimensionView, DofState, HitKind, PlaneToScreen,
    SketchCurveView, SketchHit, SketchPointView, SketchStatusView, SketchView, SolverState,
    hit_test, paint_sketch,
};
pub use sketch_field::{DimensionFieldView, FieldEvent, dimension_field};
pub use sketch_preview::{SketchPreviewView, SnapGlyphView};
pub use sketch_regions::SketchRegionView;
pub use theme::{Rgba, ThemeView};
pub use view::{
    CardsView, DocumentTab, DocumentTabsView, FeatureState, PaletteEntry, PaletteView,
    PlanePointer, RibbonCommand, RibbonGroup, RibbonTab, RibbonView, ShellView, StatusView,
    TreeFeature, TreePart, TreeView, UiIntent,
};
