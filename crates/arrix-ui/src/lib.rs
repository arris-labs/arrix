//! egui widgets over plain view types, and the declarative-UI renderer. Never
//! names a document or sketch type (docs/UI-RENDERING.md §View types).

mod palette;
mod shell;
mod theme;
pub mod view;

pub use shell::{ShellLayout, ShellResponse, shell};
pub use theme::{Rgba, ThemeView};
pub use view::{
    CardsView, DocumentTab, DocumentTabsView, PaletteEntry, PaletteView, RibbonCommand,
    RibbonGroup, RibbonTab, RibbonView, ShellView, StatusView, TreePart, TreeView, UiIntent,
};
