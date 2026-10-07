//! The command palette's session state and its search over the registry
//! (docs/UI-RENDERING.md §The shell). It is session state and issues no
//! document command: running a result is what does.

use arrix_config::{CommandRegistry, CommandSpec, Keymap};
use arrix_ui::{PaletteEntry, PaletteView};

use crate::commands::{self, Context};

/// How many results the palette shows.
const ROWS: usize = 12;

#[derive(Clone, Debug, Default)]
pub struct Palette {
    pub query: String,
    pub selected: usize,
}

/// The listed commands whose title and id contain every word of `query`,
/// those whose title starts with the first word first, then those whose
/// title has it, then the rest, each group in registry order.
pub fn search<'a>(registry: &'a CommandRegistry, query: &str) -> Vec<&'a CommandSpec> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let rank = |c: &CommandSpec| -> Option<u8> {
        let title = c.title.to_lowercase();
        let id = c.id.to_lowercase();
        if !words
            .iter()
            .all(|w| title.contains(w.as_str()) || id.contains(w.as_str()))
        {
            return None;
        }
        Some(match words.first() {
            None => 0,
            Some(w) if title.starts_with(w.as_str()) => 0,
            Some(w) if title.contains(w.as_str()) => 1,
            Some(_) => 2,
        })
    };
    let mut found: Vec<_> = registry
        .commands
        .iter()
        .filter(|c| c.listed)
        .filter_map(|c| rank(c).map(|r| (r, c)))
        .collect();
    found.sort_by_key(|(r, _)| *r);
    found.into_iter().map(|(_, c)| c).take(ROWS).collect()
}

impl Palette {
    pub fn results<'a>(&self, registry: &'a CommandRegistry) -> Vec<&'a CommandSpec> {
        search(registry, &self.query)
    }

    /// Moves the selection by `delta`, staying on the results.
    pub fn step(&mut self, registry: &CommandRegistry, delta: isize) {
        let last = self.results(registry).len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(delta).min(last);
    }

    pub fn view(&self, registry: &CommandRegistry, keymap: &Keymap, cx: Context) -> PaletteView {
        let results = self.results(registry);
        PaletteView {
            query: self.query.clone(),
            selected: self.selected.min(results.len().saturating_sub(1)),
            results: results
                .into_iter()
                .map(|c| PaletteEntry {
                    id: c.id.clone(),
                    title: c.title.clone(),
                    keys: keymap.keys_of(&c.id),
                    enabled: commands::enabled(c, cx),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> CommandRegistry {
        commands::command_registry(&commands::feature_types())
    }

    fn ids(query: &str) -> Vec<String> {
        search(&registry(), query)
            .iter()
            .map(|c| c.id.clone())
            .collect()
    }

    #[test]
    fn it_searches_title_and_id_and_every_word_must_match() {
        assert_eq!(ids("spur"), ["gears.spur.create"]);
        assert_eq!(ids("GEARS create"), ["gears.spur.create"]);
        assert_eq!(ids("palette"), ["core.palette.open"]);
        assert!(ids("no such command").is_empty());
    }

    #[test]
    fn a_title_that_starts_with_the_word_comes_first() {
        let found = ids("new");
        assert!(found.iter().all(|id| id.ends_with(".create")), "{found:?}");
        let found = ids("tab");
        assert_eq!(found[0], "core.ribbon.sketch", "{found:?}");
    }

    #[test]
    fn the_palettes_own_keys_are_not_listed() {
        assert!(
            ids("palette")
                .iter()
                .all(|id| !id.starts_with("core.palette.n"))
        );
        assert!(!ids("").iter().any(|id| id == "core.palette.run"));
    }

    #[test]
    fn the_selection_stays_on_the_results() {
        let registry = registry();
        let mut p = Palette {
            query: "spur".into(),
            selected: 0,
        };
        p.step(&registry, 5);
        assert_eq!(p.selected, 0);
        p.query.clear();
        p.step(&registry, 2);
        assert_eq!(p.selected, 2);
        p.step(&registry, -9);
        assert_eq!(p.selected, 0);
    }
}
