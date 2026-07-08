//! A live session and the events flowing across it.

use tokio::sync::mpsc;

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
    PointerMoved {
        x: f32,
        y: f32,
    },
    PointerButton {
        button: PointerButton,
        pressed: bool,
    },
    /// A key identified by its X11 keysym, matching what RFB/Spice expect.
    Key {
        keysym: u32,
        pressed: bool,
    },
}

/// Events emitted by a running session toward the UI.
#[derive(Debug, Clone)]
pub enum SessionEvent {
    /// The session is established; the remote framebuffer is `size` pixels.
    Connected { size: (u32, u32) },
    /// A new framebuffer is available.
    Frame(Frame),
    /// The remote framebuffer changed size.
    Resized { width: u32, height: u32 },
    /// The session ended. `reason` is `None` on a clean close.
    Disconnected { reason: Option<String> },
}

/// The UI's end of a live session.
///
/// The protocol implementation owns the connection on a background task and
/// holds the other end of both channels. Send [`InputEvent`]s on `input` and
/// receive [`SessionEvent`]s on `events`. Dropping the handle closes `input`,
/// which the implementation should treat as a request to shut down.
pub struct SessionHandle {
    pub events: mpsc::Receiver<SessionEvent>,
    pub input: mpsc::Sender<InputEvent>,
}
