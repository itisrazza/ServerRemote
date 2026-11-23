//! ServerRemote's application logic, shared by the desktop (iced) and tablet
//! (SwiftUI) front ends. Nothing here depends on a UI toolkit.

pub mod connect;
pub mod i18n;
pub mod persistence;
pub mod protocol;
pub mod update;

pub use protocol::Protocol;
