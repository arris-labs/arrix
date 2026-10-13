//! Names that are safe in a URDF and in a file name.

/// ASCII letters, digits, `-`, `_` and `.` stay; anything else is `_`. An
/// empty name is `unnamed`.
pub fn sanitize(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    // A dot-only name would be a path part.
    if s.is_empty() || s.chars().all(|c| c == '.') {
        "unnamed".into()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::sanitize;

    #[test]
    fn unsafe_characters_become_underscores() {
        assert_eq!(sanitize("Arm 1/ü"), "Arm_1__");
        assert_eq!(sanitize(""), "unnamed");
        assert_eq!(sanitize(".."), "unnamed");
    }
}
