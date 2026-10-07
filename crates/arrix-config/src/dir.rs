//! Where the user's config directory is on each platform. Pure: the caller
//! hands in the environment, so this reads nothing and builds for wasm.

use std::path::PathBuf;

/// The operating system families whose conventions differ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    Linux,
    MacOs,
    Windows,
}

impl Os {
    /// The one this was built for; any Unix that is not macOS counts as Linux.
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}

/// The directory of the config files: `$ARRIX_CONFIG_DIR`, else `arrix`
/// under the platform's config directory (`$XDG_CONFIG_HOME` or
/// `~/.config`; `~/Library/Application Support`; `%APPDATA%`). `var` reads
/// the environment; an empty variable counts as unset. `None` when the
/// platform's base is unknown, which is the built-in configuration.
pub fn config_dir(os: Os, var: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let var = |name: &str| var(name).filter(|v| !v.is_empty());
    if let Some(dir) = var("ARRIX_CONFIG_DIR") {
        return Some(PathBuf::from(dir));
    }
    let base = match os {
        Os::Linux => var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| var("HOME").map(|h| PathBuf::from(h).join(".config"))),
        Os::MacOs => var("HOME").map(|h| PathBuf::from(h).join("Library/Application Support")),
        Os::Windows => var("APPDATA").map(PathBuf::from),
    };
    base.map(|b| b.join("arrix"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| (*v).to_owned())
        }
    }

    #[test]
    fn the_variable_wins_then_each_platforms_own() {
        let all = [
            ("ARRIX_CONFIG_DIR", "/etc/mine"),
            ("XDG_CONFIG_HOME", "/xdg"),
            ("HOME", "/home/u"),
            ("APPDATA", "C:/Roaming"),
        ];
        for os in [Os::Linux, Os::MacOs, Os::Windows] {
            assert_eq!(config_dir(os, env(&all)), Some("/etc/mine".into()));
        }
        let rest = &all[1..];
        assert_eq!(config_dir(Os::Linux, env(rest)), Some("/xdg/arrix".into()));
        assert_eq!(
            config_dir(Os::Linux, env(&rest[1..])),
            Some("/home/u/.config/arrix".into())
        );
        assert_eq!(
            config_dir(Os::MacOs, env(rest)),
            Some("/home/u/Library/Application Support/arrix".into())
        );
        assert_eq!(
            config_dir(Os::Windows, env(rest)),
            Some("C:/Roaming/arrix".into())
        );
    }

    #[test]
    fn an_empty_variable_is_unset_and_no_base_is_none() {
        let empty = [
            ("ARRIX_CONFIG_DIR", ""),
            ("XDG_CONFIG_HOME", ""),
            ("HOME", "/h"),
        ];
        assert_eq!(
            config_dir(Os::Linux, env(&empty)),
            Some("/h/.config/arrix".into())
        );
        assert_eq!(config_dir(Os::Linux, env(&[])), None);
        assert_eq!(config_dir(Os::Windows, env(&[("HOME", "/h")])), None);
    }
}
