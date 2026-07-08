//! A single connection tab: runs a [`Connector`] on a background subscription
//! and renders the framebuffer it streams back.
//!
//! The subscription worker owns the [`SessionHandle`] and bridges it into
//! iced's world: session events become [`Message`]s, and it hands the UI an
//! [`mpsc::Sender`] so input can flow the other way. The `State` here holds only
//! what the view needs — the latest frame, the status, and that input sender.

use std::sync::Arc;

use iced::{
    Element, Length, Point, Subscription, Task,
    futures::{SinkExt, Stream, channel::mpsc as futures_mpsc},
    widget::{center, image, mouse_area, text},
};
use server_remote_core::{Connector, DummyConnector, InputEvent, SessionEvent, Target, TlsPolicy};
use tokio::sync::mpsc;

/// The protocol a connection speaks. Only [`Protocol::Dummy`] is implemented so
/// far; the others report "not implemented yet" when connected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Protocol {
    Dummy,
    Vnc,
    Spice,
    Ilo,
}

impl Protocol {
    pub const ALL: [Protocol; 4] = [
        Protocol::Dummy,
        Protocol::Vnc,
        Protocol::Spice,
        Protocol::Ilo,
    ];

    fn label(self) -> &'static str {
        match self {
            Protocol::Dummy => "dummy",
            Protocol::Vnc => "vnc",
            Protocol::Spice => "spice",
            Protocol::Ilo => "ilo",
        }
    }
}

impl std::fmt::Display for Protocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

enum Status {
    Connecting,
    Connected,
    Disconnected(Option<String>),
}

pub struct State {
    id: u64,
    name: Arc<str>,
    protocol: Protocol,
    target: Target,
    tls: TlsPolicy,
    status: Status,
    frame: Option<image::Handle>,
    input: Option<mpsc::Sender<InputEvent>>,
}

#[derive(Debug, Clone)]
pub enum Message {
    /// The worker is live and this is the channel to send input on.
    Ready(mpsc::Sender<InputEvent>),
    /// An event from the running session.
    Session(SessionEvent),
    /// The pointer moved over the framebuffer.
    Pointer(Point),
}

impl State {
    pub fn new(
        id: u64,
        name: Arc<str>,
        protocol: Protocol,
        target: Target,
        tls: TlsPolicy,
    ) -> Self {
        Self {
            id,
            name,
            protocol,
            target,
            tls,
            status: Status::Connecting,
            frame: None,
            input: None,
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn name(&self) -> &Arc<str> {
        &self.name
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Ready(sender) => {
                self.input = Some(sender);
            }
            Message::Session(event) => self.apply(event),
            Message::Pointer(point) => {
                if let Some(input) = &self.input {
                    // TODO: scale widget coordinates to framebuffer coordinates.
                    let _ = input.try_send(InputEvent::PointerMoved {
                        x: point.x,
                        y: point.y,
                    });
                }
            }
        }
        Task::none()
    }

    fn apply(&mut self, event: SessionEvent) {
        match event {
            SessionEvent::Connected { .. } => self.status = Status::Connected,
            SessionEvent::Frame(frame) => {
                self.frame = Some(image::Handle::from_rgba(
                    frame.width,
                    frame.height,
                    frame.pixels,
                ));
            }
            SessionEvent::Resized { .. } => {}
            SessionEvent::Disconnected { reason } => {
                self.status = Status::Disconnected(reason);
                self.input = None;
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let body: Element<'_, Message> = match (&self.status, &self.frame) {
            (Status::Connecting, _) => text("Connecting…").into(),
            (Status::Disconnected(reason), _) => {
                text(reason.clone().unwrap_or_else(|| "Disconnected".to_owned())).into()
            }
            (Status::Connected, None) => text("Waiting for first frame…").into(),
            (Status::Connected, Some(handle)) => mouse_area(
                image(handle.clone())
                    .width(Length::Fill)
                    .height(Length::Fill),
            )
            .on_move(Message::Pointer)
            .into(),
        };

        center(body).into()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        if let Status::Disconnected(_) = self.status {
            return Subscription::none();
        }

        // `run_with` identifies the subscription by hashing `data` (plus the
        // worker's function pointer), so the `id` keeps each connection's
        // subscription distinct. The worker must be a plain `fn`, so everything
        // it needs travels in `data` rather than a captured closure.
        Subscription::run_with(
            Params {
                id: self.id,
                protocol: self.protocol,
                target: self.target.clone(),
                tls: self.tls.clone(),
            },
            worker,
        )
    }
}

/// Everything the subscription worker needs, hashed to identify the session.
#[derive(Hash)]
struct Params {
    id: u64,
    protocol: Protocol,
    target: Target,
    tls: TlsPolicy,
}

// `use<>` declares the returned stream captures nothing from `&Params`, so it
// is `'static` and the fn coerces to the `fn(&D) -> S` that `run_with` wants.
fn worker(params: &Params) -> impl Stream<Item = Message> + use<> {
    let protocol = params.protocol;
    let target = params.target.clone();
    let tls = params.tls.clone();

    iced::stream::channel(
        32,
        move |mut output: futures_mpsc::Sender<Message>| async move {
            if protocol != Protocol::Dummy {
                let reason = format!("{protocol} is not implemented yet");
                let _ = output
                    .send(Message::Session(SessionEvent::Disconnected {
                        reason: Some(reason),
                    }))
                    .await;
                return;
            }

            let handle = match DummyConnector.connect(&target, &tls).await {
                Ok(handle) => handle,
                Err(error) => {
                    let _ = output
                        .send(Message::Session(SessionEvent::Disconnected {
                            reason: Some(error.to_string()),
                        }))
                        .await;
                    return;
                }
            };

            let (input_tx, mut input_rx) = mpsc::channel::<InputEvent>(64);
            if output.send(Message::Ready(input_tx)).await.is_err() {
                return;
            }

            let mut events = handle.events;
            let input = handle.input;
            let mut input_open = true;
            loop {
                tokio::select! {
                    event = events.recv() => match event {
                        Some(event) => {
                            let ended = matches!(event, SessionEvent::Disconnected { .. });
                            if output.send(Message::Session(event)).await.is_err() || ended {
                                break;
                            }
                        }
                        None => {
                            let _ = output
                                .send(Message::Session(SessionEvent::Disconnected { reason: None }))
                                .await;
                            break;
                        }
                    },
                    incoming = input_rx.recv(), if input_open => match incoming {
                        Some(event) => {
                            let _ = input.send(event).await;
                        }
                        // UI dropped its sender; stop polling this branch.
                        None => input_open = false,
                    },
                }
            }
        },
    )
}
