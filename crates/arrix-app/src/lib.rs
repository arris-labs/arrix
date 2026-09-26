//! The application: the shell, the command registry, and the translation
//! between document and view types (docs/UI-RENDERING.md §The shell).

mod debug;

pub use debug::DebugState;

use arrix_ui::{RibbonTab, RibbonView, ShellLayout, ShellView, StatusView, TreeView, UiIntent};

/// Client-side state that is not in the document (SEED.md §6.1 rule 4).
struct Session {
    ribbon_tab: String,
}

/// The application state eframe drives.
pub struct ArrixApp {
    session: Session,
    /// What the last frame drew, for [`ArrixApp::debug_state`].
    drawn: Option<Drawn>,
}

struct Drawn {
    window: egui::Rect,
    view: ShellView,
    layout: ShellLayout,
}

impl ArrixApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            session: Session {
                ribbon_tab: "core.sketch".into(),
            },
            drawn: None,
        }
    }

    /// The view the shell is drawn from: a projection of the session (and,
    /// from M1, of the document and its events).
    fn shell_view(&self) -> ShellView {
        let tab = |id: &str, title: &str| RibbonTab {
            id: id.into(),
            title: title.into(),
        };
        ShellView {
            ribbon: RibbonView {
                tabs: vec![tab("core.sketch", "Sketch"), tab("core.part", "Part")],
                active: self.session.ribbon_tab.clone(),
            },
            tree: TreeView::default(),
            status: StatusView {
                generation: 0,
                diagnostics: 0,
                length_unit: "mm".into(),
                angle_unit: "deg".into(),
            },
        }
    }

    fn apply(&mut self, intent: UiIntent) {
        match intent {
            UiIntent::RibbonTabSelected { tab } => self.session.ribbon_tab = tab,
        }
    }
}

impl eframe::App for ArrixApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let window = ui.max_rect();
        let view = self.shell_view();
        let response = arrix_ui::shell(ui, &view, |_ui| {});
        self.drawn = Some(Drawn {
            window,
            view,
            layout: response.layout,
        });
        for intent in response.intents {
            self.apply(intent);
        }
    }
}
