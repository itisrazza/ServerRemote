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

use iced::keyboard::key::Named;
use iced::{
    Alignment, Element, Length, Point, Subscription, Task,
    futures::{SinkExt, Stream, channel::mpsc as futures_mpsc},
    keyboard,
    widget::{Column, Row, Space, button, center, image, mouse_area, responsive, text},
};
use server_remote_core::{
    Capabilities, Connector, Control, DummyConnector, InputEvent, PointerButton, PointerMode,
    SessionCommand, SessionEvent, SessionHandle, Target, TlsPolicy,
};
use server_remote_vnc::VncConnector;
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

    /// Default port when the hostname doesn't specify one.
    pub fn default_port(self) -> u16 {
        match self {
            Protocol::Dummy => 0,
            Protocol::Vnc | Protocol::Spice => 5900,
            Protocol::Ilo => 443,
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
    password: String,
    status: Status,
    capabilities: Capabilities,
    frame: Option<image::Handle>,
    /// Remote framebuffer size in pixels, for mapping pointer coordinates.
    size: Option<(u32, u32)>,
    controls: Option<Controls>,
}

#[derive(Debug, Clone)]
pub enum Message {
    /// The worker is live; these are the channels to the session.
    Ready(Controls),
    /// An event from the running session.
    Session(SessionEvent),
    /// The pointer moved over the framebuffer (in framebuffer coordinates).
    Pointer(Point),
    /// A mouse button was pressed or released over the framebuffer.
    Button {
        button: PointerButton,
        pressed: bool,
    },
    /// A key was pressed or released while this connection is the active tab.
    Key { key: keyboard::Key, pressed: bool },
    /// Invoke a declarative action advertised in [`Capabilities`].
    Action(Cow<'static, str>),
    /// Disconnect and close the tab. Handled by the application, which owns the
    /// tab list; dropping the tab tears the session down. An *unexpected* close
    /// arrives as a `Session(Disconnected { .. })` event instead, so its reason
    /// stays on screen.
    Close,
}

impl State {
    pub fn new(
        id: u64,
        name: Arc<str>,
        protocol: Protocol,
        target: Target,
        tls: TlsPolicy,
        password: String,
    ) -> Self {
        Self {
            id,
            name,
            protocol,
            target,
            tls,
            password,
            status: Status::Connecting,
            capabilities: Capabilities::default(),
            frame: None,
            size: None,
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
            Message::Button { button, pressed } => {
                if let Some(controls) = &self.controls {
                    let _ = controls
                        .input
                        .try_send(InputEvent::PointerButton { button, pressed });
                }
            }
            Message::Key { key, pressed } => {
                if let (Some(controls), Some(keysym)) = (&self.controls, keysym(&key)) {
                    let _ = controls.input.try_send(InputEvent::Key { keysym, pressed });
                }
            }
            Message::Action(id) => {
                if let Some(controls) = &self.controls {
                    let _ = controls.commands.try_send(SessionCommand::Action { id });
                }
            }
            // The application owns the tab list, so it intercepts `Close`; this
            // arm only satisfies exhaustiveness.
            Message::Close => {}
        }
        Task::none()
    }

    fn apply(&mut self, event: SessionEvent) {
        match event {
            SessionEvent::Connected { size, capabilities } => {
                self.size = Some(size);
                self.capabilities = capabilities;
                self.status = Status::Connected;
            }
            SessionEvent::Frame(frame) => {
                self.size = Some((frame.width, frame.height));
                self.frame = Some(image::Handle::from_rgba(
                    frame.width,
                    frame.height,
                    frame.pixels,
                ));
            }
            SessionEvent::Resized { width, height } => self.size = Some((width, height)),
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
            // `point` is already in framebuffer coordinates (mapped in the view).
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
            (Status::Connected, Some(handle)) => {
                framebuffer_view(handle.clone(), self.size.unwrap_or((1, 1)))
            }
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
                    .on_press(Message::Close),
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
                password: self.password.clone(),
            },
            worker,
        )
    }
}

/// The framebuffer as an aspect-preserving, scaled image with pointer and
/// button handling. `responsive` gives us the on-screen area, so pointer
/// coordinates can be mapped from the displayed rectangle back to framebuffer
/// pixels regardless of window size.
fn framebuffer_view(handle: image::Handle, size: (u32, u32)) -> Element<'static, Message> {
    let fb_w = size.0.max(1) as f32;
    let fb_h = size.1.max(1) as f32;

    responsive(move |area| {
        let scale = (area.width / fb_w).min(area.height / fb_h);
        let disp_w = fb_w * scale;
        let disp_h = fb_h * scale;
        let off_x = (area.width - disp_w) / 2.0;
        let off_y = (area.height - disp_h) / 2.0;

        mouse_area(center(image(handle.clone()).width(disp_w).height(disp_h)))
            .on_move(move |point| {
                let x = ((point.x - off_x) / scale).clamp(0.0, fb_w - 1.0);
                let y = ((point.y - off_y) / scale).clamp(0.0, fb_h - 1.0);
                Message::Pointer(Point::new(x, y))
            })
            .on_press(Message::Button {
                button: PointerButton::Left,
                pressed: true,
            })
            .on_release(Message::Button {
                button: PointerButton::Left,
                pressed: false,
            })
            .on_right_press(Message::Button {
                button: PointerButton::Right,
                pressed: true,
            })
            .on_right_release(Message::Button {
                button: PointerButton::Right,
                pressed: false,
            })
            .on_middle_press(Message::Button {
                button: PointerButton::Middle,
                pressed: true,
            })
            .on_middle_release(Message::Button {
                button: PointerButton::Middle,
                pressed: false,
            })
            .into()
    })
    .into()
}

/// Translate an iced key into an X11 keysym, which is what RFB/Spice expect.
/// Returns `None` for keys we don't map (they're simply ignored).
fn keysym(key: &keyboard::Key) -> Option<u32> {
    match key {
        keyboard::Key::Character(text) => text.chars().next().map(character_keysym),
        keyboard::Key::Named(named) => named_keysym(*named),
        keyboard::Key::Unidentified => None,
    }
}

fn character_keysym(character: char) -> u32 {
    let code = character as u32;
    // Latin-1 code points are their own keysyms; the rest use the Unicode form.
    if code <= 0xFF {
        code
    } else {
        0x0100_0000 + code
    }
}

fn named_keysym(named: Named) -> Option<u32> {
    let keysym = match named {
        Named::Enter => 0xFF0D,
        Named::Tab => 0xFF09,
        Named::Space => 0x0020,
        Named::Backspace => 0xFF08,
        Named::Escape => 0xFF1B,
        Named::Delete => 0xFFFF,
        Named::Insert => 0xFF63,
        Named::Home => 0xFF50,
        Named::End => 0xFF57,
        Named::PageUp => 0xFF55,
        Named::PageDown => 0xFF56,
        Named::ArrowLeft => 0xFF51,
        Named::ArrowUp => 0xFF52,
        Named::ArrowRight => 0xFF53,
        Named::ArrowDown => 0xFF54,
        Named::Shift => 0xFFE1,
        Named::Control => 0xFFE3,
        Named::Alt => 0xFFE9,
        Named::AltGraph => 0xFFEA,
        Named::Super | Named::Meta => 0xFFEB,
        Named::CapsLock => 0xFFE5,
        Named::F1 => 0xFFBE,
        Named::F2 => 0xFFBF,
        Named::F3 => 0xFFC0,
        Named::F4 => 0xFFC1,
        Named::F5 => 0xFFC2,
        Named::F6 => 0xFFC3,
        Named::F7 => 0xFFC4,
        Named::F8 => 0xFFC5,
        Named::F9 => 0xFFC6,
        Named::F10 => 0xFFC7,
        Named::F11 => 0xFFC8,
        Named::F12 => 0xFFC9,
        _ => return None,
    };
    Some(keysym)
}

/// Everything the subscription worker needs, hashed to identify the session.
#[derive(Hash)]
struct Params {
    id: u64,
    protocol: Protocol,
    target: Target,
    tls: TlsPolicy,
    password: String,
}

// `use<>` declares the returned stream captures nothing from `&Params`, so it
// is `'static` and the fn coerces to the `fn(&D) -> S` that `run_with` wants.
fn worker(params: &Params) -> impl Stream<Item = Message> + use<> {
    let protocol = params.protocol;
    let target = params.target.clone();
    let tls = params.tls.clone();
    let password = params.password.clone();

    iced::stream::channel(
        32,
        move |mut output: futures_mpsc::Sender<Message>| async move {
            let result = match protocol {
                Protocol::Dummy => DummyConnector.connect(&target, &tls).await,
                Protocol::Vnc => {
                    let password = (!password.is_empty()).then_some(password);
                    VncConnector::new(password).connect(&target, &tls).await
                }
                other => {
                    let reason = format!("{other} is not implemented yet");
                    let _ = output
                        .send(Message::Session(SessionEvent::Disconnected {
                            reason: Some(reason),
                        }))
                        .await;
                    return;
                }
            };

            let handle = match result {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn characters_map_to_latin1_keysyms() {
        assert_eq!(character_keysym('a'), 0x61);
        assert_eq!(character_keysym('A'), 0x41);
        assert_eq!(character_keysym('!'), 0x21);
        // Beyond Latin-1 uses the Unicode keysym form.
        assert_eq!(character_keysym('€'), 0x0100_0000 + '€' as u32);
    }

    #[test]
    fn named_keys_map_to_x11_keysyms() {
        assert_eq!(named_keysym(Named::Enter), Some(0xFF0D));
        assert_eq!(named_keysym(Named::ArrowUp), Some(0xFF52));
        assert_eq!(named_keysym(Named::F5), Some(0xFFC2));
        assert_eq!(named_keysym(Named::Control), Some(0xFFE3));
        assert_eq!(named_keysym(Named::PrintScreen), None);
    }

    #[test]
    fn keysym_dispatches_on_key_kind() {
        assert_eq!(keysym(&keyboard::Key::Character("z".into())), Some(0x7A));
        assert_eq!(keysym(&keyboard::Key::Named(Named::Escape)), Some(0xFF1B));
        assert_eq!(keysym(&keyboard::Key::Unidentified), None);
    }
}
