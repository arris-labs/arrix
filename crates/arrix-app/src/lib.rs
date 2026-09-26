//! The application: the shell, the command registry, and the translation
//! between document and view types (docs/UI-RENDERING.md §The shell).

/// The application state eframe drives.
#[derive(Default)]
pub struct ArrixApp;

impl eframe::App for ArrixApp {
    fn ui(&mut self, _ui: &mut egui::Ui, _frame: &mut eframe::Frame) {}
}
