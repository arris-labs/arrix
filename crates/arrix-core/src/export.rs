//! The exchange formats ArriX writes (docs/ARCHITECTURE.md §Formats and
//! tools): plain data, so a request, a call record and a file dialog name
//! one the same way.

use serde::{Deserialize, Serialize};

/// A file format a part is exported as.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    /// STEP AP214 Part 21 text, the exact B-rep.
    Step,
    /// Binary STL of the refined meshes.
    Stl,
    /// OBJ text of the refined meshes.
    Obj,
}

impl ExportFormat {
    pub const ALL: [ExportFormat; 3] = [ExportFormat::Step, ExportFormat::Stl, ExportFormat::Obj];

    /// The file extension, without the dot.
    pub const fn extension(self) -> &'static str {
        match self {
            ExportFormat::Step => "step",
            ExportFormat::Stl => "stl",
            ExportFormat::Obj => "obj",
        }
    }

    /// The format a file extension (any case, `stp` too) names.
    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension.to_ascii_lowercase().as_str() {
            "step" | "stp" => Some(ExportFormat::Step),
            "stl" => Some(ExportFormat::Stl),
            "obj" => Some(ExportFormat::Obj),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_extension_names_its_format_and_back() {
        for f in ExportFormat::ALL {
            assert_eq!(ExportFormat::from_extension(f.extension()), Some(f));
        }
        assert_eq!(
            ExportFormat::from_extension("STP"),
            Some(ExportFormat::Step)
        );
        assert_eq!(ExportFormat::from_extension("3mf"), None);
    }
}
