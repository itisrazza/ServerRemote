//! A live session and the events flowing across it.

use std::borrow::Cow;

use tokio::sync::mpsc;

use crate::Capabilities;

/// A decoded framebuffer: RGBA8, row-major, exactly `width * height * 4` bytes.
#[derive(Debug, Clone)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// A pointer button reported by the local UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerButton {
    Left,
    Middle,
    Right,
}

/// Input travelling from the local UI toward the remote session.
#[derive(Debug, Clone)]
pub enum InputEvent {
    /// Absolute pointer position, for [`PointerMode::Absolute`] sessions.
    ///
    /// [`PointerMode::Absolute`]: crate::PointerMode::Absolute
    PointerMoved { x: f32, y: f32 },
    /// Relative pointer motion, for [`PointerMode::Relative`] sessions.
    ///
    /// [`PointerMode::Relative`]: crate::PointerMode::Relative
    PointerMotion { dx: f32, dy: f32 },
    PointerButton {
        button: PointerButton,
        pressed: bool,
    },
    /// A key identified by its X11 keysym, matching what RFB/Spice expect.
    Key { keysym: u32, pressed: bool },
}

/// Events emitted by a running session toward the UI.
#[derive(Debug, Clone)]
pub enum SessionEvent {
    /// The session is established; the remote framebuffer is `size` pixels and
    /// the session supports `capabilities`.
    Connected {
        size: (u32, u32),
        capabilities: Capabilities,
    },
    /// A new framebuffer is available.
    Frame(Frame),
    /// The remote framebuffer changed size.
    Resized { width: u32, height: u32 },
    /// The session's capabilities changed mid-session (e.g. the server switched
    /// pointer mode).
    Capabilities(Capabilities),
    /// The session ended. `reason` is `None` on a clean close.
    Disconnected { reason: Option<String> },
}

/// A non-input request from the UI to the running session.
///
/// Input has its own channel; this carries the rarer, structured actions that
/// [`Capabilities`] advertises.
#[derive(Debug, Clone)]
pub enum SessionCommand {
    /// Invoke a declarative [`Action`](crate::Action) by id.
    Action { id: Cow<'static, str> },
}

/// The UI's end of a live session.
///
/// The protocol implementation owns the connection on a background task and
/// holds the other end of every channel. Send [`InputEvent`]s on `input` and
/// [`SessionCommand`]s on `commands`; receive [`SessionEvent`]s on `events`.
/// Dropping every sender (and this handle's `events`) signals shutdown.
pub struct SessionHandle {
    pub events: mpsc::Receiver<SessionEvent>,
    pub input: mpsc::Sender<InputEvent>,
    pub commands: mpsc::Sender<SessionCommand>,
}
