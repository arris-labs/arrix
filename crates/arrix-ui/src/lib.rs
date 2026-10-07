//! egui widgets over plain view types, and the declarative-UI renderer. Never
//! names a document or sketch type (docs/UI-RENDERING.md §View types).

mod shell;
pub mod view;

pub use shell::{ShellLayout, ShellResponse, shell};
pub use view::{
    RibbonCommand, RibbonGroup, RibbonTab, RibbonView, ShellView, StatusView, TreePart, TreeView,
    UiIntent,
};
