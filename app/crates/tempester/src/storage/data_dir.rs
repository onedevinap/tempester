//! The ONE access point to the user's data folder. Nothing else in the app builds a path from `HOME`.
//!
//! - macOS: `~/Library/Application Support/Tempester`
//! - Windows: `%APPDATA%\Tempester`
//! - Linux: `$XDG_DATA_HOME/tempester`, else `~/.local/share/tempester`
//! - `TEMPESTER_DATA_DIR` overrides all of the above (portable installs, tests).
//!
//! Under `cfg(test)` the folder is refused unless it lies inside the system temporary folder: a test run with the
//! user's real home must fail instead of touching real files (testing rules; run tests with `tools/test_app.sh`).

use std::fmt;
use std::path::PathBuf;
#[cfg(test)]
use std::path::{Component, Path};

/// The environment variable that overrides the platform location.
pub const OVERRIDE_VAR: &str = "TEMPESTER_DATA_DIR";

/// Operating systems with their own convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOs,
    Windows,
    Linux,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}

/// Why the data folder cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataDirError {
    /// No home folder (or `APPDATA`) is known.
    NoHome,
    /// A test run points outside the temporary folder.
    #[cfg(test)]
    NotDisposable(PathBuf),
}

impl fmt::Display for DataDirError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoHome => write!(f, "cannot find the home folder"),
            #[cfg(test)]
            Self::NotDisposable(path) => write!(
                f,
                "{} is not inside the temporary folder: tests must run with a disposable HOME \
                 (tools/test_app.sh)",
                path.display()
            ),
        }
    }
}

impl std::error::Error for DataDirError {}

/// Where the data folder is, given an environment lookup. Pure: nothing is created or checked.
pub fn resolve(
    platform: Platform,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<PathBuf, DataDirError> {
    let get = |name: &str| env(name).filter(|value| !value.is_empty());
    if let Some(path) = get(OVERRIDE_VAR) {
        return Ok(PathBuf::from(path));
    }
    let home = || get("HOME").map(PathBuf::from).ok_or(DataDirError::NoHome);
    match platform {
        Platform::MacOs => Ok(home()?.join("Library/Application Support/Tempester")),
        Platform::Windows => get("APPDATA")
            .map(|appdata| PathBuf::from(appdata).join("Tempester"))
            .ok_or(DataDirError::NoHome),
        Platform::Linux => match get("XDG_DATA_HOME") {
            Some(xdg) => Ok(PathBuf::from(xdg).join("tempester")),
            None => Ok(home()?.join(".local/share/tempester")),
        },
    }
}

#[cfg(test)]
/// Refuses `path` unless it is inside `temp_root`, comparing real locations (`/var` is `/private/var` on macOS).
pub fn ensure_disposable(path: &Path, temp_root: &Path) -> Result<(), DataDirError> {
    if lenient_canonical(path).starts_with(lenient_canonical(temp_root)) {
        Ok(())
    } else {
        Err(DataDirError::NotDisposable(path.to_path_buf()))
    }
}

#[cfg(test)]
/// Canonical form of the deepest existing ancestor, followed by the components that do not exist yet.
fn lenient_canonical(path: &Path) -> PathBuf {
    let mut missing = Vec::new();
    let mut existing = path;
    loop {
        if let Ok(real) = existing.canonicalize() {
            return missing.iter().rev().fold(real, |acc, part| acc.join(part));
        }
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                missing.push(name.to_os_string());
                existing = parent;
            }
            _ => return normalise(path),
        }
    }
}

#[cfg(test)]
/// Last resort when nothing exists: drop `.` and resolve `..` textually.
fn normalise(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

/// The data folder of this run (not created here).
pub fn data_dir() -> Result<PathBuf, DataDirError> {
    let path = resolve(Platform::current(), &|name| std::env::var(name).ok())?;
    #[cfg(test)]
    ensure_disposable(&path, &std::env::temp_dir())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.to_string())
        }
    }

    #[test]
    fn test_resolve_when_macos_returns_application_support() {
        let path = resolve(Platform::MacOs, &env(&[("HOME", "/Users/a")]));

        assert_eq!(
            path,
            Ok(PathBuf::from(
                "/Users/a/Library/Application Support/Tempester"
            ))
        );
    }

    #[test]
    fn test_resolve_when_windows_returns_appdata_tempester() {
        let path = resolve(
            Platform::Windows,
            &env(&[("APPDATA", "C:\\Users\\a\\AppData\\Roaming")]),
        );

        assert_eq!(
            path,
            Ok(PathBuf::from("C:\\Users\\a\\AppData\\Roaming").join("Tempester"))
        );
    }

    #[test]
    fn test_resolve_when_linux_with_xdg_returns_xdg_tempester() {
        let path = resolve(
            Platform::Linux,
            &env(&[("HOME", "/h"), ("XDG_DATA_HOME", "/x")]),
        );

        assert_eq!(path, Ok(PathBuf::from("/x/tempester")));
    }

    #[test]
    fn test_resolve_when_linux_without_xdg_returns_local_share() {
        let path = resolve(Platform::Linux, &env(&[("HOME", "/h")]));

        assert_eq!(path, Ok(PathBuf::from("/h/.local/share/tempester")));
    }

    #[test]
    fn test_resolve_when_override_set_returns_it_before_any_platform_rule() {
        let path = resolve(
            Platform::MacOs,
            &env(&[("HOME", "/h"), (OVERRIDE_VAR, "/portable")]),
        );

        assert_eq!(path, Ok(PathBuf::from("/portable")));
    }

    #[test]
    fn test_resolve_when_no_home_returns_error() {
        assert_eq!(
            resolve(Platform::MacOs, &env(&[])),
            Err(DataDirError::NoHome)
        );
        assert_eq!(
            resolve(Platform::Windows, &env(&[("HOME", "/h")])),
            Err(DataDirError::NoHome)
        );
        assert_eq!(
            resolve(Platform::Linux, &env(&[("HOME", "")])),
            Err(DataDirError::NoHome)
        );
    }

    #[test]
    fn test_ensure_disposable_when_real_home_returns_refusal() {
        let real = Path::new("/Users/someone/Library/Application Support/Tempester");

        let result = ensure_disposable(real, &std::env::temp_dir());

        assert_eq!(result, Err(DataDirError::NotDisposable(real.to_path_buf())));
    }

    #[test]
    fn test_ensure_disposable_when_inside_temp_returns_ok_even_if_not_created() {
        let inside = std::env::temp_dir().join("tempester-guard-test/not/created/yet");

        assert_eq!(ensure_disposable(&inside, &std::env::temp_dir()), Ok(()));
    }

    #[test]
    fn test_ensure_disposable_when_dotdot_escapes_temp_returns_refusal() {
        let escaping = std::env::temp_dir().join("a/../../../../../../etc/tempester");

        assert!(ensure_disposable(&escaping, &std::env::temp_dir()).is_err());
    }

    #[test]
    fn test_data_dir_when_run_by_a_test_never_returns_a_path_outside_temp() {
        // Under cfg(test) the guard applies: with a real HOME this is Err, with tools/test_app.sh it is Ok.
        if let Ok(path) = data_dir() {
            assert_eq!(ensure_disposable(&path, &std::env::temp_dir()), Ok(()));
        }
    }
}
