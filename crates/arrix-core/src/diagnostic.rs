//! User-facing failures as data (docs/ARCHITECTURE.md §Errors and
//! diagnostics). The persistent references to highlight and a lost
//! reference's ranked candidates join in M1, with `Ref`.

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// A stable, countable key: `segment(.segment)*`, each segment lower-kebab
/// (`ref.lost`, `kernel.degenerate.tangent-contact`). Checked when built,
/// so one that crosses a boundary is always well-formed.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct DiagnosticCode(String);

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error(
    "{0:?} is not a diagnostic code: dot-separated segments, each lower-case letters and \
     digits in hyphen-joined words, starting with a letter"
)]
pub struct InvalidCode(pub String);

fn is_segment(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_lowercase())
        && s.split('-').all(|word| {
            !word.is_empty()
                && word
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

impl DiagnosticCode {
    pub fn new(code: impl Into<String>) -> Result<Self, InvalidCode> {
        let code = code.into();
        if code.split('.').all(is_segment) {
            Ok(Self(code))
        } else {
            Err(InvalidCode(code))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for DiagnosticCode {
    type Error = InvalidCode;

    fn try_from(code: String) -> Result<Self, InvalidCode> {
        Self::new(code)
    }
}

impl From<DiagnosticCode> for String {
    fn from(code: DiagnosticCode) -> String {
        code.0
    }
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A failure, warning or note the user sees, on a feature, in the status
/// bar and in `arrix eval`'s output.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: DiagnosticCode,
    pub message: String,
}

impl Diagnostic {
    pub fn new(
        severity: Severity,
        code: &str,
        message: impl Into<String>,
    ) -> Result<Self, InvalidCode> {
        Ok(Self {
            severity,
            code: DiagnosticCode::new(code)?,
            message: message.into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_code_grammar() {
        for code in [
            "ref.lost",
            "kernel.degenerate.tangent-contact",
            "plugin.frozen.missing",
            "doc",
            "io.schema-v2",
        ] {
            assert!(DiagnosticCode::new(code).is_ok(), "{code}");
        }
    }

    #[test]
    fn refuses_what_breaks_it() {
        for code in [
            "",
            "Ref.lost",
            "ref..lost",
            "ref.lost.",
            ".ref",
            "ref.-lost",
            "ref.lost-",
            "ref.tangent--contact",
            "ref.2d",
            "ref_lost",
            "ref lost",
        ] {
            assert_eq!(
                DiagnosticCode::new(code),
                Err(InvalidCode(code.into())),
                "{code}"
            );
        }
    }

    #[test]
    fn crosses_as_json_and_is_checked_on_the_way_in() {
        let d = Diagnostic::new(Severity::Error, "ref.lost", "the face is gone").unwrap();
        let json = serde_json::to_string(&d).unwrap();
        assert_eq!(
            json,
            r#"{"severity":"error","code":"ref.lost","message":"the face is gone"}"#
        );
        assert_eq!(serde_json::from_str::<Diagnostic>(&json).unwrap(), d);
        let bad = json.replace("ref.lost", "Ref.Lost");
        assert!(serde_json::from_str::<Diagnostic>(&bad).is_err());
    }
}
