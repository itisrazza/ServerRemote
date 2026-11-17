//! Local persistence: [`sessions`] (saved connection profiles), stored as
//! TOML files in the platform config directory.
//!
//! There are no application-wide settings yet to justify their own module —
//! `store` is generic, so adding one later is a matter of a `Settings` struct
//! plus a `settings.rs` mirroring `sessions.rs`, not new plumbing.

mod store;

pub mod sessions;
