//! The user's defaults for feature parameters: `[defaults."<type>"]` in
//! `settings.toml`. A default only seeds the form of a new feature; once
//! committed, the value is the document's, and it never reaches a
//! feature's `evaluate` (ADR-0015, `SEED.md` §6.3). The type's own
//! default stays under it, so a bad entry is one diagnostic and falls back.

use std::collections::BTreeMap;

use arrix_core::{Diagnostic, QuantityKind};
use arrix_plugin_api::FeatureTypeSpec;
use toml::de::DeValue;

use crate::spanned::{File, in_file_order, read_text, type_name};
use crate::{ConfigDiagnostic, ConfigSource, SETTINGS_TOML};

/// Checks that an expression is a constant of a quantity kind. `arrix-doc`
/// owns the expression language and this crate does not depend on it, so
/// the caller hands its checker in (`arrix_doc::expr::check_constant`).
pub type ExprCheck<'a> = &'a dyn Fn(&str, QuantityKind) -> Result<(), Diagnostic>;

/// The user's defaults, by feature type and parameter name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParamDefaults {
    by_type: BTreeMap<String, BTreeMap<String, String>>,
}

impl ParamDefaults {
    /// The defaults `settings.toml` in `source` sets, checked against
    /// `types`.
    pub fn load(
        source: &impl ConfigSource,
        types: &[FeatureTypeSpec],
        check: ExprCheck<'_>,
    ) -> (Self, Vec<ConfigDiagnostic>) {
        let mut diagnostics = Vec::new();
        let Some(text) = read_text(source, SETTINGS_TOML, &mut diagnostics) else {
            return (Self::default(), Vec::new());
        };
        // The file's own problems are `Settings`'; only these are ours.
        let defaults = Self::apply(SETTINGS_TOML, &text, types, check, &mut diagnostics);
        in_file_order(&mut diagnostics);
        (defaults, diagnostics)
    }

    fn apply(
        path: &str,
        text: &str,
        types: &[FeatureTypeSpec],
        check: ExprCheck<'_>,
        diagnostics: &mut Vec<ConfigDiagnostic>,
    ) -> Self {
        let file = File { path, text };
        let parsed = file.parse();
        let mut out = Self::default();
        let Some((_, section)) = parsed
            .table
            .get_ref()
            .iter()
            .find(|(k, _)| k.get_ref() == "defaults")
        else {
            return out;
        };
        let DeValue::Table(by_type) = section.get_ref() else {
            diagnostics.push(file.error(
                section.span(),
                "config.type",
                format!(
                    "`defaults` is a table of feature types, not {}",
                    type_name(section.get_ref())
                ),
            ));
            return out;
        };
        for (type_key, params) in by_type.iter() {
            let type_id = type_key.get_ref().as_ref();
            let Some(spec) = types.iter().find(|t| t.id == type_id) else {
                diagnostics.push(file.error(
                    type_key.span(),
                    "config.unknown-feature-type",
                    format!("`{type_id}` is not a feature type"),
                ));
                continue;
            };
            let DeValue::Table(params) = params.get_ref() else {
                diagnostics.push(file.error(
                    params.span(),
                    "config.type",
                    format!(
                        "`defaults.\"{type_id}\"` is a table of parameters, not {}",
                        type_name(params.get_ref())
                    ),
                ));
                continue;
            };
            for (name_key, value) in params.iter() {
                if parsed.is_broken(name_key, value) {
                    continue;
                }
                let name = name_key.get_ref().as_ref();
                let Some(param) = spec.params.iter().find(|p| p.name == name) else {
                    let known: Vec<_> = spec.params.iter().map(|p| p.name.as_str()).collect();
                    diagnostics.push(file.error(
                        name_key.span(),
                        "config.unknown-key",
                        format!(
                            "`{name}` is not a parameter of `{type_id}` ({})",
                            known.join(", ")
                        ),
                    ));
                    continue;
                };
                let DeValue::String(expr) = value.get_ref() else {
                    diagnostics.push(file.error(
                        value.span(),
                        "config.type",
                        format!(
                            "`{name}` is an expression in a string, not {}",
                            type_name(value.get_ref())
                        ),
                    ));
                    continue;
                };
                match check(expr, param.kind) {
                    Ok(()) => {
                        out.by_type
                            .entry(type_id.to_owned())
                            .or_default()
                            .insert(name.to_owned(), expr.to_string());
                    }
                    Err(d) => diagnostics.push(file.error(
                        value.span(),
                        // `expr.syntax` is `config.expr.syntax`: the
                        // expression's own code, kept.
                        &format!("config.{}", d.code.as_str()),
                        format!("the default of `{name}` in `{type_id}`: {}", d.message),
                    )),
                }
            }
        }
        out
    }

    /// The initial form of a new feature of `ty`: each parameter's name and
    /// expression, the user's default where there is one and the type's
    /// own where there is not.
    pub fn effective_params(&self, ty: &FeatureTypeSpec) -> Vec<(String, String)> {
        let own = self.by_type.get(&ty.id);
        ty.params
            .iter()
            .map(|p| {
                let expr = own
                    .and_then(|o| o.get(&p.name))
                    .unwrap_or(&p.default)
                    .clone();
                (p.name.clone(), expr)
            })
            .collect()
    }

    /// Whether the user changed `param` of `type_id`.
    pub fn is_overridden(&self, type_id: &str, param: &str) -> bool {
        self.by_type
            .get(type_id)
            .is_some_and(|o| o.contains_key(param))
    }
}
