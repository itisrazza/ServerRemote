//! ServerRemote's application logic, shared by the desktop (iced) and tablet
//! (SwiftUI) front ends. Nothing here depends on a UI toolkit.

pub mod app;
pub mod connect;
pub mod connection;
pub mod i18n;
pub mod keys;
pub mod persistence;
pub mod protocol;
pub mod update;

pub use app::App;
pub use protocol::Protocol;
