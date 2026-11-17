//! A self-contained connector that streams a moving gradient.
//!
//! It talks to nothing, so it's useful for exercising the UI and the session
//! plumbing without a real server.

use std::borrow::Cow;
use std::time::Duration;

use tokio::sync::mpsc;

use crate::{
    Action, Capabilities, ConnectError, Connector, Control, Frame, PointerMode, SessionCommand,
    SessionEvent, SessionHandle, Target, TlsPolicy,
};

const WIDTH: u32 = 320;
const HEIGHT: u32 = 240;
const FRAME_INTERVAL: Duration = Duration::from_millis(50);

/// Id of the one declarative action the dummy advertises.
const RESET_ACTION: &str = "reset";

pub struct DummyConnector;

impl Connector for DummyConnector {
    fn name(&self) -> &'static str {
        "Dummy"
    }

    async fn connect(
        &self,
        _target: &Target,
        _policy: &TlsPolicy,
    ) -> Result<SessionHandle, ConnectError> {
        let (event_tx, events) = mpsc::channel(16);
        let (input, mut input_rx) = mpsc::channel(64);
        let (commands, mut command_rx) = mpsc::channel::<SessionCommand>(16);

        tokio::spawn(async move {
            let connected = SessionEvent::Connected {
                size: (WIDTH, HEIGHT),
                capabilities: capabilities(),
            };
            if event_tx.send(connected).await.is_err() {
                return;
            }

            let mut tick = tokio::time::interval(FRAME_INTERVAL);
            let mut frame_no: u32 = 0;
            let mut input_open = true;
            let mut commands_open = true;
            loop {
                tokio::select! {
                    _ = tick.tick() => {
                        let frame = gradient_frame(frame_no);
                        frame_no = frame_no.wrapping_add(1);
                        if event_tx.send(SessionEvent::Frame(frame)).await.is_err() {
                            break;
                        }
                    }
                    // Drain input; the dummy ignores it beyond noticing hangup.
                    incoming = input_rx.recv(), if input_open => {
                        if incoming.is_none() {
                            input_open = false;
                        }
                    }
                    command = command_rx.recv(), if commands_open => match command {
                        Some(SessionCommand::Action { id }) if id == RESET_ACTION => {
                            frame_no = 0;
                        }
                        Some(SessionCommand::Action { .. }) => {}
                        None => commands_open = false,
                    },
                }

                // The UI has hung up every request channel; wind down.
                if !input_open && !commands_open {
                    break;
                }
            }
        });

        Ok(SessionHandle {
            events,
            input,
            commands,
        })
    }
}

fn capabilities() -> Capabilities {
    Capabilities {
        pointer: PointerMode::Absolute,
        actions: vec![Action {
            id: Cow::Borrowed(RESET_ACTION),
            label: Cow::Borrowed("Reset gradient"),
            control: Control::Button,
        }],
    }
}

fn gradient_frame(frame_no: u32) -> Frame {
    let mut pixels = vec![0u8; (WIDTH * HEIGHT * 4) as usize];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let i = ((y * WIDTH + x) * 4) as usize;
            pixels[i] = x.wrapping_add(frame_no) as u8; // R
            pixels[i + 1] = y as u8; // G
            pixels[i + 2] = frame_no as u8; // B
            pixels[i + 3] = 255; // A
        }
    }
    Frame {
        width: WIDTH,
        height: HEIGHT,
        pixels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn connect() -> SessionHandle {
        DummyConnector
            .connect(&Target::new("localhost", 0), &TlsPolicy::default())
            .await
            .expect("dummy connect never fails")
    }

    /// R channel of the top-left pixel, which encodes `frame_no % 256`.
    fn first_frame_no(frame: &Frame) -> u8 {
        frame.pixels[0]
    }

    #[tokio::test]
    async fn streams_connected_then_frames() {
        let mut handle = connect().await;

        match handle.events.recv().await {
            Some(SessionEvent::Connected { size, capabilities }) => {
                assert_eq!(size, (WIDTH, HEIGHT));
                assert_eq!(capabilities.pointer, PointerMode::Absolute);
                assert_eq!(capabilities.actions.len(), 1);
                assert_eq!(capabilities.actions[0].id, RESET_ACTION);
            }
            other => panic!("expected Connected, got {other:?}"),
        }

        match handle.events.recv().await {
            Some(SessionEvent::Frame(frame)) => {
                assert_eq!(frame.width, WIDTH);
                assert_eq!(frame.height, HEIGHT);
                assert_eq!(frame.pixels.len(), (WIDTH * HEIGHT * 4) as usize);
            }
            other => panic!("expected Frame, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn reset_action_restarts_the_gradient() {
        let mut handle = connect().await;

        // Advance a few frames so the counter is clearly non-zero.
        let mut advanced = false;
        for _ in 0..8 {
            if let Some(SessionEvent::Frame(frame)) = handle.events.recv().await
                && first_frame_no(&frame) > 0
            {
                advanced = true;
                break;
            }
        }
        assert!(advanced, "gradient never advanced");

        handle
            .commands
            .send(SessionCommand::Action {
                id: Cow::Borrowed(RESET_ACTION),
            })
            .await
            .unwrap();

        // A post-reset frame restarts at frame_no 0.
        let mut reset_seen = false;
        for _ in 0..4 {
            if let Some(SessionEvent::Frame(frame)) = handle.events.recv().await
                && first_frame_no(&frame) == 0
            {
                reset_seen = true;
                break;
            }
        }
        assert!(reset_seen, "gradient did not reset");
    }

    #[tokio::test]
    async fn shuts_down_when_handle_dropped() {
        let handle = connect().await;
        let input = handle.input.clone();
        drop(handle);

        // Once every request channel is dropped, the task exits and closes its
        // receivers, so the surviving sender clone reports the channel closed.
        input.closed().await;
    }
}
