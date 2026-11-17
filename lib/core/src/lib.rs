//! Core types and traits shared across ServerRemote's protocol implementations.
//!
//! A protocol crate (VNC, Spice, iLO, ...) provides a [`Connector`]. Connecting
//! yields a [`SessionHandle`]: the UI sends [`InputEvent`]s in and receives
//! [`SessionEvent`]s out, while the protocol implementation owns the live
//! connection on a background task.

pub mod capabilities;
pub mod connector;
pub mod dummy;
pub mod session;
pub mod status;
pub mod tls;
pub mod transport;

pub use capabilities::{
    Action, Capabilities, Confirmation, Control, Icon, MenuItem, PointerMode, ToolbarItem,
};
pub use connector::{ConnectError, Connector};
pub use dummy::DummyConnector;
pub use session::{
    ActionValue, Frame, InputEvent, PointerButton, SessionCommand, SessionEvent, SessionHandle,
};
pub use status::{Chip, Detail, IndicatorState, Status, StatusItem, Traffic};
pub use tls::{CertFingerprint, TlsPolicy};
pub use transport::Target;
