//! A config file parsed with its spans, so every later check can name the
//! line it is about. A syntax error does not lose the rest of the file:
//! what parsed is still applied (failing soft, ADR-0015).

use toml::Spanned;
use toml::de::{DeTable, DeValue};

use crate::{ConfigDiagnostic, ConfigSource};

/// One file's text: its path for diagnostics, and the text spans index.
#[derive(Clone, Copy)]
pub(crate) struct File<'a> {
    pub path: &'a str,
    pub text: &'a str,
}

impl<'a> File<'a> {
    /// An error at the start of `span`.
    pub fn error(
        &self,
        span: std::ops::Range<usize>,
        code: &str,
        message: impl Into<String>,
    ) -> ConfigDiagnostic {
        ConfigDiagnostic::error(self.path, self.text, Some(span.start), code, message)
    }

    /// A warning at the start of `span`.
    pub fn warning(
        &self,
        span: std::ops::Range<usize>,
        code: &str,
        message: impl Into<String>,
    ) -> ConfigDiagnostic {
        ConfigDiagnostic::warning(self.path, self.text, Some(span.start), code, message)
    }

    /// The file's tables, as far as they parsed.
    pub fn parse(&self) -> Parsed<'a> {
        let (table, errors) = DeTable::parse_recoverable(self.text);
        let errors: Vec<_> = errors
            .iter()
            .map(|e| {
                (
                    e.span().map_or(0, |s| s.start),
                    e.message().trim().to_owned(),
                )
            })
            .collect();
        Parsed { table, errors }
    }
}

/// A file parsed as far as it goes, with where its syntax errors are.
pub(crate) struct Parsed<'a> {
    pub table: Spanned<DeTable<'a>>,
    errors: Vec<(usize, String)>,
}

impl Parsed<'_> {
    /// One `config.syntax` diagnostic per syntax error.
    pub fn diagnostics(&self, file: File<'_>) -> Vec<ConfigDiagnostic> {
        self.errors
            .iter()
            .map(|(at, message)| file.error(*at..*at, "config.syntax", message.clone()))
            .collect()
    }

    /// Whether a syntax error lies in `key = value`: the parser's recovery
    /// may have guessed the value, so the key keeps the value below it.
    pub fn is_broken(&self, key: &Spanned<impl Sized>, value: &Spanned<impl Sized>) -> bool {
        let span = key.span().start..=value.span().end;
        self.errors.iter().any(|(at, _)| span.contains(at))
    }
}

/// Sorts `diagnostics` into the file's order.
pub(crate) fn in_file_order(diagnostics: &mut [ConfigDiagnostic]) {
    diagnostics.sort_by_key(|d| (d.line, d.column));
}

/// The TOML type's name, for "is a …, not a …" messages.
pub(crate) fn type_name(value: &DeValue<'_>) -> &'static str {
    match value {
        DeValue::String(_) => "a string",
        DeValue::Integer(_) => "an integer",
        DeValue::Float(_) => "a float",
        DeValue::Boolean(_) => "a boolean",
        DeValue::Datetime(_) => "a date-time",
        DeValue::Array(_) => "an array",
        DeValue::Table(_) => "a table",
    }
}

/// The text of `path` from `source`: `None` when there is no such file,
/// and when it can't be read or isn't UTF-8, which is then one diagnostic
/// without a line.
pub(crate) fn read_text(
    source: &impl ConfigSource,
    path: &str,
    diagnostics: &mut Vec<ConfigDiagnostic>,
) -> Option<String> {
    let whole_file = |code, message| ConfigDiagnostic::error(path, "", None, code, message);
    match source.read(path) {
        Ok(None) => None,
        Ok(Some(bytes)) => match String::from_utf8(bytes) {
            Ok(text) => Some(text),
            Err(e) => {
                diagnostics.push(whole_file("config.syntax", format!("not UTF-8 text: {e}")));
                None
            }
        },
        Err(e) => {
            diagnostics.push(whole_file("config.io", format!("cannot read: {e}")));
            None
        }
    }
}
