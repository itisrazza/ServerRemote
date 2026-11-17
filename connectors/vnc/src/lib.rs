//! VNC (RFB) connector.
//!
//! Wraps [`vnc-rs`](https://crates.io/crates/vnc-rs): it connects and decodes
//! the protocol, emitting incremental rectangle updates; this crate composites
//! those into a [`Framebuffer`] and bridges its event/input model onto the
//! session channels.

mod framebuffer;

use std::time::Duration;

use framebuffer::Framebuffer;
use server_remote_core::{
    Action, Capabilities, ConnectError, Connector, InputEvent, PointerButton, PointerMode,
    SessionCommand, SessionEvent, SessionHandle, Target, TlsPolicy,
};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use vnc::{
    ClientKeyEvent, ClientMouseEvent, PixelFormat, VncClient, VncConnector as VncBuilder,
    VncEncoding, VncError, VncEvent, X11Event,
};

/// Declarative action id for sending Ctrl+Alt+Del.
const CAD_ACTION: &str = "ctrl-alt-del";
/// How often to request an incremental framebuffer update (~30 fps).
const FRAME_INTERVAL: Duration = Duration::from_millis(33);

/// Connects to a VNC/RFB server. Holds the password (if any) used for VNC
/// authentication; servers offering no-auth ignore it.
pub struct VncConnector {
    password: Option<String>,
}

impl VncConnector {
    pub fn new(password: Option<String>) -> Self {
        Self { password }
    }
}

impl Connector for VncConnector {
    fn name(&self) -> &'static str {
        "VNC"
    }

    async fn connect(
        &self,
        target: &Target,
        _policy: &TlsPolicy,
    ) -> Result<SessionHandle, ConnectError> {
        // vnc-rs only speaks None / VNC auth over plain TCP; TLS variants
        // (VeNCrypt) aren't supported, so the policy is ignored here.
        let stream = TcpStream::connect((target.host.as_str(), target.port))
            .await
            .map_err(ConnectError::Transport)?;

        let password = self.password.clone();
        let client = VncBuilder::new(stream)
            .set_auth_method(async move { password.ok_or(VncError::NoPassword) })
            .add_encoding(VncEncoding::Raw)
            .add_encoding(VncEncoding::CopyRect)
            .allow_shared(true)
            .set_pixel_format(PixelFormat::rgba())
            .build()
            .map_err(connect_error)?
            .try_start()
            .await
            .map_err(connect_error)?
            .finish()
            .map_err(connect_error)?;

        let (event_tx, events) = mpsc::channel(16);
        let (input, input_rx) = mpsc::channel(256);
        let (commands, command_rx) = mpsc::channel(16);

        tokio::spawn(run_session(client, event_tx, input_rx, command_rx));

        Ok(SessionHandle {
            events,
            input,
            commands,
        })
    }
}

fn connect_error(error: VncError) -> ConnectError {
    match error {
        VncError::NoPassword | VncError::WrongPassword => ConnectError::Auth(error.to_string()),
        VncError::IoError(io) => ConnectError::Transport(io),
        other => ConnectError::Protocol(other.to_string()),
    }
}

fn capabilities() -> Capabilities {
    Capabilities {
        pointer: PointerMode::Absolute,
        input_actions: vec![Action::button(CAD_ACTION, "Ctrl+Alt+Del")],
        ..Capabilities::default()
    }
}

/// The current pointer state; VNC's `PointerEvent` always carries the full
/// position and button mask, so we track both across events.
#[derive(Default)]
struct Pointer {
    x: u16,
    y: u16,
    buttons: u8,
}

async fn run_session(
    client: VncClient,
    event_tx: mpsc::Sender<SessionEvent>,
    mut input_rx: mpsc::Receiver<InputEvent>,
    mut command_rx: mpsc::Receiver<SessionCommand>,
) {
    let mut fb: Option<Framebuffer> = None;
    let mut pointer = Pointer::default();
    let mut connected = false;
    let mut commands_open = true;
    let mut refresh = tokio::time::interval(FRAME_INTERVAL);

    loop {
        // Drain everything the decoder has produced, then snapshot once.
        let mut dirty = false;
        loop {
            match client.poll_event().await {
                Ok(Some(event)) => {
                    match handle_event(event, &client, &event_tx, &mut fb, &mut connected).await {
                        EventOutcome::Redraw => dirty = true,
                        EventOutcome::Nothing => {}
                        EventOutcome::Stop => return,
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    disconnect(&event_tx, Some(error.to_string())).await;
                    return;
                }
            }
        }
        if dirty
            && let Some(fb) = &fb
            && event_tx
                .send(SessionEvent::Frame(fb.to_frame()))
                .await
                .is_err()
        {
            return;
        }

        tokio::select! {
            _ = refresh.tick() => {
                if connected && client.input(X11Event::Refresh).await.is_err() {
                    disconnect(&event_tx, None).await;
                    return;
                }
            }
            input = input_rx.recv() => match input {
                Some(event) => apply_input(&client, event, &mut pointer).await,
                None => return,
            },
            command = command_rx.recv(), if commands_open => match command {
                Some(SessionCommand::Action { id, .. }) if id == CAD_ACTION => send_cad(&client).await,
                Some(_) => {}
                None => commands_open = false,
            },
        }
    }
}

enum EventOutcome {
    Redraw,
    Nothing,
    Stop,
}

async fn handle_event(
    event: VncEvent,
    client: &VncClient,
    event_tx: &mpsc::Sender<SessionEvent>,
    fb: &mut Option<Framebuffer>,
    connected: &mut bool,
) -> EventOutcome {
    match event {
        VncEvent::SetResolution(screen) => {
            let (width, height) = (screen.width as u32, screen.height as u32);
            *fb = Some(Framebuffer::new(width, height));
            let event = if *connected {
                SessionEvent::Resized { width, height }
            } else {
                *connected = true;
                SessionEvent::Connected {
                    size: (width, height),
                    capabilities: capabilities(),
                }
            };
            if event_tx.send(event).await.is_err() {
                return EventOutcome::Stop;
            }
            // Ask for the whole screen now that the canvas is sized.
            let _ = client.input(X11Event::FullRefresh).await;
            EventOutcome::Nothing
        }
        VncEvent::RawImage(rect, data) => {
            if let Some(fb) = fb {
                fb.apply_raw(
                    rect.x as u32,
                    rect.y as u32,
                    rect.width as u32,
                    rect.height as u32,
                    &data,
                );
            }
            EventOutcome::Redraw
        }
        VncEvent::Copy(dst, src) => {
            if let Some(fb) = fb {
                fb.apply_copy(
                    dst.x as u32,
                    dst.y as u32,
                    src.x as u32,
                    src.y as u32,
                    src.width as u32,
                    src.height as u32,
                );
            }
            EventOutcome::Redraw
        }
        VncEvent::Error(message) => {
            disconnect(event_tx, Some(message)).await;
            EventOutcome::Stop
        }
        // Bell, clipboard, cursor and JPEG (Tight) aren't requested/handled yet.
        _ => EventOutcome::Nothing,
    }
}

async fn apply_input(client: &VncClient, event: InputEvent, pointer: &mut Pointer) {
    if let Some(x11) = update_pointer(pointer, event) {
        let _ = client.input(x11).await;
    }
}

/// Applies an [`InputEvent`] to the tracked pointer state, returning the RFB
/// event to send (`None` for [`InputEvent::PointerMotion`], which VNC's
/// absolute pointer model can't express).
fn update_pointer(pointer: &mut Pointer, event: InputEvent) -> Option<X11Event> {
    let x11 = match event {
        // TODO: widget coordinates need scaling to framebuffer coordinates.
        InputEvent::PointerMoved { x, y } => {
            pointer.x = x.max(0.0) as u16;
            pointer.y = y.max(0.0) as u16;
            pointer_event(pointer)
        }
        InputEvent::PointerButton { button, pressed } => {
            let mask = match button {
                PointerButton::Left => 0b001,
                PointerButton::Middle => 0b010,
                PointerButton::Right => 0b100,
                // RFB's core pointer-event button mask has no spec-guaranteed
                // bit for the side buttons (some servers repurpose bits 5-7
                // by convention, but there's nothing to reliably target), so
                // this connector doesn't support them yet rather than guess.
                PointerButton::Back | PointerButton::Forward => return None,
            };
            if pressed {
                pointer.buttons |= mask;
            } else {
                pointer.buttons &= !mask;
            }
            pointer_event(pointer)
        }
        // VNC is absolute; relative motion doesn't apply.
        InputEvent::PointerMotion { .. } => return None,
        InputEvent::Key { keysym, pressed } => X11Event::KeyEvent(ClientKeyEvent {
            keycode: keysym,
            down: pressed,
        }),
    };
    Some(x11)
}

fn pointer_event(pointer: &Pointer) -> X11Event {
    X11Event::PointerEvent(ClientMouseEvent {
        position_x: pointer.x,
        position_y: pointer.y,
        bottons: pointer.buttons,
    })
}

async fn send_cad(client: &VncClient) {
    const CTRL_L: u32 = 0xFFE3;
    const ALT_L: u32 = 0xFFE9;
    const DELETE: u32 = 0xFFFF;
    let sequence = [
        (CTRL_L, true),
        (ALT_L, true),
        (DELETE, true),
        (DELETE, false),
        (ALT_L, false),
        (CTRL_L, false),
    ];
    for (keycode, down) in sequence {
        let _ = client
            .input(X11Event::KeyEvent(ClientKeyEvent { keycode, down }))
            .await;
    }
}

async fn disconnect(event_tx: &mpsc::Sender<SessionEvent>, reason: Option<String>) {
    let _ = event_tx.send(SessionEvent::Disconnected { reason }).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use server_remote_core::Control;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn connect_to_closed_port_is_a_transport_error() {
        // Reserve a port, then free it so the connection is refused.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let target = Target::new("127.0.0.1", port);
        let result = VncConnector::new(None)
            .connect(&target, &TlsPolicy::default())
            .await;

        match result {
            Err(ConnectError::Transport(_)) => {}
            Err(other) => panic!("expected a transport error, got {other:?}"),
            Ok(_) => panic!("expected the connection to be refused"),
        }
    }

    #[test]
    fn no_password_or_wrong_password_is_an_auth_error() {
        assert!(matches!(
            connect_error(VncError::NoPassword),
            ConnectError::Auth(_)
        ));
        assert!(matches!(
            connect_error(VncError::WrongPassword),
            ConnectError::Auth(_)
        ));
    }

    #[test]
    fn io_error_is_a_transport_error() {
        let io = std::io::Error::from(std::io::ErrorKind::ConnectionReset);
        assert!(matches!(
            connect_error(VncError::IoError(io)),
            ConnectError::Transport(_)
        ));
    }

    #[test]
    fn other_vnc_errors_are_protocol_errors() {
        assert!(matches!(
            connect_error(VncError::NoEncoding),
            ConnectError::Protocol(_)
        ));
    }

    #[test]
    fn capabilities_report_absolute_pointer_and_ctrl_alt_del() {
        let caps = capabilities();
        assert_eq!(caps.pointer, PointerMode::Absolute);
        assert_eq!(caps.input_actions.len(), 1);
        assert_eq!(caps.input_actions[0].id, CAD_ACTION);
        assert!(matches!(caps.input_actions[0].control, Control::Button));
    }

    #[test]
    fn pointer_moved_updates_position_and_keeps_current_buttons() {
        let mut pointer = Pointer {
            x: 0,
            y: 0,
            buttons: 0b010,
        };
        let event = update_pointer(&mut pointer, InputEvent::PointerMoved { x: 12.0, y: 34.0 });

        assert_eq!(pointer.x, 12);
        assert_eq!(pointer.y, 34);
        assert_eq!(pointer.buttons, 0b010);
        match event {
            Some(X11Event::PointerEvent(mouse)) => {
                assert_eq!(
                    (mouse.position_x, mouse.position_y, mouse.bottons),
                    (12, 34, 0b010)
                );
            }
            other => panic!("expected a pointer event, got {other:?}"),
        }
    }

    #[test]
    fn negative_pointer_position_clamps_to_zero() {
        let mut pointer = Pointer::default();
        update_pointer(&mut pointer, InputEvent::PointerMoved { x: -5.0, y: -1.0 });
        assert_eq!((pointer.x, pointer.y), (0, 0));
    }

    #[test]
    fn pointer_buttons_set_and_clear_the_right_bit() {
        let mut pointer = Pointer::default();

        update_pointer(
            &mut pointer,
            InputEvent::PointerButton {
                button: PointerButton::Left,
                pressed: true,
            },
        );
        assert_eq!(pointer.buttons, 0b001);

        update_pointer(
            &mut pointer,
            InputEvent::PointerButton {
                button: PointerButton::Right,
                pressed: true,
            },
        );
        assert_eq!(pointer.buttons, 0b101);

        update_pointer(
            &mut pointer,
            InputEvent::PointerButton {
                button: PointerButton::Left,
                pressed: false,
            },
        );
        assert_eq!(pointer.buttons, 0b100);
    }

    #[test]
    fn back_and_forward_buttons_are_not_representable_over_vnc() {
        let mut pointer = Pointer::default();
        let event = update_pointer(
            &mut pointer,
            InputEvent::PointerButton {
                button: PointerButton::Back,
                pressed: true,
            },
        );
        assert!(event.is_none());
        // No bit was set for the unsupported button, either.
        assert_eq!(pointer.buttons, 0);
    }

    #[test]
    fn pointer_motion_is_not_representable_over_vnc() {
        let mut pointer = Pointer::default();
        let event = update_pointer(&mut pointer, InputEvent::PointerMotion { dx: 1.0, dy: 1.0 });
        assert!(event.is_none());
    }

    #[test]
    fn key_event_carries_the_keysym_as_the_keycode() {
        let mut pointer = Pointer::default();
        let event = update_pointer(
            &mut pointer,
            InputEvent::Key {
                keysym: 0xFF0D,
                pressed: true,
            },
        );
        match event {
            Some(X11Event::KeyEvent(key)) => {
                assert_eq!((key.keycode, key.down), (0xFF0D, true));
            }
            other => panic!("expected a key event, got {other:?}"),
        }
    }
}
