//! SPICE connector.
//!
//! Wraps [`spice-rs`](spice_rs): it connects and does all the protocol-level
//! work (link handshake, display-channel decoding, input translation); this
//! crate is pure translation between `spice-rs`'s own generic event/input
//! types and `server-remote-core`'s, the same relationship
//! `server-remote-vnc` has with `vnc-rs`.

use std::borrow::Cow;

use server_remote_core::{
    Action, Capabilities, ConnectError, Connector, Control, Frame, InputEvent, PointerButton,
    PointerMode, SessionCommand, SessionEvent, SessionHandle, Target, TlsPolicy,
};
use tokio::sync::mpsc;

/// Declarative action id for sending Ctrl+Alt+Del.
const CAD_ACTION: &str = "ctrl-alt-del";

/// Connects to a SPICE server. Holds the password (if any) used for ticket
/// auth; servers requiring no password ignore it.
pub struct SpiceConnector {
    password: Option<String>,
}

impl SpiceConnector {
    pub fn new(password: Option<String>) -> Self {
        Self { password }
    }
}

impl Connector for SpiceConnector {
    fn name(&self) -> &'static str {
        "SPICE"
    }

    async fn connect(
        &self,
        target: &Target,
        _policy: &TlsPolicy,
    ) -> Result<SessionHandle, ConnectError> {
        // spice-rs speaks plain TCP only (no TLS support yet), so the
        // policy is ignored here, same as VNC.
        let options = spice_rs::ConnectOptions {
            host: target.host.clone(),
            port: target.port,
            password: self.password.clone(),
        };

        let handle = spice_rs::connect(options).await.map_err(connect_error)?;

        let (event_tx, events) = mpsc::channel(16);
        let (input, input_rx) = mpsc::channel(256);
        let (commands, command_rx) = mpsc::channel(16);

        tokio::spawn(forward_session(handle, event_tx, input_rx, command_rx));

        Ok(SessionHandle {
            events,
            input,
            commands,
        })
    }
}

fn connect_error(error: spice_rs::SpiceConnectError) -> ConnectError {
    match error {
        spice_rs::SpiceConnectError::Transport(io) => ConnectError::Transport(io),
        spice_rs::SpiceConnectError::Auth(message) => ConnectError::Auth(message),
        spice_rs::SpiceConnectError::Protocol(message) => ConnectError::Protocol(message),
    }
}

fn capabilities() -> Capabilities {
    Capabilities {
        pointer: PointerMode::Absolute,
        actions: vec![Action {
            id: Cow::Borrowed(CAD_ACTION),
            label: Cow::Borrowed("Ctrl+Alt+Del"),
            control: Control::Button,
        }],
    }
}

/// Pumps `spice-rs`'s session channels, translating in both directions,
/// until either side closes.
async fn forward_session(
    handle: spice_rs::SpiceHandle,
    event_tx: mpsc::Sender<SessionEvent>,
    mut input_rx: mpsc::Receiver<InputEvent>,
    mut command_rx: mpsc::Receiver<SessionCommand>,
) {
    let spice_rs::SpiceHandle {
        mut events,
        input: spice_input,
    } = handle;
    let mut commands_open = true;

    loop {
        tokio::select! {
            event = events.recv() => match event {
                Some(spice_event) => {
                    if event_tx.send(translate_event(spice_event)).await.is_err() {
                        return;
                    }
                }
                None => return,
            },
            incoming = input_rx.recv() => match incoming {
                Some(event) => {
                    if let Some(input) = translate_input(event) {
                        let _ = spice_input.send(input).await;
                    }
                }
                None => return,
            },
            command = command_rx.recv(), if commands_open => match command {
                Some(SessionCommand::Action { id }) if id == CAD_ACTION => send_cad(&spice_input).await,
                Some(_) => {}
                None => commands_open = false,
            },
        }
    }
}

fn translate_event(event: spice_rs::SpiceEvent) -> SessionEvent {
    match event {
        spice_rs::SpiceEvent::Connected { width, height } => SessionEvent::Connected {
            size: (width, height),
            capabilities: capabilities(),
        },
        spice_rs::SpiceEvent::Frame(frame) => SessionEvent::Frame(Frame {
            width: frame.width,
            height: frame.height,
            pixels: frame.pixels,
        }),
        spice_rs::SpiceEvent::Resized { width, height } => SessionEvent::Resized { width, height },
        spice_rs::SpiceEvent::Disconnected { reason } => SessionEvent::Disconnected { reason },
    }
}

/// Translates an [`InputEvent`] to `spice-rs`'s own input type.
/// `PointerMotion` (relative) has no `spice-rs` equivalent yet — dropped,
/// same as VNC's precedent for the symmetric case. `PointerButton` for a
/// button SPICE has no wire representation for (see [`translate_button`])
/// is dropped the same way.
fn translate_input(event: InputEvent) -> Option<spice_rs::SpiceInput> {
    match event {
        InputEvent::PointerMoved { x, y } => Some(spice_rs::SpiceInput::PointerMoved { x, y }),
        InputEvent::PointerButton { button, pressed } => translate_button(button)
            .map(|button| spice_rs::SpiceInput::PointerButton { button, pressed }),
        InputEvent::Key { keysym, pressed } => Some(spice_rs::SpiceInput::Key { keysym, pressed }),
        InputEvent::PointerMotion { .. } => None,
    }
}

/// SPICE's `SPICE_MOUSE_BUTTON_*` enum only defines left/middle/right (plus
/// the wheel, which arrives as its own message, not a button) — there's no
/// slot for the mouse's side buttons, so this connector doesn't support them
/// yet rather than invent a mapping.
fn translate_button(button: PointerButton) -> Option<spice_rs::MouseButton> {
    match button {
        PointerButton::Left => Some(spice_rs::MouseButton::Left),
        PointerButton::Middle => Some(spice_rs::MouseButton::Middle),
        PointerButton::Right => Some(spice_rs::MouseButton::Right),
        PointerButton::Back | PointerButton::Forward => None,
    }
}

async fn send_cad(input: &mpsc::Sender<spice_rs::SpiceInput>) {
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
    for (keysym, pressed) in sequence {
        let _ = input
            .send(spice_rs::SpiceInput::Key { keysym, pressed })
            .await;
    }
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

        let target = Target::new("127.0.0.1", port);
        let result = SpiceConnector::new(None)
            .connect(&target, &TlsPolicy::default())
            .await;

        match result {
            Err(ConnectError::Transport(_)) => {}
            Err(other) => panic!("expected a transport error, got {other:?}"),
            Ok(_) => panic!("expected the connection to be refused"),
        }
    }

    #[test]
    fn spice_connect_error_maps_to_the_matching_connect_error() {
        assert!(matches!(
            connect_error(spice_rs::SpiceConnectError::Transport(
                std::io::Error::from(std::io::ErrorKind::ConnectionReset)
            )),
            ConnectError::Transport(_)
        ));
        assert!(matches!(
            connect_error(spice_rs::SpiceConnectError::Auth("bad ticket".into())),
            ConnectError::Auth(_)
        ));
        assert!(matches!(
            connect_error(spice_rs::SpiceConnectError::Protocol("bad magic".into())),
            ConnectError::Protocol(_)
        ));
    }

    #[test]
    fn capabilities_report_absolute_pointer_and_ctrl_alt_del() {
        let caps = capabilities();
        assert_eq!(caps.pointer, PointerMode::Absolute);
        assert_eq!(caps.actions.len(), 1);
        assert_eq!(caps.actions[0].id, CAD_ACTION);
        assert!(matches!(caps.actions[0].control, Control::Button));
    }

    #[test]
    fn spice_event_connected_maps_to_session_event_connected() {
        match translate_event(spice_rs::SpiceEvent::Connected {
            width: 800,
            height: 600,
        }) {
            SessionEvent::Connected { size, capabilities } => {
                assert_eq!(size, (800, 600));
                assert_eq!(capabilities.pointer, PointerMode::Absolute);
            }
            other => panic!("expected a connected event, got {other:?}"),
        }
    }

    #[test]
    fn spice_event_frame_maps_straight_through() {
        let frame = spice_rs::Frame {
            width: 2,
            height: 1,
            pixels: vec![1, 2, 3, 4, 5, 6, 7, 8],
        };
        match translate_event(spice_rs::SpiceEvent::Frame(frame)) {
            SessionEvent::Frame(frame) => {
                assert_eq!((frame.width, frame.height), (2, 1));
                assert_eq!(frame.pixels, vec![1, 2, 3, 4, 5, 6, 7, 8]);
            }
            other => panic!("expected a frame event, got {other:?}"),
        }
    }

    #[test]
    fn spice_event_disconnected_maps_straight_through() {
        match translate_event(spice_rs::SpiceEvent::Disconnected {
            reason: Some("reset".into()),
        }) {
            SessionEvent::Disconnected { reason } => assert_eq!(reason, Some("reset".to_owned())),
            other => panic!("expected a disconnected event, got {other:?}"),
        }
    }

    #[test]
    fn input_event_pointer_and_key_map_to_spice_input() {
        match translate_input(InputEvent::PointerMoved { x: 1.0, y: 2.0 }) {
            Some(spice_rs::SpiceInput::PointerMoved { x, y }) => assert_eq!((x, y), (1.0, 2.0)),
            other => panic!("expected a pointer moved input, got {other:?}"),
        }

        match translate_input(InputEvent::PointerButton {
            button: PointerButton::Right,
            pressed: true,
        }) {
            Some(spice_rs::SpiceInput::PointerButton { button, pressed }) => {
                assert!(matches!(button, spice_rs::MouseButton::Right));
                assert!(pressed);
            }
            other => panic!("expected a pointer button input, got {other:?}"),
        }

        match translate_input(InputEvent::Key {
            keysym: 0xFF1B,
            pressed: true,
        }) {
            Some(spice_rs::SpiceInput::Key { keysym, pressed }) => {
                assert_eq!(keysym, 0xFF1B);
                assert!(pressed);
            }
            other => panic!("expected a key input, got {other:?}"),
        }
    }

    #[test]
    fn pointer_motion_has_no_spice_input_equivalent_and_is_dropped() {
        let translated = translate_input(InputEvent::PointerMotion { dx: 1.0, dy: 1.0 });
        assert!(translated.is_none());
    }

    #[test]
    fn back_and_forward_buttons_have_no_spice_wire_slot_and_are_dropped() {
        assert!(translate_button(PointerButton::Back).is_none());
        assert!(translate_button(PointerButton::Forward).is_none());

        let translated = translate_input(InputEvent::PointerButton {
            button: PointerButton::Back,
            pressed: true,
        });
        assert!(translated.is_none());
    }

    #[tokio::test]
    async fn action_ctrl_alt_del_sends_the_synthetic_key_sequence() {
        let (input, mut input_rx) = mpsc::channel(16);
        send_cad(&input).await;

        let mut received = Vec::new();
        while let Ok(event) = input_rx.try_recv() {
            match event {
                spice_rs::SpiceInput::Key { keysym, pressed } => received.push((keysym, pressed)),
                other => panic!("expected only key events, got {other:?}"),
            }
        }

        assert_eq!(
            received,
            vec![
                (0xFFE3, true),
                (0xFFE9, true),
                (0xFFFF, true),
                (0xFFFF, false),
                (0xFFE9, false),
                (0xFFE3, false),
            ]
        );
    }
}
