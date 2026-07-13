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
    Alignment, Element, Length, Point, Size, Subscription, Task,
    futures::{SinkExt, Stream, channel::mpsc as futures_mpsc},
    keyboard,
    widget::{Column, Row, Space, button, center, image, mouse_area, responsive, text},
};
use server_remote_core::{
    Capabilities, Connector, Control, DummyConnector, InputEvent, PointerButton, PointerMode,
    SessionCommand, SessionEvent, SessionHandle, Target, TlsPolicy,
};
use server_remote_spice::SpiceConnector;
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

#[derive(Debug, PartialEq)]
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
        let (disp_w, disp_h) = letterboxed_size(area, (fb_w, fb_h));

        mouse_area(center(image(handle.clone()).width(disp_w).height(disp_h)))
            .on_move(move |point| {
                let fb_point = map_to_framebuffer(area, (fb_w, fb_h), point);
                Message::Pointer(fb_point)
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

/// The scale factor that fits the framebuffer inside `area` while preserving
/// its aspect ratio.
fn fit_scale(area: Size, fb: (f32, f32)) -> f32 {
    (area.width / fb.0).min(area.height / fb.1)
}

/// The displayed size of the framebuffer image inside `area`, preserving
/// aspect ratio and letterboxing to fit.
fn letterboxed_size(area: Size, fb: (f32, f32)) -> (f32, f32) {
    let scale = fit_scale(area, fb);
    (fb.0 * scale, fb.1 * scale)
}

/// Maps a pointer position in `area`'s local coordinates to framebuffer
/// pixel coordinates, accounting for the letterboxing `letterboxed_size`
/// applies, and clamping to the framebuffer bounds.
fn map_to_framebuffer(area: Size, fb: (f32, f32), point: Point) -> Point {
    let scale = fit_scale(area, fb);
    let (disp_w, disp_h) = (fb.0 * scale, fb.1 * scale);
    let off_x = (area.width - disp_w) / 2.0;
    let off_y = (area.height - disp_h) / 2.0;

    let x = ((point.x - off_x) / scale).clamp(0.0, fb.0 - 1.0);
    let y = ((point.y - off_y) / scale).clamp(0.0, fb.1 - 1.0);
    Point::new(x, y)
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
                Protocol::Spice => {
                    let password = (!password.is_empty()).then_some(password);
                    SpiceConnector::new(password).connect(&target, &tls).await
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
    use server_remote_core::Frame;

    fn state() -> State {
        State::new(
            0,
            Arc::from("test"),
            Protocol::Dummy,
            Target::new("localhost", 0),
            TlsPolicy::default(),
            String::new(),
        )
    }

    fn wired_controls() -> (
        Controls,
        mpsc::Receiver<InputEvent>,
        mpsc::Receiver<SessionCommand>,
    ) {
        let (input, input_rx) = mpsc::channel(4);
        let (commands, command_rx) = mpsc::channel(4);
        (Controls { input, commands }, input_rx, command_rx)
    }

    #[test]
    fn connected_event_sets_size_capabilities_and_status() {
        let mut state = state();
        let capabilities = Capabilities {
            pointer: PointerMode::Relative,
            actions: Vec::new(),
        };
        state.apply(SessionEvent::Connected {
            size: (800, 600),
            capabilities,
        });

        assert_eq!(state.size, Some((800, 600)));
        assert_eq!(state.capabilities.pointer, PointerMode::Relative);
        assert_eq!(state.status, Status::Connected);
    }

    #[test]
    fn frame_event_updates_size_and_stores_the_frame() {
        let mut state = state();
        state.apply(SessionEvent::Frame(Frame {
            width: 4,
            height: 2,
            pixels: vec![0; 4 * 2 * 4],
        }));

        assert_eq!(state.size, Some((4, 2)));
        assert!(state.frame.is_some());
    }

    #[test]
    fn resized_event_only_updates_size() {
        let mut state = state();
        state.apply(SessionEvent::Resized {
            width: 10,
            height: 20,
        });
        assert_eq!(state.size, Some((10, 20)));
    }

    #[test]
    fn disconnected_event_sets_reason_and_drops_controls() {
        let mut state = state();
        let (controls, _input_rx, _command_rx) = wired_controls();
        state.controls = Some(controls);

        state.apply(SessionEvent::Disconnected {
            reason: Some("connection reset".to_owned()),
        });

        assert_eq!(
            state.status,
            Status::Disconnected(Some("connection reset".to_owned()))
        );
        assert!(state.controls.is_none());
    }

    #[test]
    fn ready_message_stores_the_controls() {
        let mut state = state();
        let (controls, _input_rx, _command_rx) = wired_controls();

        let _ = state.update(Message::Ready(controls));

        assert!(state.controls.is_some());
    }

    #[test]
    fn button_message_forwards_to_the_input_channel() {
        let mut state = state();
        let (controls, mut input_rx, _command_rx) = wired_controls();
        state.controls = Some(controls);

        let _ = state.update(Message::Button {
            button: PointerButton::Right,
            pressed: true,
        });

        match input_rx.try_recv() {
            Ok(InputEvent::PointerButton { button, pressed }) => {
                assert_eq!(button, PointerButton::Right);
                assert!(pressed);
            }
            other => panic!("expected a pointer button event, got {other:?}"),
        }
    }

    #[test]
    fn key_message_forwards_the_mapped_keysym() {
        let mut state = state();
        let (controls, mut input_rx, _command_rx) = wired_controls();
        state.controls = Some(controls);

        let _ = state.update(Message::Key {
            key: keyboard::Key::Named(Named::Escape),
            pressed: true,
        });

        match input_rx.try_recv() {
            Ok(InputEvent::Key { keysym, pressed }) => {
                assert_eq!(keysym, 0xFF1B);
                assert!(pressed);
            }
            other => panic!("expected a key event, got {other:?}"),
        }
    }

    #[test]
    fn key_message_with_no_mapping_sends_nothing() {
        let mut state = state();
        let (controls, mut input_rx, _command_rx) = wired_controls();
        state.controls = Some(controls);

        let _ = state.update(Message::Key {
            key: keyboard::Key::Unidentified,
            pressed: true,
        });

        assert!(input_rx.try_recv().is_err());
    }

    #[test]
    fn action_message_forwards_the_action_id() {
        let mut state = state();
        let (controls, _input_rx, mut command_rx) = wired_controls();
        state.controls = Some(controls);

        let _ = state.update(Message::Action(Cow::Borrowed("ctrl-alt-del")));

        match command_rx.try_recv() {
            Ok(SessionCommand::Action { id }) => assert_eq!(id, "ctrl-alt-del"),
            other => panic!("expected an action command, got {other:?}"),
        }
    }

    #[test]
    fn pointer_message_forwards_absolute_position() {
        let mut state = state();
        let (controls, mut input_rx, _command_rx) = wired_controls();
        state.controls = Some(controls);
        state.capabilities.pointer = PointerMode::Absolute;

        let _ = state.update(Message::Pointer(Point::new(3.0, 4.0)));

        match input_rx.try_recv() {
            Ok(InputEvent::PointerMoved { x, y }) => assert_eq!((x, y), (3.0, 4.0)),
            other => panic!("expected a pointer moved event, got {other:?}"),
        }
    }

    #[test]
    fn pointer_message_is_dropped_in_relative_mode() {
        let mut state = state();
        let (controls, mut input_rx, _command_rx) = wired_controls();
        state.controls = Some(controls);
        state.capabilities.pointer = PointerMode::Relative;

        let _ = state.update(Message::Pointer(Point::new(3.0, 4.0)));

        assert!(input_rx.try_recv().is_err());
    }

    #[test]
    fn default_ports_match_each_protocol() {
        assert_eq!(Protocol::Dummy.default_port(), 0);
        assert_eq!(Protocol::Vnc.default_port(), 5900);
        assert_eq!(Protocol::Spice.default_port(), 5900);
        assert_eq!(Protocol::Ilo.default_port(), 443);
    }

    #[test]
    fn letterboxed_size_preserves_aspect_ratio_when_area_is_wider() {
        // 800x600 framebuffer in a 1600x300 area: height is the limiting
        // dimension (scale 0.5), so width should letterbox, not fill.
        let (w, h) = letterboxed_size(Size::new(1600.0, 300.0), (800.0, 600.0));
        assert_eq!((w, h), (400.0, 300.0));
    }

    #[test]
    fn letterboxed_size_preserves_aspect_ratio_when_area_is_taller() {
        let (w, h) = letterboxed_size(Size::new(300.0, 1600.0), (800.0, 600.0));
        assert_eq!((w, h), (300.0, 225.0));
    }

    #[test]
    fn map_to_framebuffer_scales_and_letterboxes() {
        // 800x600 fb displayed at 400x300 inside a 400x400 area: the image
        // is vertically centred with a 50px letterbox band top and bottom.
        let area = Size::new(400.0, 400.0);
        let fb = (800.0, 600.0);

        let top_left = map_to_framebuffer(area, fb, Point::new(0.0, 50.0));
        assert_eq!(top_left, Point::new(0.0, 0.0));

        let centre = map_to_framebuffer(area, fb, Point::new(200.0, 200.0));
        assert_eq!(centre, Point::new(400.0, 300.0));
    }

    #[test]
    fn map_to_framebuffer_clamps_points_outside_the_letterbox_band() {
        let area = Size::new(400.0, 400.0);
        let fb = (800.0, 600.0);

        // Above the letterbox band (y < 50) clamps to the top row.
        let above = map_to_framebuffer(area, fb, Point::new(0.0, 0.0));
        assert_eq!(above.y, 0.0);

        // Past the right/bottom edge clamps to the last pixel, not the size.
        let corner = map_to_framebuffer(area, fb, Point::new(1000.0, 1000.0));
        assert_eq!(corner, Point::new(fb.0 - 1.0, fb.1 - 1.0));
    }

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
