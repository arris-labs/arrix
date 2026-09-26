//! User-facing failures as data (docs/ARCHITECTURE.md §Errors and
//! diagnostics): a severity, a stable code, a message, the references to
//! highlight and, for a lost reference, the ranked candidates.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::Ref;

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

pub(crate) fn is_segment(s: &str) -> bool {
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
    /// What to highlight: the entities the failure is about.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refs: Vec<Ref>,
    /// For a lost reference, what the user might re-pick, best first
    /// (docs/DATA-MODEL.md §Persistent naming, "Resolution"). Never
    /// applied by the core: re-picking is the user's command.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<Ref>,
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
            refs: Vec::new(),
            candidates: Vec::new(),
        })
    }

    pub fn with_refs(mut self, refs: impl IntoIterator<Item = Ref>) -> Self {
        self.refs.extend(refs);
        self
    }

    pub fn with_candidates(mut self, candidates: impl IntoIterator<Item = Ref>) -> Self {
        self.candidates.extend(candidates);
        self
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

    #[test]
    fn carries_its_references_and_ranked_candidates() {
        let lost: Ref = Ref::Topo(
            "face:sweep.0000000000001.side.0000000000002"
                .parse()
                .unwrap(),
        );
        let near: Ref = Ref::Topo(
            "face:sweep.0000000000001.side.0000000000003"
                .parse()
                .unwrap(),
        );
        let far: Ref = Ref::Topo("face:sweep.0000000000001.end-cap".parse().unwrap());
        let d = Diagnostic::new(Severity::Error, "ref.lost", "the flank is gone")
            .unwrap()
            .with_refs([lost])
            .with_candidates([near.clone(), far.clone()]);
        let json = serde_json::to_string(&d).unwrap();
        let back: Diagnostic = serde_json::from_str(&json).unwrap();
        assert_eq!(back, d);
        assert_eq!(
            back.candidates,
            [near, far],
            "the ranking survives the crossing"
        );
    }
}
