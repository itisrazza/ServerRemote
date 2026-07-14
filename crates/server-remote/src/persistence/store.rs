//! Shared TOML load/save plumbing for [`settings`](super::settings) and
//! [`sessions`](super::sessions).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Serialize, de::DeserializeOwned};

/// The directory app data lives in, or `None` if the platform offers no
/// notion of one — in which case settings/sessions simply don't persist.
pub fn config_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("io", "razza", "ServerRemote")
        .map(|dirs| dirs.config_dir().to_path_buf())
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Parse(toml::de::Error),
    Serialize(toml::ser::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(err) => write!(f, "{err}"),
            Error::Parse(err) => write!(f, "{err}"),
            Error::Serialize(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for Error {}

/// Loads a TOML file at `path`, returning `T::default()` if it doesn't exist
/// yet (e.g. first run).
pub fn load<T: DeserializeOwned + Default>(path: &Path) -> Result<T, Error> {
    match fs::read_to_string(path) {
        Ok(contents) => toml::from_str(&contents).map_err(Error::Parse),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(T::default()),
        Err(err) => Err(Error::Io(err)),
    }
}

/// Serializes `value` as TOML and writes it to `path`, creating parent
/// directories as needed.
pub fn save<T: Serialize>(path: &Path, value: &T) -> Result<(), Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(Error::Io)?;
    }
    let contents = toml::to_string_pretty(value).map_err(Error::Serialize)?;
    fs::write(path, contents).map_err(Error::Io)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
    struct Doc {
        #[serde(default)]
        name: String,
    }

    #[test]
    fn load_returns_default_when_the_file_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.toml");

        let doc: Doc = load(&path).unwrap();

        assert_eq!(doc, Doc::default());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("doc.toml");
        let doc = Doc {
            name: "example".to_owned(),
        };

        save(&path, &doc).unwrap();
        let loaded: Doc = load(&path).unwrap();

        assert_eq!(loaded, doc);
    }

    #[test]
    fn load_reports_a_parse_error_for_malformed_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.toml");
        fs::write(&path, "not = [valid").unwrap();

        let result: Result<Doc, Error> = load(&path);

        assert!(matches!(result, Err(Error::Parse(_))));
    }
}
