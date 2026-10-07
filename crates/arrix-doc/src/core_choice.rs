//! Declaring the built-in types' choices and where their fields show
//! (ADR-0018): shorthand for the spec literals.

use arrix_plugin_api::{ChoiceOption, ChoiceSpec, ShownWhen};

/// A choice of `options` (word, title), holding `default` when a record
/// gives none.
pub(crate) fn choice(
    name: &str,
    title: &str,
    options: &[(&str, &str)],
    default: Option<&str>,
) -> ChoiceSpec {
    ChoiceSpec {
        name: name.into(),
        title: title.into(),
        options: options
            .iter()
            .map(|(name, title)| ChoiceOption {
                name: (*name).into(),
                title: (*title).into(),
            })
            .collect(),
        default: default.map(Into::into),
    }
}

/// Shown while `choice` holds one of `values`.
pub(crate) fn shown_when(choice: &str, values: &[&str]) -> Option<ShownWhen> {
    Some(ShownWhen {
        choice: choice.into(),
        values: values.iter().map(|v| (*v).into()).collect(),
    })
}
