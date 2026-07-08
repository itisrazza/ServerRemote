//! A single connection tab: runs a [`Connector`] on a background subscription,
//! renders the framebuffer it streams back, and drives a capability-driven
//! toolbar.
//!
//! The subscription worker owns the connection's [`SessionHandle`] events end
//! and hands the UI the input and command senders, so the UI talks to the
//! session directly. `State` holds what the view needs — the latest frame, the
//! status, the reported [`Capabilities`], and those senders.

use std::borrow::Cow;
use std::sync::Arc;

use iced::{
    Alignment, Element, Length, Point, Subscription, Task,
    futures::{SinkExt, Stream, channel::mpsc as futures_mpsc},
    widget::{Column, Row, Space, button, center, image, mouse_area, text},
};
use server_remote_core::{
    Capabilities, Connector, Control, DummyConnector, InputEvent, PointerMode, SessionCommand,
    SessionEvent, SessionHandle, Target, TlsPolicy,
};
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

/// The senders the UI uses to talk to a running session.
#[derive(Debug, Clone)]
pub struct Controls {
    input: mpsc::Sender<InputEvent>,
    commands: mpsc::Sender<SessionCommand>,
}

pub struct State {
    id: u64,
    name: Arc<str>,
    protocol: Protocol,
    target: Target,
    tls: TlsPolicy,
    status: Status,
    capabilities: Capabilities,
    frame: Option<image::Handle>,
    controls: Option<Controls>,
}

#[derive(Debug, Clone)]
pub enum Message {
    /// The worker is live; these are the channels to the session.
    Ready(Controls),
    /// An event from the running session.
    Session(SessionEvent),
    /// The pointer moved over the framebuffer.
    Pointer(Point),
    /// Invoke a declarative action advertised in [`Capabilities`].
    Action(Cow<'static, str>),
    /// Tear the session down.
    Disconnect,
    /// Close the tab. Handled by the application, which owns the tab list.
    Close,
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
            capabilities: Capabilities::default(),
            frame: None,
            controls: None,
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
            Message::Ready(controls) => self.controls = Some(controls),
            Message::Session(event) => self.apply(event),
            Message::Pointer(point) => self.send_pointer(point),
            Message::Action(id) => {
                if let Some(controls) = &self.controls {
                    let _ = controls.commands.try_send(SessionCommand::Action { id });
                }
            }
            Message::Disconnect => {
                // Dropping the controls and stopping the subscription (via the
                // status change) tears the session down.
                self.status = Status::Disconnected(None);
                self.controls = None;
            }
            // The application owns the tab list, so it intercepts `Close`; this
            // arm only satisfies exhaustiveness.
            Message::Close => {}
        }
        Task::none()
    }

    fn apply(&mut self, event: SessionEvent) {
        match event {
            SessionEvent::Connected { capabilities, .. } => {
                self.capabilities = capabilities;
                self.status = Status::Connected;
            }
            SessionEvent::Frame(frame) => {
                self.frame = Some(image::Handle::from_rgba(
                    frame.width,
                    frame.height,
                    frame.pixels,
                ));
            }
            SessionEvent::Resized { .. } => {}
            SessionEvent::Capabilities(capabilities) => self.capabilities = capabilities,
            SessionEvent::Disconnected { reason } => {
                self.status = Status::Disconnected(reason);
                self.controls = None;
            }
        }
    }

    fn send_pointer(&self, point: Point) {
        let Some(controls) = &self.controls else {
            return;
        };
        match self.capabilities.pointer {
            // TODO: scale widget coordinates to framebuffer coordinates.
            PointerMode::Absolute => {
                let _ = controls.input.try_send(InputEvent::PointerMoved {
                    x: point.x,
                    y: point.y,
                });
            }
            // TODO: grab the pointer and send PointerMotion deltas.
            PointerMode::Relative => {}
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let body: Element<'_, Message> = match (&self.status, &self.frame) {
            (Status::Connecting, _) => text("Connecting…").into(),
            (Status::Disconnected(reason), _) => {
                let reason = reason.clone().unwrap_or_else(|| "Disconnected".to_owned());
                Column::new()
                    .spacing(8)
                    .align_x(Alignment::Center)
                    .push(text(reason))
                    .push(
                        button(text("Close tab"))
                            .style(button::subtle)
                            .on_press(Message::Close),
                    )
                    .into()
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

        let mut view = Column::new().spacing(4);
        if !matches!(self.status, Status::Disconnected(_)) {
            view = view.push(self.toolbar());
        }
        view.push(center(body)).into()
    }

    fn toolbar(&self) -> Element<'_, Message> {
        let mut actions = Row::new().spacing(4);
        for action in &self.capabilities.actions {
            match &action.control {
                Control::Button => {
                    actions = actions.push(
                        button(text(action.label.to_string()))
                            .style(button::subtle)
                            .on_press(Message::Action(action.id.clone())),
                    );
                }
            }
        }

        Row::new()
            .push(actions)
            .push(Space::new().width(Length::Fill))
            .push(
                button(text("Disconnect"))
                    .style(button::danger)
                    .on_press(Message::Disconnect),
            )
            .spacing(4)
            .into()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        if let Status::Disconnected(_) = self.status {
            return Subscription::none();
        }

        // `run_with` identifies the subscription by hashing `Params` (plus the
        // worker's function pointer), so the `id` keeps each connection's
        // subscription distinct. The worker must be a plain `fn`, so everything
        // it needs travels in `Params` rather than a captured closure.
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

            // Hand the UI the session's own senders, then just pump events out.
            let SessionHandle {
                mut events,
                input,
                commands,
            } = handle;
            if output
                .send(Message::Ready(Controls { input, commands }))
                .await
                .is_err()
            {
                return;
            }

            while let Some(event) = events.recv().await {
                let ended = matches!(event, SessionEvent::Disconnected { .. });
                if output.send(Message::Session(event)).await.is_err() || ended {
                    break;
                }
            }
        },
    )
}
