//! Local persistence: [`sessions`] (saved connection profiles) and
//! [`settings`] (app-wide preferences), stored as TOML files in the platform
//! config directory.

mod store;

pub mod sessions;
pub mod settings;
