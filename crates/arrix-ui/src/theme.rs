//! The theme as view data, and its mapping onto egui's `Visuals`
//! (docs/UI-RENDERING.md §Configuration). `arrix-app` builds a
//! [`ThemeView`] from the user's theme; what a role colours is decided here.

use serde::{Deserialize, Serialize};

/// An sRGB colour with alpha, as `#rrggbb` or `#rrggbbaa` when it crosses a
/// boundary, so a golden reads like a theme file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgba(pub [u8; 4]);

impl Serialize for Rgba {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let [r, g, b, a] = self.0;
        if a == 255 {
            s.serialize_str(&format!("#{r:02x}{g:02x}{b:02x}"))
        } else {
            s.serialize_str(&format!("#{r:02x}{g:02x}{b:02x}{a:02x}"))
        }
    }
}

impl<'de> Deserialize<'de> for Rgba {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        let digits = text.strip_prefix('#').unwrap_or(&text);
        let byte = |i: usize| u8::from_str_radix(digits.get(i..i + 2).unwrap_or(""), 16);
        let bad = |_| serde::de::Error::custom(format!("`{text}` is not a colour"));
        let alpha = if digits.len() == 8 {
            byte(6).map_err(bad)?
        } else {
            255
        };
        Ok(Self([
            byte(0).map_err(bad)?,
            byte(2).map_err(bad)?,
            byte(4).map_err(bad)?,
            alpha,
        ]))
    }
}

/// The theme: its name, which of egui's two looks it starts from, and one
/// colour per role (`arrix-config`'s `THEME_ROLES`, which says what each
/// colours).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeView {
    pub name: String,
    pub dark: bool,
    pub panel: Rgba,
    pub window: Rgba,
    pub field: Rgba,
    pub text: Rgba,
    pub text_widget: Rgba,
    pub text_hover: Rgba,
    pub text_active: Rgba,
    pub border: Rgba,
    pub button: Rgba,
    pub button_hover: Rgba,
    pub button_active: Rgba,
    pub accent: Rgba,
    pub accent_text: Rgba,
    pub link: Rgba,
    pub warn: Rgba,
    pub error: Rgba,
}

fn color(c: Rgba) -> egui::Color32 {
    let [r, g, b, a] = c.0;
    egui::Color32::from_rgba_unmultiplied(r, g, b, a)
}

impl ThemeView {
    /// The `Visuals` this theme is: egui's dark or light look with each
    /// role's colour laid over it. What isn't the theme's to say, whether
    /// the text cursor blinks (a test harness turns that off), stays as
    /// it is in `current`.
    pub fn visuals(&self, current: &egui::Visuals) -> egui::Visuals {
        let mut v = if self.dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        v.text_cursor.blink = current.text_cursor.blink;
        v.panel_fill = color(self.panel);
        v.window_fill = color(self.window);
        v.window_stroke.color = color(self.border);
        v.extreme_bg_color = color(self.field);
        v.hyperlink_color = color(self.link);
        v.warn_fg_color = color(self.warn);
        v.error_fg_color = color(self.error);
        v.selection.bg_fill = color(self.accent);
        v.selection.stroke.color = color(self.accent_text);
        let w = &mut v.widgets;
        w.noninteractive.bg_stroke.color = color(self.border);
        w.noninteractive.fg_stroke.color = color(self.text);
        w.noninteractive.weak_bg_fill = color(self.panel);
        w.noninteractive.bg_fill = color(self.panel);
        w.inactive.fg_stroke.color = color(self.text_widget);
        w.hovered.fg_stroke.color = color(self.text_hover);
        w.active.fg_stroke.color = color(self.text_active);
        for (state, fill) in [
            (&mut w.inactive, self.button),
            (&mut w.hovered, self.button_hover),
            (&mut w.active, self.button_active),
        ] {
            state.weak_bg_fill = color(fill);
            state.bg_fill = color(fill);
        }
        v
    }
}
