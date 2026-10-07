//! A diagnostic about a config file: the core `Diagnostic` plus where it
//! is, so a user or their agent goes straight to the line (ADR-0016,
//! constraint 2).

use arrix_core::{Diagnostic, Severity};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigDiagnostic {
    /// The file's path inside the config directory, `/`-separated.
    pub file: String,
    /// 1-based; absent when the whole file is at fault (it can't be read).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// 1-based, in characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
    #[serde(flatten)]
    pub diagnostic: Diagnostic,
}

impl ConfigDiagnostic {
    /// An error in `file`, at byte `offset` of its `text` when known.
    /// `code` is one of this crate's own, so it is always well-formed.
    pub(crate) fn error(
        file: &str,
        text: &str,
        offset: Option<usize>,
        code: &str,
        message: impl Into<String>,
    ) -> Self {
        let (line, column) = match offset {
            Some(at) => {
                let (line, column) = line_column(text, at);
                (Some(line), Some(column))
            }
            None => (None, None),
        };
        Self {
            file: file.to_owned(),
            line,
            column,
            diagnostic: Diagnostic::new(Severity::Error, code, message)
                .expect("a config diagnostic code is well-formed"),
        }
    }
}

/// The 1-based line and character column of byte `offset` in `text`.
fn line_column(text: &str, offset: usize) -> (u32, u32) {
    let mut at = offset.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    let before = &text[..at];
    let line = before.matches('\n').count() + 1;
    let start = before.rfind('\n').map_or(0, |i| i + 1);
    let column = before[start..].chars().count() + 1;
    (to_u32(line), to_u32(column))
}

fn to_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_and_columns_are_one_based_and_count_characters() {
        let text = "a = 1\né = 2\n";
        assert_eq!(line_column(text, 0), (1, 1));
        assert_eq!(line_column(text, 4), (1, 5));
        assert_eq!(line_column(text, 6), (2, 1));
        // `=` after the two-byte `é` and a space is the third character.
        assert_eq!(line_column(text, 9), (2, 3));
    }

    #[test]
    fn crosses_as_one_flat_json_object() {
        let d = ConfigDiagnostic::error(
            "settings.toml",
            "x = 1",
            Some(0),
            "config.unknown-key",
            "no",
        );
        assert_eq!(
            serde_json::to_string(&d).unwrap(),
            r#"{"file":"settings.toml","line":1,"column":1,"severity":"error","code":"config.unknown-key","message":"no"}"#
        );
    }
}
