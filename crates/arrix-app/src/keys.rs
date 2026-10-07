//! Keyboard input as commands: egui key events become [`Chord`]s, which the
//! keymap resolves to a command id in the contexts of the moment
//! (docs/UI-RENDERING.md §The shell).

use arrix_config::Chord;

/// The chord of a key press; a key's `name()`, lower-cased, is its keymap
/// name.
pub fn chord(key: egui::Key, modifiers: egui::Modifiers) -> Chord {
    Chord {
        ctrl: modifiers.ctrl,
        shift: modifiers.shift,
        alt: modifiers.alt,
        super_: modifiers.mac_cmd,
        key: key.name().to_lowercase(),
    }
}

/// The chords pressed this frame, in order. A held key's repeats do not
/// count: one press is one gesture.
pub fn pressed(ctx: &egui::Context) -> Vec<Chord> {
    ctx.input(|i| {
        i.events
            .iter()
            .filter_map(|event| match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } => Some(chord(*key, *modifiers)),
                _ => None,
            })
            .collect()
    })
}

/// Removes the presses of `chord` from this frame's input, so a widget drawn
/// later doesn't act on a key the keymap already took.
pub fn consume(ctx: &egui::Context, chord: &Chord) {
    ctx.input_mut(|i| {
        i.events.retain(|event| match event {
            egui::Event::Key { key, modifiers, .. } => self::chord(*key, *modifiers) != *chord,
            _ => true,
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_press_names_its_key_like_the_keymap_does() {
        let ctrl = egui::Modifiers::CTRL;
        assert_eq!(chord(egui::Key::K, ctrl), Chord::parse("ctrl+k").unwrap());
        assert_eq!(
            chord(egui::Key::ArrowUp, egui::Modifiers::SHIFT),
            Chord::parse("shift+up").unwrap()
        );
        assert_eq!(
            chord(egui::Key::Num2, egui::Modifiers::NONE),
            Chord::parse("2").unwrap()
        );
        assert_eq!(
            chord(egui::Key::F12, ctrl),
            Chord::parse("ctrl+f12").unwrap()
        );
    }

    #[test]
    fn every_key_the_keymap_accepts_is_one_egui_can_press() {
        // The names the keymap accepts, as egui spells them.
        for name in [
            "a",
            "z",
            "0",
            "9",
            "f1",
            "f12",
            "enter",
            "escape",
            "tab",
            "space",
            "backspace",
            "delete",
            "insert",
            "home",
            "end",
            "pageup",
            "pagedown",
            "up",
            "down",
            "left",
            "right",
        ] {
            assert!(
                egui::Key::ALL
                    .iter()
                    .any(|k| k.name().to_lowercase() == name),
                "egui has no key `{name}`"
            );
        }
    }
}
