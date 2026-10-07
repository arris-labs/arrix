//! The user's theme as the view data the shell is drawn from.

use arrix_config::{Base, Color, Theme};
use arrix_ui::{Rgba, ThemeView};

pub fn view(theme: &Theme) -> ThemeView {
    let c = |role: &str| -> Rgba {
        let Color(rgba) = theme.color(role);
        Rgba(rgba)
    };
    ThemeView {
        name: theme.name.clone(),
        dark: theme.base == Base::Dark,
        panel: c("panel"),
        window: c("window"),
        field: c("field"),
        text: c("text"),
        text_widget: c("text-widget"),
        text_hover: c("text-hover"),
        text_active: c("text-active"),
        border: c("border"),
        button: c("button"),
        button_hover: c("button-hover"),
        button_active: c("button-active"),
        accent: c("accent"),
        accent_text: c("accent-text"),
        link: c("link"),
        warn: c("warn"),
        error: c("error"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrix_config::THEME_ROLES;

    /// The built-in themes are egui's own looks: a theme is a change of
    /// colours, never of what the shell looked like before themes.
    #[test]
    fn the_built_in_themes_are_eguis_own_looks() {
        for (name, own) in [
            ("dark", egui::Visuals::dark()),
            ("light", egui::Visuals::light()),
        ] {
            let theme = Theme::builtin(name).unwrap();
            assert_eq!(view(&theme).visuals(&own), own, "{name}");
        }
    }

    /// Every role of the config crate is one the view takes: `view` reads
    /// each by name, so a role added there and not here fails this.
    #[test]
    fn every_role_is_in_the_view() {
        let theme = Theme::builtin("dark").unwrap();
        let json = serde_json::to_value(view(&theme)).unwrap();
        for (role, _) in THEME_ROLES {
            assert!(json.get(role.replace('-', "_")).is_some(), "{role}");
        }
    }
}
