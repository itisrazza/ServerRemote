//! Saved connection profiles ("sessions"), persisted as `sessions.toml` in
//! the platform config directory. Offered on the Manager tab as one-click
//! reconnects.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::ui::connection::Protocol;

use super::store;

/// A saved connection profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedSession {
    pub name: String,
    pub protocol: Protocol,
    pub hostname: String,
    /// Only iLO uses a user name.
    #[serde(default)]
    pub username: String,
    /// Stored in plaintext for now. TODO: move to OS keychain storage
    /// (`keyring` crate) once the saved-sessions flow is closer to
    /// production use — see the persistence-feature discussion.
    #[serde(default)]
    pub password: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sessions {
    pub saved: Vec<SavedSession>,
}

impl Sessions {
    fn path() -> Option<PathBuf> {
        store::config_dir().map(|dir| dir.join("sessions.toml"))
    }

    /// Loads saved sessions from disk, falling back to an empty list if the
    /// file is missing, unreadable, or fails to parse.
    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        store::load(&path).unwrap_or_else(|error| {
            eprintln!("failed to load saved sessions: {error}");
            Self::default()
        })
    }

    /// Persists saved sessions to disk. Failures are logged to stderr rather
    /// than surfaced to the UI — there's no notification mechanism yet.
    pub fn save(&self) {
        let Some(path) = Self::path() else {
            return;
        };
        if let Err(error) = store::save(&path, self) {
            eprintln!("failed to save saved sessions: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(name: &str) -> SavedSession {
        SavedSession {
            name: name.to_owned(),
            protocol: Protocol::Vnc,
            username: String::new(),
            hostname: "example.com".to_owned(),
            password: String::new(),
        }
    }

    #[test]
    fn sessions_round_trip_through_toml() {
        let sessions = Sessions {
            saved: vec![session("home"), session("work")],
        };

        let text = toml::to_string_pretty(&sessions).unwrap();
        let parsed: Sessions = toml::from_str(&text).unwrap();

        assert_eq!(parsed, sessions);
    }

    #[test]
    fn missing_password_field_defaults_to_empty() {
        let parsed: Sessions = toml::from_str(
            r#"
            [[saved]]
            name = "home"
            protocol = "vnc"
            hostname = "example.com"
            "#,
        )
        .unwrap();

        assert_eq!(parsed.saved[0].password, "");
    }
}
