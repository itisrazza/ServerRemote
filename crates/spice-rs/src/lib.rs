//! A minimal, pure-Rust SPICE protocol client: connection, display, and
//! input. Wraps [`shakenfist-spice-protocol`](shakenfist_spice_protocol) for
//! the link handshake (RSA-OAEP/SHA-1 ticket auth) and adds what that crate
//! deliberately leaves to callers: draw-command parsing, ACK flow control, a
//! session/channel model, and keysym-to-scancode translation.
//!
//! Scope: connection, video (uncompressed `Pixmap` images only — no
//! LZ/GLZ/QUIC/JPEG/video-stream decoding), and input (keyboard, absolute
//! pointer). No TLS, no clipboard/audio/cursor-channel rendering, no
//! migration.

mod framebuffer;
mod keymap;
mod wire;

use std::io;

use shakenfist_spice_protocol::link::{self, SpiceStream};
use shakenfist_spice_protocol::messages::{self, MessageHeader};
use shakenfist_spice_protocol::{ChannelType, ImageType, SpiceError, main_server};
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::mpsc;

use framebuffer::Framebuffer;
use wire::AckState;

/// A decoded framebuffer: RGBA8, row-major, `width * height * 4` bytes.
#[derive(Debug, Clone)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Parameters for [`connect`].
pub struct ConnectOptions {
    pub host: String,
    pub port: u16,
    pub password: Option<String>,
}

/// A pointer button reported by the local UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
}

/// Input travelling from the local UI toward the remote session.
#[derive(Debug, Clone)]
pub enum SpiceInput {
    /// Absolute pointer position (SPICE's client mouse mode — the only mode
    /// this crate drives).
    PointerMoved {
        x: f32,
        y: f32,
    },
    PointerButton {
        button: MouseButton,
        pressed: bool,
    },
    /// An X11 keysym; translated internally to a PC/AT Set-1 scan code,
    /// since that's SPICE protocol domain knowledge any consumer of this
    /// crate would otherwise have to duplicate.
    Key {
        keysym: u32,
        pressed: bool,
    },
}

/// Events emitted by a running session toward the UI.
#[derive(Debug, Clone)]
pub enum SpiceEvent {
    Connected { width: u32, height: u32 },
    Frame(Frame),
    Resized { width: u32, height: u32 },
    Disconnected { reason: Option<String> },
}

/// The caller's end of a live session. The session runs on a background
/// tokio task and holds the other end of both channels; dropping both
/// signals shutdown.
pub struct SpiceHandle {
    pub events: mpsc::Receiver<SpiceEvent>,
    pub input: mpsc::Sender<SpiceInput>,
}

/// Why a connection attempt failed.
#[derive(Debug)]
pub enum SpiceConnectError {
    Transport(io::Error),
    Auth(String),
    Protocol(String),
}

impl std::fmt::Display for SpiceConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpiceConnectError::Transport(error) => write!(f, "transport error: {error}"),
            SpiceConnectError::Auth(message) => write!(f, "authentication failed: {message}"),
            SpiceConnectError::Protocol(message) => write!(f, "protocol error: {message}"),
        }
    }
}

impl std::error::Error for SpiceConnectError {}

/// Open a SPICE session: link + authenticate the Main, Display, and Inputs
/// channels, then spawn a background task that owns them.
pub async fn connect(options: ConnectOptions) -> Result<SpiceHandle, SpiceConnectError> {
    let ConnectOptions {
        host,
        port,
        password,
    } = options;

    let mut main_stream =
        open_channel(&host, port, 0, ChannelType::Main, 0, password.as_deref()).await?;

    let (header, payload) = wire::read_message(&mut main_stream)
        .await
        .map_err(SpiceConnectError::Transport)?;
    if header.message_type != main_server::INIT {
        return Err(SpiceConnectError::Protocol(format!(
            "expected MAIN_INIT (opcode {}), got opcode {}",
            main_server::INIT,
            header.message_type
        )));
    }
    let main_init = messages::MainInit::read(&payload)
        .map_err(|e| SpiceConnectError::Protocol(e.to_string()))?;
    let session_id = main_init.session_id;

    // Verify empirically: this connects Display/Inputs directly at
    // well-known channel_id 0 rather than waiting for
    // SPICE_MSGC_MAIN_ATTACH_CHANNELS/SPICE_MSG_MAIN_CHANNELS_LIST. If a
    // server rejects that (ChannelUnavailable/BadConnectionId), the
    // attach/list round-trip needs adding here.
    let mut display_stream = open_channel(
        &host,
        port,
        session_id,
        ChannelType::Display,
        0,
        password.as_deref(),
    )
    .await?;
    let inputs_stream = open_channel(
        &host,
        port,
        session_id,
        ChannelType::Inputs,
        0,
        password.as_deref(),
    )
    .await?;

    // Required, not optional despite the protocol spec text calling it
    // optional: spice-server's DisplayChannelClient sits in a
    // "wait_for_init" state after link/auth and won't send anything
    // (SURFACE_CREATE, DRAW_*) until this arrives, eventually timing out
    // and closing the connection if it never does. We don't use image
    // caching or a GLZ dictionary, so all-zero fields.
    let mut display_init = Vec::new();
    messages::DisplayInit {
        cache_id: 0,
        cache_size: 0,
        glz_dict_id: 0,
        glz_dict_window: 0,
    }
    .write(&mut display_init)
    .map_err(SpiceConnectError::Transport)?;
    wire::write_message(
        &mut display_stream,
        shakenfist_spice_protocol::display_client::INIT,
        &display_init,
    )
    .await
    .map_err(SpiceConnectError::Transport)?;

    // Best-effort: ask the server to prefer uncompressed images. Some older
    // spice-server builds ignore this; the fallback is disabling image
    // compression in the server's own configuration.
    wire::write_message(
        &mut display_stream,
        wire::DISPLAY_PREFERRED_COMPRESSION,
        &[wire::IMAGE_COMPRESSION_OFF],
    )
    .await
    .map_err(SpiceConnectError::Transport)?;

    let (event_tx, events) = mpsc::channel(16);
    let (input, input_rx) = mpsc::channel(256);

    tokio::spawn(run_session(
        main_stream,
        display_stream,
        inputs_stream,
        event_tx,
        input_rx,
    ));

    Ok(SpiceHandle { events, input })
}

async fn open_channel(
    host: &str,
    port: u16,
    connection_id: u32,
    channel_type: ChannelType,
    channel_id: u8,
    password: Option<&str>,
) -> Result<TcpStream, SpiceConnectError> {
    let tcp = TcpStream::connect((host, port))
        .await
        .map_err(SpiceConnectError::Transport)?;
    let mut stream = SpiceStream::Plain(tcp);

    let reply = link::perform_link(&mut stream, connection_id, channel_type, channel_id)
        .await
        .map_err(map_handshake_error)?;

    if let Some(error) = link_error_to_connect_error(reply.error) {
        return Err(error);
    }

    link::perform_auth(&mut stream, &reply.pub_key, password)
        .await
        .map_err(map_handshake_error)?;

    match stream {
        SpiceStream::Plain(tcp) => Ok(tcp),
        SpiceStream::Tls(_) => unreachable!("this connector never requests a TLS connection"),
    }
}

/// Classify a completed link's `reply.error` before any `anyhow` is
/// involved, giving full fidelity on this path (unlike
/// `shakenfist_spice_protocol::client::SpiceClient::connect_channel`, which
/// formats every non-`Ok` case, including `NeedSecured`, into a string).
fn link_error_to_connect_error(error: SpiceError) -> Option<SpiceConnectError> {
    match error {
        SpiceError::Ok => None,
        SpiceError::NeedSecured => Some(SpiceConnectError::Protocol(
            "server requires TLS; this connector only supports plain TCP".into(),
        )),
        other => Some(SpiceConnectError::Protocol(format!(
            "link error: {other:?}"
        ))),
    }
}

/// Maps handshake failures from `shakenfist_spice_protocol::link`'s
/// `anyhow::Result`-returning functions. `perform_auth`'s ticket-rejection
/// path has no typed error — this exact "Authentication failed" prefix
/// comes from the crate's own fixed format string (not attacker-controlled
/// text), so matching it is safe.
fn map_handshake_error(err: anyhow::Error) -> SpiceConnectError {
    if let Some(io) = err.downcast_ref::<io::Error>() {
        return SpiceConnectError::Transport(io::Error::new(io.kind(), io.to_string()));
    }
    let message = err.to_string();
    if message.starts_with("Authentication failed") {
        return SpiceConnectError::Auth(message);
    }
    SpiceConnectError::Protocol(message)
}

/// Reads messages off `read_half` in a loop and forwards each fully-read
/// `(MessageHeader, Vec<u8>)` (or the terminal read error) through the
/// returned channel.
///
/// `wire::read_message` spans two separate `.await` points (header, then
/// payload); calling it fresh inside `tokio::select!` on every loop
/// iteration is *not* cancellation-safe — if some other branch completes
/// first while a read is mid-flight, the in-progress read is dropped,
/// silently discarding however many bytes it had already consumed from the
/// socket, and the next call starts framing from the wrong offset. Giving
/// each channel its own task that owns the read loop end to end sidesteps
/// this: `run_session`'s `select!` only ever receives from the resulting
/// `mpsc::Receiver`, which *is* cancellation-safe.
fn spawn_reader(
    mut read_half: OwnedReadHalf,
) -> mpsc::Receiver<io::Result<(MessageHeader, Vec<u8>)>> {
    let (tx, rx) = mpsc::channel(8);
    tokio::spawn(async move {
        loop {
            let result = wire::read_message(&mut read_half).await;
            let failed = result.is_err();
            if tx.send(result).await.is_err() || failed {
                return;
            }
        }
    });
    rx
}

async fn run_session(
    main_stream: TcpStream,
    display_stream: TcpStream,
    inputs_stream: TcpStream,
    event_tx: mpsc::Sender<SpiceEvent>,
    mut input_rx: mpsc::Receiver<SpiceInput>,
) {
    let (main_read, mut main_write) = main_stream.into_split();
    let (display_read, mut display_write) = display_stream.into_split();
    let (inputs_read, mut inputs_write) = inputs_stream.into_split();

    let mut main_rx = spawn_reader(main_read);
    let mut display_rx = spawn_reader(display_read);
    let mut inputs_rx = spawn_reader(inputs_read);

    let mut fb: Option<Framebuffer> = None;
    let mut connected = false;
    let mut main_ack = AckState::new();
    let mut display_ack = AckState::new();
    let mut inputs_ack = AckState::new();
    let mut pointer = PointerState::default();

    loop {
        tokio::select! {
            message = main_rx.recv() => {
                match message {
                    Some(Ok((header, payload))) => {
                        if handle_ack_or_count(&header, &payload, &mut main_write, &mut main_ack).await.is_err() {
                            disconnect(&event_tx, None).await;
                            return;
                        }
                    }
                    Some(Err(error)) => {
                        disconnect(&event_tx, Some(error.to_string())).await;
                        return;
                    }
                    None => return,
                }
            }
            message = display_rx.recv() => {
                match message {
                    Some(Ok((header, payload))) => {
                        let mut dirty = match handle_display_message(&header, &payload, &mut display_write, &mut display_ack, &mut fb, &mut connected, &event_tx).await {
                            Ok(DisplayOutcome::Stop) => return,
                            Ok(DisplayOutcome::Dirty) => true,
                            Ok(DisplayOutcome::Continue) => false,
                            Err(error) => {
                                disconnect(&event_tx, Some(error.to_string())).await;
                                return;
                            }
                        };

                        // Drain anything else already queued up before
                        // snapshotting, so a burst of small draws becomes one
                        // coherent frame instead of several partially-painted
                        // ones (see DisplayOutcome::Dirty's doc comment).
                        while let Ok(next) = display_rx.try_recv() {
                            match next {
                                Ok((header, payload)) => {
                                    match handle_display_message(&header, &payload, &mut display_write, &mut display_ack, &mut fb, &mut connected, &event_tx).await {
                                        Ok(DisplayOutcome::Stop) => return,
                                        Ok(DisplayOutcome::Dirty) => dirty = true,
                                        Ok(DisplayOutcome::Continue) => {}
                                        Err(error) => {
                                            disconnect(&event_tx, Some(error.to_string())).await;
                                            return;
                                        }
                                    }
                                }
                                Err(error) => {
                                    disconnect(&event_tx, Some(error.to_string())).await;
                                    return;
                                }
                            }
                        }

                        if dirty
                            && let Some(canvas) = &fb
                            && event_tx.send(SpiceEvent::Frame(canvas.to_frame())).await.is_err()
                        {
                            return;
                        }
                    }
                    Some(Err(error)) => {
                        disconnect(&event_tx, Some(error.to_string())).await;
                        return;
                    }
                    None => return,
                }
            }
            message = inputs_rx.recv() => {
                match message {
                    Some(Ok((header, payload))) => {
                        if handle_ack_or_count(&header, &payload, &mut inputs_write, &mut inputs_ack).await.is_err() {
                            disconnect(&event_tx, None).await;
                            return;
                        }
                    }
                    Some(Err(error)) => {
                        disconnect(&event_tx, Some(error.to_string())).await;
                        return;
                    }
                    None => return,
                }
            }
            input = input_rx.recv() => match input {
                Some(event) => apply_input(&mut inputs_write, &mut pointer, event).await,
                None => return,
            },
        }
    }
}

/// `SPICE_MSG_SET_ACK`, numerically identical across every channel.
const SET_ACK: u16 = 3;

/// Handles messages common to every channel (`SET_ACK` flow control); every
/// other opcode on the Main/Inputs channels is out of scope for this pass
/// and ignored.
async fn handle_ack_or_count(
    header: &MessageHeader,
    payload: &[u8],
    stream: &mut OwnedWriteHalf,
    ack: &mut AckState,
) -> io::Result<()> {
    if header.message_type == SET_ACK {
        let set_ack = messages::SetAck::read(payload)?;
        let sync_payload = ack.set_ack(set_ack.generation, set_ack.window);
        wire::write_message(stream, wire::ACK_SYNC, &sync_payload).await?;
        return Ok(());
    }
    if ack.record_message() {
        wire::write_message(stream, wire::ACK, &[]).await?;
    }
    Ok(())
}

/// What handling one Display-channel message means for the run loop.
enum DisplayOutcome {
    /// Nothing further needed; any `Connected`/`Resized` event (rare, not a
    /// flood risk) was already sent directly.
    Continue,
    /// The canvas changed — the caller should coalesce this with any other
    /// already-queued draws before sending a single `Frame` snapshot,
    /// rather than one snapshot per tiny draw op (which otherwise shows the
    /// UI a rapid sequence of partially-painted intermediate states —
    /// visible as flicker/strobing, exactly as VNC's `run_session` avoids
    /// by draining its decoder before ever snapshotting).
    Dirty,
    /// `event_tx` is closed; the caller should stop the session.
    Stop,
}

async fn handle_display_message(
    header: &MessageHeader,
    payload: &[u8],
    stream: &mut OwnedWriteHalf,
    ack: &mut AckState,
    fb: &mut Option<Framebuffer>,
    connected: &mut bool,
    event_tx: &mpsc::Sender<SpiceEvent>,
) -> io::Result<DisplayOutcome> {
    use shakenfist_spice_protocol::display_server;

    let outcome = match header.message_type {
        SET_ACK => {
            handle_ack_or_count(header, payload, stream, ack).await?;
            return Ok(DisplayOutcome::Continue);
        }
        opcode if opcode == display_server::SURFACE_CREATE => {
            let mut outcome = DisplayOutcome::Continue;
            if let Ok(surface) = messages::SurfaceCreate::read(payload)
                && surface.surface_id == 0
            {
                *fb = Some(Framebuffer::new(surface.width, surface.height));
                let event = if *connected {
                    SpiceEvent::Resized {
                        width: surface.width,
                        height: surface.height,
                    }
                } else {
                    *connected = true;
                    SpiceEvent::Connected {
                        width: surface.width,
                        height: surface.height,
                    }
                };
                if event_tx.send(event).await.is_err() {
                    outcome = DisplayOutcome::Stop;
                }
            }
            outcome
        }
        opcode if opcode == display_server::DRAW_COPY => {
            let dirty = fb
                .as_mut()
                .is_some_and(|canvas| handle_draw_copy(payload, canvas) == Some(true));
            if dirty {
                DisplayOutcome::Dirty
            } else {
                DisplayOutcome::Continue
            }
        }
        // MODE/MARK/RESET/COPY_BITS, other DRAW_* ops, STREAM_*,
        // MONITORS_CONFIG, GL_* — out of scope, ignored.
        _ => DisplayOutcome::Continue,
    };

    if ack.record_message() {
        wire::write_message(stream, wire::ACK, &[]).await?;
    }
    Ok(outcome)
}

/// Parses and applies one `DRAW_COPY` message. `Some(true)` if the canvas
/// changed and a frame should go out; `Some(false)` if the draw referenced
/// an image type/pixel format this crate doesn't decode (compressed images,
/// indexed palettes — skipped, not an error); `None` if the message was too
/// malformed to parse at all (also skipped, not fatal to the session).
fn handle_draw_copy(payload: &[u8], canvas: &mut Framebuffer) -> Option<bool> {
    let base = messages::DrawBase::read(payload).ok()?;
    let copy = wire::SpiceCopy::read(&payload[base.end_offset..]).ok()?;

    let image_start = copy.src_bitmap as usize;
    let descriptor_bytes = payload.get(image_start..)?;
    let descriptor = messages::ImageDescriptor::read(descriptor_bytes).ok()?;
    if descriptor.image_type != ImageType::Pixmap as u8 {
        return Some(false);
    }

    let bitmap_start = image_start + messages::ImageDescriptor::SIZE;
    let bitmap_bytes = payload.get(bitmap_start..)?;
    let bitmap = wire::SpiceBitmap::read(bitmap_bytes).ok()?;
    let format = wire::bitmap_pixel_format(bitmap.format)?;

    let pixel_start = bitmap_start + wire::SpiceBitmap::SIZE;
    let pixels = payload.get(pixel_start..)?;

    // Honor the source sub-rectangle (src_area) rather than assuming the
    // bitmap always starts at its own origin — the guest can reference a
    // sub-region of a larger cached bitmap.
    let src_w = copy.src_right.saturating_sub(copy.src_left);
    let src_h = copy.src_bottom.saturating_sub(copy.src_top);
    let w = base
        .right
        .saturating_sub(base.left)
        .min(src_w)
        .min(bitmap.width.saturating_sub(copy.src_left));
    let h = base
        .bottom
        .saturating_sub(base.top)
        .min(src_h)
        .min(bitmap.height.saturating_sub(copy.src_top));

    let row_offset = (copy.src_top as usize).checked_mul(bitmap.stride as usize)?;
    let col_offset = (copy.src_left as usize).checked_mul(format.bytes_per_pixel())?;
    let pixels = pixels.get(row_offset.checked_add(col_offset)?..)?;

    canvas.apply_pixmap(
        framebuffer::PixmapBlit {
            x: base.left,
            y: base.top,
            w,
            h,
            stride: bitmap.stride,
            format,
        },
        pixels,
    );
    Some(true)
}

async fn disconnect(event_tx: &mpsc::Sender<SpiceEvent>, reason: Option<String>) {
    let _ = event_tx.send(SpiceEvent::Disconnected { reason }).await;
}

/// Tracked pointer state; SPICE's `MousePosition`/`MouseButton` always carry
/// the full position/button-mask state, not deltas.
#[derive(Default)]
struct PointerState {
    x: u32,
    y: u32,
    buttons: u16,
}

fn button_mask(button: MouseButton) -> u16 {
    match button {
        MouseButton::Left => 0x01,
        MouseButton::Middle => 0x02,
        MouseButton::Right => 0x04,
    }
}

fn update_pointer_moved(pointer: &mut PointerState, x: f32, y: f32) -> messages::MousePosition {
    pointer.x = x.max(0.0) as u32;
    pointer.y = y.max(0.0) as u32;
    messages::MousePosition {
        x: pointer.x,
        y: pointer.y,
        buttons: pointer.buttons,
        display_id: 0,
    }
}

fn update_pointer_button(
    pointer: &mut PointerState,
    button: MouseButton,
    pressed: bool,
) -> messages::MouseButton {
    let mask = button_mask(button);
    if pressed {
        pointer.buttons |= mask;
    } else {
        pointer.buttons &= !mask;
    }
    messages::MouseButton {
        button: mask as u8,
        buttons_state: pointer.buttons,
    }
}

/// The ordered (scan code, is-key-down) pairs to send for one key
/// press/release, applying shift-bracketing (for characters that need Shift
/// on a US layout) and the extended `0xE0` prefix. Empty if `keysym` isn't
/// in `keymap`'s table — dropped rather than sent as a garbage scan code.
fn key_scancodes(keysym: u32, pressed: bool) -> Vec<(u32, bool)> {
    let Some(translation) = keymap::keysym_to_scancode(keysym) else {
        return Vec::new();
    };

    let mut events = Vec::new();
    if translation.shift && pressed {
        push_scancode(
            &mut events,
            keymap::ScanCode::Simple(keymap::LEFT_SHIFT),
            true,
        );
        push_scancode(&mut events, translation.code, true);
    } else if translation.shift {
        push_scancode(&mut events, translation.code, false);
        push_scancode(
            &mut events,
            keymap::ScanCode::Simple(keymap::LEFT_SHIFT),
            false,
        );
    } else {
        push_scancode(&mut events, translation.code, pressed);
    }
    events
}

fn push_scancode(events: &mut Vec<(u32, bool)>, code: keymap::ScanCode, pressed: bool) {
    if let keymap::ScanCode::Extended(_) = code {
        events.push((0xE0, pressed));
    }
    let byte = match code {
        keymap::ScanCode::Simple(b) | keymap::ScanCode::Extended(b) => b,
    };
    events.push((keymap::make_code_byte(byte, pressed) as u32, pressed));
}

async fn apply_input(stream: &mut OwnedWriteHalf, pointer: &mut PointerState, event: SpiceInput) {
    use shakenfist_spice_protocol::inputs_client;

    let result: io::Result<()> = async {
        match event {
            SpiceInput::PointerMoved { x, y } => {
                let mut buf = Vec::new();
                update_pointer_moved(pointer, x, y).write(&mut buf)?;
                wire::write_message(stream, inputs_client::MOUSE_POSITION, &buf).await
            }
            SpiceInput::PointerButton { button, pressed } => {
                let mut buf = Vec::new();
                update_pointer_button(pointer, button, pressed).write(&mut buf)?;
                let opcode = if pressed {
                    inputs_client::MOUSE_PRESS
                } else {
                    inputs_client::MOUSE_RELEASE
                };
                wire::write_message(stream, opcode, &buf).await
            }
            SpiceInput::Key { keysym, pressed } => {
                for (scancode, key_down) in key_scancodes(keysym, pressed) {
                    let opcode = if key_down {
                        inputs_client::KEY_DOWN
                    } else {
                        inputs_client::KEY_UP
                    };
                    let mut buf = Vec::new();
                    messages::KeyEvent { scancode }.write(&mut buf)?;
                    wire::write_message(stream, opcode, &buf).await?;
                }
                Ok(())
            }
        }
    }
    .await;
    // A failed write here means the session is dying; the next read on any
    // channel in run_session's select! will observe the same failure and
    // disconnect, so this is silently dropped rather than duplicating that.
    let _ = result;
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn connect_to_closed_port_is_a_transport_error() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let result = connect(ConnectOptions {
            host: "127.0.0.1".into(),
            port,
            password: None,
        })
        .await;

        match result {
            Err(SpiceConnectError::Transport(_)) => {}
            Err(other) => panic!("expected a transport error, got {other:?}"),
            Ok(_) => panic!("expected the connection to be refused"),
        }
    }

    #[test]
    fn io_error_during_handshake_is_a_transport_error() {
        let io_err = io::Error::new(io::ErrorKind::ConnectionReset, "reset");
        match map_handshake_error(anyhow::Error::from(io_err)) {
            SpiceConnectError::Transport(_) => {}
            other => panic!("expected a transport error, got {other:?}"),
        }
    }

    #[test]
    fn authentication_failed_message_is_an_auth_error() {
        let err = anyhow::anyhow!("Authentication failed: {:?}", SpiceError::PermissionDenied);
        match map_handshake_error(err) {
            SpiceConnectError::Auth(_) => {}
            other => panic!("expected an auth error, got {other:?}"),
        }
    }

    #[test]
    fn other_anyhow_errors_are_protocol_errors() {
        let err = anyhow::anyhow!("something else went wrong");
        match map_handshake_error(err) {
            SpiceConnectError::Protocol(_) => {}
            other => panic!("expected a protocol error, got {other:?}"),
        }
    }

    #[test]
    fn need_secured_link_error_is_a_protocol_error() {
        match link_error_to_connect_error(SpiceError::NeedSecured) {
            Some(SpiceConnectError::Protocol(_)) => {}
            other => panic!("expected a protocol error, got {other:?}"),
        }
    }

    #[test]
    fn other_spice_link_errors_are_protocol_errors() {
        for error in [
            SpiceError::InvalidMagic,
            SpiceError::VersionMismatch,
            SpiceError::ChannelUnavailable,
        ] {
            match link_error_to_connect_error(error) {
                Some(SpiceConnectError::Protocol(_)) => {}
                other => panic!("expected a protocol error for {error:?}, got {other:?}"),
            }
        }
    }

    #[test]
    fn ok_link_result_is_not_an_error() {
        assert!(link_error_to_connect_error(SpiceError::Ok).is_none());
    }

    #[test]
    fn pointer_moved_updates_position_and_keeps_current_buttons() {
        let mut pointer = PointerState {
            x: 0,
            y: 0,
            buttons: 0x02,
        };
        let msg = update_pointer_moved(&mut pointer, 12.0, 34.0);
        assert_eq!((pointer.x, pointer.y, pointer.buttons), (12, 34, 0x02));
        assert_eq!((msg.x, msg.y, msg.buttons), (12, 34, 0x02));
    }

    #[test]
    fn negative_pointer_position_clamps_to_zero() {
        let mut pointer = PointerState::default();
        update_pointer_moved(&mut pointer, -5.0, -1.0);
        assert_eq!((pointer.x, pointer.y), (0, 0));
    }

    #[test]
    fn pointer_buttons_set_and_clear_the_right_bit() {
        let mut pointer = PointerState::default();
        update_pointer_button(&mut pointer, MouseButton::Left, true);
        assert_eq!(pointer.buttons, 0x01);
        update_pointer_button(&mut pointer, MouseButton::Right, true);
        assert_eq!(pointer.buttons, 0x05);
        update_pointer_button(&mut pointer, MouseButton::Left, false);
        assert_eq!(pointer.buttons, 0x04);
    }

    #[test]
    fn key_event_uses_the_translated_scancode_not_the_keysym() {
        // 'a' (keysym 0x61) -> scan code 0x1E, not the raw keysym.
        assert_eq!(key_scancodes(0x61, true), vec![(0x1E, true)]);
    }

    #[test]
    fn shifted_key_brackets_a_synthetic_shift() {
        // 'A' (keysym 0x41) needs Shift held for the duration of the press.
        assert_eq!(key_scancodes(0x41, true), vec![(0x2A, true), (0x1E, true)]);
        assert_eq!(
            key_scancodes(0x41, false),
            vec![(0x9E, false), (0xAA, false)]
        );
    }

    #[test]
    fn extended_key_sends_e0_prefix_before_the_code() {
        assert_eq!(
            key_scancodes(0xFF51, true),
            vec![(0xE0, true), (0x4B, true)]
        ); // ArrowLeft
        assert_eq!(
            key_scancodes(0xFF51, false),
            vec![(0xE0, false), (0xCB, false)]
        );
    }

    #[test]
    fn unmapped_keysym_produces_no_scancodes() {
        assert_eq!(key_scancodes(0xFF61, true), Vec::new()); // PrintScreen
    }
}
