//! A self-contained connector that streams a moving gradient.
//!
//! It talks to nothing, so it's useful for exercising the UI and the session
//! plumbing without a real server.

use std::time::Duration;

use tokio::sync::mpsc;

use crate::{ConnectError, Connector, Frame, SessionEvent, SessionHandle, Target, TlsPolicy};

const WIDTH: u32 = 320;
const HEIGHT: u32 = 240;

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

        tokio::spawn(async move {
            let connected = SessionEvent::Connected {
                size: (WIDTH, HEIGHT),
            };
            if event_tx.send(connected).await.is_err() {
                return;
            }

            let mut tick = tokio::time::interval(Duration::from_millis(500));
            let mut frame_no: u32 = 0;
            loop {
                tokio::select! {
                    _ = tick.tick() => {
                        let frame = gradient_frame(frame_no);
                        frame_no = frame_no.wrapping_add(1);
                        if event_tx.send(SessionEvent::Frame(frame)).await.is_err() {
                            break;
                        }
                    }
                    // Drain input so we notice when the UI drops its handle.
                    incoming = input_rx.recv() => {
                        if incoming.is_none() {
                            break;
                        }
                    }
                }
            }
        });

        Ok(SessionHandle { events, input })
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

    #[tokio::test]
    async fn streams_connected_then_frames() {
        let mut handle = DummyConnector
            .connect(&Target::new("localhost", 0), &TlsPolicy::default())
            .await
            .expect("dummy connect never fails");

        match handle.events.recv().await {
            Some(SessionEvent::Connected { size }) => assert_eq!(size, (WIDTH, HEIGHT)),
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
    async fn shuts_down_when_handle_dropped() {
        let handle = DummyConnector
            .connect(&Target::new("localhost", 0), &TlsPolicy::default())
            .await
            .unwrap();
        let input = handle.input.clone();
        drop(handle);

        // The background task drains input; once it stops, the sender closes.
        input.closed().await;
    }
}
