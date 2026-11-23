//! Saved connection profiles ("sessions"), persisted as `sessions.toml` in
//! the config directory and offered on the Manager tab as one-click
//! reconnects. Each one may have a thumbnail of its last screen, kept as a
//! PNG beside it (see [`thumbnail`]).

use std::fs;
use std::hash::{BuildHasher, Hasher, RandomState};
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ui::connection::Protocol;

use super::store;

/// A saved connection profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedSession {
    /// Stable identity, so a thumbnail survives renaming. Zero means not
    /// yet assigned (files from older versions, imports); [`Sessions::load`]
    /// and [`Sessions::add`] hand out real ones.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub id: u64,
    pub name: String,
    /// The sidebar group it's listed under; empty for none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub group: String,
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

fn is_zero(id: &u64) -> bool {
    *id == 0
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sessions {
    pub saved: Vec<SavedSession>,
}

impl Sessions {
    fn path(dir: &Path) -> PathBuf {
        dir.join("sessions.toml")
    }

    /// Copies the sessions file to the first free `sessions.toml.bak`,
    /// `sessions.toml.bak.1`, … so an earlier backup is never replaced.
    fn back_up(dir: &Path) {
        let source = Self::path(dir);
        let backup = (0..)
            .map(|n| match n {
                0 => dir.join("sessions.toml.bak"),
                n => dir.join(format!("sessions.toml.bak.{n}")),
            })
            .find(|path| !path.exists())
            .expect("an unbounded range always finds a free name");
        match fs::copy(&source, &backup) {
            Ok(_) => eprintln!("kept the unreadable sessions as {}", backup.display()),
            Err(error) => eprintln!("failed to back up saved sessions: {error}"),
        }
    }

    /// Loads saved sessions from `dir`, falling back to an empty list without
    /// one or if the file is unreadable or fails to parse. Sessions saved
    /// before they had ids get one, written back straight away so their
    /// thumbnails stay attached. A file that fails to load is copied aside
    /// first, since the next save overwrites it.
    pub fn load(dir: Option<&Path>) -> Self {
        let Some(dir) = dir else {
            return Self::default();
        };
        let mut sessions: Self = store::load(&Self::path(dir)).unwrap_or_else(|error| {
            eprintln!("failed to load saved sessions: {error}");
            Self::back_up(dir);
            Self::default()
        });
        if sessions.assign_ids() {
            sessions.save(Some(dir));
        }
        sessions
    }

    /// Persists saved sessions to `dir`; does nothing without one. Failures
    /// are logged to stderr rather than surfaced to the UI.
    pub fn save(&self, dir: Option<&Path>) {
        let Some(dir) = dir else {
            return;
        };
        if let Err(error) = store::save(&Self::path(dir), self) {
            eprintln!("failed to save saved sessions: {error}");
        }
    }

    pub fn get(&self, id: u64) -> Option<&SavedSession> {
        self.saved.iter().find(|session| session.id == id)
    }

    /// Appends `session` under a fresh id, which it returns.
    pub fn add(&mut self, mut session: SavedSession) -> u64 {
        session.id = self.fresh_id();
        let id = session.id;
        self.saved.push(session);
        id
    }

    /// Replaces the session with `id`, keeping the id. Returns whether there
    /// was one.
    pub fn replace(&mut self, id: u64, session: SavedSession) -> bool {
        match self.saved.iter_mut().find(|saved| saved.id == id) {
            Some(saved) => {
                *saved = SavedSession { id, ..session };
                true
            }
            None => false,
        }
    }

    pub fn remove(&mut self, id: u64) -> Option<SavedSession> {
        let index = self.saved.iter().position(|session| session.id == id)?;
        Some(self.saved.remove(index))
    }

    /// The groups in use, sorted, each with how many sessions it holds.
    pub fn groups(&self) -> Vec<(&str, usize)> {
        let mut groups: Vec<(&str, usize)> = Vec::new();
        for session in &self.saved {
            if session.group.is_empty() {
                continue;
            }
            match groups.iter_mut().find(|(name, _)| *name == session.group) {
                Some((_, count)) => *count += 1,
                None => groups.push((&session.group, 1)),
            }
        }
        groups.sort_by(|a, b| a.0.cmp(b.0));
        groups
    }

    /// Gives every session without an id (or sharing one) a fresh one.
    /// Returns whether any changed.
    fn assign_ids(&mut self) -> bool {
        let mut changed = false;
        for index in 0..self.saved.len() {
            let id = self.saved[index].id;
            let taken = id == 0 || self.saved[..index].iter().any(|other| other.id == id);
            if taken {
                self.saved[index].id = self.fresh_id();
                changed = true;
            }
        }
        changed
    }

    fn fresh_id(&self) -> u64 {
        loop {
            // `RandomState` is seeded randomly per instance; no need for a
            // random number crate just for this.
            let id = RandomState::new().build_hasher().finish();
            if id != 0 && self.get(id).is_none() {
                return id;
            }
        }
    }
}

/// Serialises `session` for sharing: no id (the importer assigns its own)
/// and no password, so an exported file can't leak one.
pub fn export(session: &SavedSession) -> Result<String, toml::ser::Error> {
    let shared = Sessions {
        saved: vec![SavedSession {
            id: 0,
            password: String::new(),
            ..session.clone()
        }],
    };
    toml::to_string_pretty(&shared)
}

/// Parses an exported file (or a whole `sessions.toml`). The sessions come
/// back without ids, ready for [`Sessions::add`].
pub fn import(contents: &str) -> Result<Vec<SavedSession>, toml::de::Error> {
    let sessions: Sessions = toml::from_str(contents)?;
    Ok(sessions
        .saved
        .into_iter()
        .map(|session| SavedSession { id: 0, ..session })
        .collect())
}

/// A session's last screen, as a small PNG beside `sessions.toml`.
pub mod thumbnail {
    use super::*;

    /// Stored at twice the largest card's thumbnail (240×135pt), so it stays
    /// sharp on 2× displays.
    pub const WIDTH: u32 = 480;
    pub const HEIGHT: u32 = 270;

    fn path(dir: &Path, id: u64) -> PathBuf {
        dir.join("thumbnails").join(format!("{id:016x}.png"))
    }

    pub fn load(dir: &Path, id: u64) -> Option<Vec<u8>> {
        fs::read(path(dir, id)).ok()
    }

    /// Crops an RGBA frame to the thumbnail's 16:9 from the centre, scales
    /// it down and encodes it as a PNG.
    pub fn encode(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
        use image::{ImageFormat, RgbaImage, imageops};

        let frame = RgbaImage::from_raw(width, height, rgba.to_vec())?;
        if width == 0 || height == 0 {
            return None;
        }
        // The largest 16:9 area that fits, centred.
        let (crop_w, crop_h) = if u64::from(width) * 9 > u64::from(height) * 16 {
            (height * 16 / 9, height)
        } else {
            (width, width * 9 / 16)
        };
        let cropped = imageops::crop_imm(
            &frame,
            (width - crop_w) / 2,
            (height - crop_h) / 2,
            crop_w.max(1),
            crop_h.max(1),
        )
        .to_image();
        let scaled = imageops::resize(&cropped, WIDTH, HEIGHT, imageops::FilterType::Triangle);
        let mut png = io::Cursor::new(Vec::new());
        scaled.write_to(&mut png, ImageFormat::Png).ok()?;
        Some(png.into_inner())
    }

    pub fn save(dir: &Path, id: u64, png: &[u8]) -> io::Result<()> {
        let path = path(dir, id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, png)
    }

    pub fn remove(dir: &Path, id: u64) {
        match fs::remove_file(path(dir, id)) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => {
                eprintln!("failed to remove thumbnail: {error}");
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(name: &str) -> SavedSession {
        SavedSession {
            id: 0,
            name: name.to_owned(),
            group: String::new(),
            protocol: Protocol::Vnc,
            username: String::new(),
            hostname: "example.com".to_owned(),
            password: String::new(),
        }
    }

    #[test]
    fn sessions_round_trip_through_toml() {
        let mut sessions = Sessions::default();
        sessions.add(session("home"));
        sessions.add(SavedSession {
            group: "Lab".to_owned(),
            ..session("work")
        });

        let text = toml::to_string_pretty(&sessions).unwrap();
        let parsed: Sessions = toml::from_str(&text).unwrap();

        assert_eq!(parsed, sessions);
    }

    #[test]
    fn missing_optional_fields_default_to_empty() {
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
        assert_eq!(parsed.saved[0].group, "");
        assert_eq!(parsed.saved[0].id, 0);
    }

    #[test]
    fn load_assigns_missing_ids_and_saves_them() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("sessions.toml"),
            "[[saved]]\nname = \"a\"\nprotocol = \"vnc\"\nhostname = \"a\"\n\
             [[saved]]\nname = \"b\"\nprotocol = \"vnc\"\nhostname = \"b\"\n",
        )
        .unwrap();

        let loaded = Sessions::load(Some(dir.path()));
        assert!(loaded.saved.iter().all(|session| session.id != 0));
        assert_ne!(loaded.saved[0].id, loaded.saved[1].id);

        let reloaded = Sessions::load(Some(dir.path()));
        assert_eq!(reloaded, loaded);
    }

    #[test]
    fn an_unreadable_file_is_kept_aside_before_it_can_be_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let broken = "[[saved]\nname = \"a\"\n";
        fs::write(dir.path().join("sessions.toml"), broken).unwrap();
        fs::write(dir.path().join("sessions.toml.bak"), "older").unwrap();

        let mut loaded = Sessions::load(Some(dir.path()));
        assert!(loaded.saved.is_empty());
        loaded.add(session("new"));
        loaded.save(Some(dir.path()));

        let read = |name| fs::read_to_string(dir.path().join(name)).unwrap();
        assert_eq!(read("sessions.toml.bak"), "older");
        assert_eq!(read("sessions.toml.bak.1"), broken);
    }

    #[test]
    fn a_missing_file_is_not_backed_up() {
        let dir = tempfile::tempdir().unwrap();
        let _ = Sessions::load(Some(dir.path()));
        assert!(!dir.path().join("sessions.toml.bak").exists());
    }

    #[test]
    fn load_without_a_directory_is_empty() {
        assert_eq!(Sessions::load(None), Sessions::default());
    }

    #[test]
    fn replace_keeps_the_id() {
        let mut sessions = Sessions::default();
        let id = sessions.add(session("old"));

        assert!(sessions.replace(id, session("new")));

        assert_eq!(sessions.get(id).unwrap().name, "new");
        assert!(!sessions.replace(id.wrapping_add(1), session("other")));
    }

    #[test]
    fn groups_are_sorted_and_counted() {
        let mut sessions = Sessions::default();
        for (name, group) in [
            ("a", "Old PCs"),
            ("b", "Homelab"),
            ("c", ""),
            ("d", "Old PCs"),
        ] {
            sessions.add(SavedSession {
                group: group.to_owned(),
                ..session(name)
            });
        }

        assert_eq!(sessions.groups(), vec![("Homelab", 1), ("Old PCs", 2)]);
    }

    #[test]
    fn export_leaves_out_the_password_and_id() {
        let mut sessions = Sessions::default();
        let id = sessions.add(SavedSession {
            password: "hunter2".to_owned(),
            ..session("home")
        });

        let text = export(sessions.get(id).unwrap()).unwrap();
        assert!(!text.contains("hunter2"));

        let imported = import(&text).unwrap();
        assert_eq!(imported, vec![session("home")]);
    }

    #[test]
    fn import_rejects_malformed_files() {
        assert!(import("not = [valid").is_err());
    }

    #[test]
    fn thumbnails_are_cropped_to_16_by_9() {
        // A 4:3 frame: the top and bottom bands are cropped off.
        let (width, height) = (64, 48);
        let png = thumbnail::encode(width, height, &vec![255; (width * height * 4) as usize])
            .expect("encodes");
        let decoded = image::load_from_memory(&png).unwrap();
        assert_eq!(
            (decoded.width(), decoded.height()),
            (thumbnail::WIDTH, thumbnail::HEIGHT)
        );

        assert!(thumbnail::encode(2, 2, &[0; 3]).is_none());
    }

    #[test]
    fn thumbnails_round_trip_through_the_directory() {
        let dir = tempfile::tempdir().unwrap();
        thumbnail::save(dir.path(), 7, b"png").unwrap();
        assert_eq!(thumbnail::load(dir.path(), 7).as_deref(), Some(&b"png"[..]));

        thumbnail::remove(dir.path(), 7);
        assert_eq!(thumbnail::load(dir.path(), 7), None);
    }
}
