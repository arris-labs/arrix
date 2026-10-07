//! egui widgets over plain view types, and the declarative-UI renderer. Never
//! names a document or sketch type (docs/UI-RENDERING.md §View types).

mod palette;
mod shell;
mod theme;
pub mod view;
pub mod viewcube;

pub use shell::{ShellLayout, ShellResponse, shell, viewcube_rect};
pub use theme::{Rgba, ThemeView};
pub use view::{
    CardsView, DocumentTab, DocumentTabsView, FeatureState, PaletteEntry, PaletteView,
    RibbonCommand, RibbonGroup, RibbonTab, RibbonView, ShellView, StatusView, TreeFeature,
    TreePart, TreeView, UiIntent,
};
