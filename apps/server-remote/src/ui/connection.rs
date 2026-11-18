//! A single connection tab: runs a [`Connector`] on a background subscription,
//! renders the framebuffer it streams back, and drives a toolbar of dropdown
//! menus and a status bar.
//!
//! The subscription worker owns the connection's [`SessionHandle`] events end
//! and hands the UI the input and command senders, so the UI talks to the
//! session directly. `State` holds what the view needs — the latest frame, the
//! status, the reported [`Capabilities`], and those senders.

use crate::i18n::fl;
use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use iced::keyboard::key::Named;
use iced::{
    Alignment, Element, Length, Point, Subscription, Task, Theme,
    futures::{SinkExt, Stream, channel::mpsc as futures_mpsc},
    keyboard,
    widget::{Column, Id, Row, Space, button, center, operation, stack, text},
};
use server_remote_core::{
    Action, ActionValue, Capabilities, CertFingerprint, ConnectError, Connector, Control,
    DummyConnector, InputEvent, PointerButton, PointerMode, SessionCommand, SessionEvent,
    SessionHandle, Status as SessionStatus, Target, TlsPolicy, Traffic,
};
use server_remote_icons as icons;
use server_remote_ilo::IloConnector;
use server_remote_spice::SpiceConnector;
use server_remote_vnc::VncConnector;
use tokio::sync::mpsc;

use super::contributed;
use super::dialog;
use super::icon;
use super::kvm_display::{DisplayFrame, kvm_display};
use super::menu::{self, Item};
use super::settings::{self as settings_ui, Change, Section};
use super::style;
use super::virtual_keyboard;
use super::virtual_trackpad;
use crate::persistence::settings::{self as settings, Scaling, Settings};

/// The protocol a connection speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
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

    /// Human-facing name, as shown in the status bar.
    pub fn name(self) -> &'static str {
        match self {
            Protocol::Dummy => "Dummy",
            Protocol::Vnc => "VNC",
            Protocol::Spice => "SPICE",
            Protocol::Ilo => "iLO",
        }
    }

    pub fn icon(self) -> icons::Icon {
        match self {
            Protocol::Dummy => icons::DIAMOND_OUTLINE,
            Protocol::Vnc => icons::SCREEN,
            Protocol::Spice => icons::SCREEN,
            Protocol::Ilo => icons::PROTOCOL_ILO,
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

/// The layout of the virtual on-screen keyboard.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
pub enum KeyboardSize {
    #[serde(rename = "compact")]
    Small,
    #[default]
    #[serde(rename = "tenkeyless")]
    Medium,
    #[serde(rename = "full-size")]
    Large,
}

impl KeyboardSize {
    pub const ALL: [KeyboardSize; 3] = [
        KeyboardSize::Small,
        KeyboardSize::Medium,
        KeyboardSize::Large,
    ];

    pub fn label(self) -> String {
        match self {
            KeyboardSize::Small => fl!("keyboard-style-compact"),
            KeyboardSize::Medium => fl!("keyboard-style-tenkeyless"),
            KeyboardSize::Large => fl!("keyboard-style-full"),
        }
    }
}

impl std::fmt::Display for KeyboardSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label())
    }
}

/// The toolbar's dropdown menus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolbarMenu {
    Display,
    Input,
    Recording,
    /// Not a menu as such: the popout with technical connection details.
    Info,
    /// A menu the connector contributed, by its id.
    Contributed(Cow<'static, str>),
}

/// Submenus that can currently be opened. Unimplemented ones render disabled
/// and so never need a variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Submenu {
    KeyboardStyle,
    KeyboardLayout,
    DisplayScale,
    RecordingFolder,
    RecordingQuality,
    RecordingFormat,
    /// A submenu of a contributed menu, by its index path through the tree.
    Contributed(Vec<usize>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenMenu {
    pub menu: ToolbarMenu,
    pub submenu: Option<Submenu>,
}

/// A dialog shown over the connection.
#[derive(Debug, Clone)]
enum Modal {
    /// Asks before invoking an action that has a
    /// [`Confirmation`](server_remote_core::Confirmation).
    Confirm(Action),
    /// Asks whether to trust a certificate that failed verification.
    TrustCertificate(CertFingerprint),
    Error {
        title: String,
        message: String,
    },
}

/// Turns the connector's cumulative byte counters into rates.
#[derive(Debug, Default)]
struct TrafficMeter {
    last: Option<(Instant, Traffic)>,
    rate: Option<Rate>,
}

/// Bytes per second in each direction.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Rate {
    received: f64,
    sent: f64,
}

impl TrafficMeter {
    /// Samples closer together than this would make the rate jumpy, so
    /// they're skipped until enough time has passed.
    const MIN_INTERVAL: Duration = Duration::from_millis(500);

    fn sample(&mut self, at: Instant, traffic: Traffic) {
        let Some((then, previous)) = self.last else {
            self.last = Some((at, traffic));
            return;
        };
        let elapsed = at.saturating_duration_since(then);
        if elapsed < Self::MIN_INTERVAL {
            return;
        }
        let per_second =
            |now: u64, before: u64| now.saturating_sub(before) as f64 / elapsed.as_secs_f64();
        self.rate = Some(Rate {
            received: per_second(traffic.received, previous.received),
            sent: per_second(traffic.sent, previous.sent),
        });
        self.last = Some((at, traffic));
    }
}

/// A rate in bits per second, the way network rates are usually written.
fn format_rate(bytes_per_second: f64) -> String {
    let bits = bytes_per_second * 8.0;
    if bits < 1_000_000.0 {
        format!("{:.0} kb/s", bits / 1_000.0)
    } else {
        format!("{:.1} Mb/s", bits / 1_000_000.0)
    }
}

/// Where the connection is in its lifecycle.
#[derive(Debug, PartialEq)]
enum Phase {
    Connecting,
    Connected,
    Disconnected(Option<String>),
}

/// What the user signs in with. Only iLO uses the user name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Credentials {
    pub username: String,
    pub password: String,
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
    credentials: Credentials,
    phase: Phase,
    capabilities: Capabilities,
    frame: Option<DisplayFrame>,
    next_generation: u64,
    /// Remote framebuffer size in pixels, for mapping pointer coordinates.
    size: Option<(u32, u32)>,
    controls: Option<Controls>,
    /// Identifies the [`kvm_display`] widget so it can be focused
    /// programmatically (see [`SessionEvent::Connected`] handling below).
    display_id: Id,
    /// Whether the virtual on-screen keyboard panel is shown, and its layout.
    show_virtual_keyboard: bool,
    keyboard_size: KeyboardSize,
    virtual_keyboard: virtual_keyboard::State,
    /// Whether the virtual trackpad panel is shown.
    show_virtual_trackpad: bool,
    virtual_trackpad: virtual_trackpad::State,
    menu: Option<OpenMenu>,
    /// The connector's latest status snapshot.
    session_status: SessionStatus,
    traffic: TrafficMeter,
    icons: icon::Cache,
    modal: Option<Modal>,
}

#[derive(Debug, Clone)]
pub enum Message {
    /// The worker is live; these are the channels to the session.
    Ready(Controls),
    /// An event from the running session.
    Session(SessionEvent),
    /// The target's TLS certificate failed verification; ask the user. The
    /// application refuses it instead if settings say so.
    UntrustedCertificate(CertFingerprint),
    /// Trust the certificate the dialog showed, and connect again. The
    /// application also saves it when `remember` is set.
    TrustCertificate {
        fingerprint: CertFingerprint,
        remember: bool,
    },
    /// Refuse the certificate the dialog showed. The application adds it to
    /// the blocked list.
    BlockCertificate(CertFingerprint),
    /// The pointer moved over the framebuffer (in framebuffer coordinates).
    Pointer(Point),
    /// A mouse button was pressed or released over the framebuffer.
    Button {
        button: PointerButton,
        pressed: bool,
    },
    /// A key was pressed or released while this connection is the active tab.
    Key { key: keyboard::Key, pressed: bool },
    /// Invoke a declarative action from [`Capabilities`]: confirm it and
    /// collect its value first if it needs either.
    Invoke(Action),
    /// Go ahead with the action awaiting confirmation.
    Confirm,
    /// Close the open dialog without doing anything.
    CloseModal,
    /// The file picker for an action closed; `path` is `None` if cancelled.
    FilePicked {
        id: Cow<'static, str>,
        path: Option<PathBuf>,
    },
    /// Send an action by id straight away, e.g. from a chip's close button.
    SendAction(Cow<'static, str>),
    /// Open a connector-supplied link in the browser.
    OpenUrl(Cow<'static, str>),
    /// Disconnect and close the tab. Handled by the application, which owns the
    /// tab list; dropping the tab tears the session down. An *unexpected* close
    /// arrives as a `Session(Disconnected { .. })` event instead, so its reason
    /// stays on screen.
    Close,
    /// Toggle the virtual on-screen keyboard panel.
    ToggleVirtualKeyboard,
    /// Switch the virtual keyboard's layout size.
    SetKeyboardSize(KeyboardSize),
    /// Toggle the virtual trackpad panel (independent of the keyboard).
    ToggleVirtualTrackpad,
    /// A message produced by the virtual keyboard widget.
    VirtualKeyboard(virtual_keyboard::Message),
    /// A message produced by the virtual trackpad widget.
    VirtualTrackpad(virtual_trackpad::Message),
    /// Open the given toolbar menu, or close it if it's already open.
    ToggleMenu(ToolbarMenu),
    /// Open a submenu of the currently open toolbar menu.
    OpenSubmenu(Submenu),
    /// Close whichever menu is open.
    CloseMenu,
    /// Show this section of the Settings tab. Handled by the application.
    OpenSettings(Section),
    /// Change an app-wide setting. Handled by the application.
    ChangeSetting(Change),
    /// Pick a new folder for recordings. Handled by the application.
    ChooseRecordingFolder,
}

impl Message {
    /// Whether handling this message should close the open menu — true for
    /// the actions menu entries trigger.
    fn closes_menu(&self) -> bool {
        matches!(
            self,
            Message::Invoke(_)
                | Message::SendAction(_)
                | Message::OpenUrl(_)
                | Message::ToggleVirtualKeyboard
                | Message::SetKeyboardSize(_)
                | Message::ToggleVirtualTrackpad
                | Message::CloseMenu
        )
    }
}

impl State {
    pub fn new(
        id: u64,
        name: Arc<str>,
        protocol: Protocol,
        target: Target,
        tls: TlsPolicy,
        credentials: Credentials,
        input: &settings::Input,
    ) -> Self {
        Self {
            display_id: Id::unique(),
            id,
            name,
            protocol,
            target,
            tls,
            credentials,
            phase: Phase::Connecting,
            capabilities: Capabilities::default(),
            frame: None,
            next_generation: 0,
            size: None,
            controls: None,
            show_virtual_keyboard: input.open_keyboard_on_connect,
            keyboard_size: input.keyboard_style,
            virtual_keyboard: virtual_keyboard::State::default(),
            show_virtual_trackpad: false,
            virtual_trackpad: virtual_trackpad::State::default(),
            menu: None,
            session_status: SessionStatus::default(),
            traffic: TrafficMeter::default(),
            icons: icon::Cache::default(),
            modal: None,
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn name(&self) -> &Arc<str> {
        &self.name
    }

    pub fn protocol(&self) -> Protocol {
        self.protocol
    }

    pub fn target(&self) -> &Target {
        &self.target
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        if message.closes_menu() {
            self.menu = None;
        }
        match message {
            Message::Ready(controls) => self.controls = Some(controls),
            Message::Session(event) => {
                // Grabs focus the moment the `kvm_display` widget first
                // appears (i.e. the first frame arrives — before that, `view`
                // still shows "Waiting for first frame…" and there's nothing
                // in the tree to focus), so typing works immediately without
                // an initial click. Matches the old window-level keyboard
                // subscription's behaviour, which forwarded keys whenever
                // this tab was merely selected.
                let should_focus = matches!(event, SessionEvent::Frame(_)) && self.frame.is_none();
                self.apply(event);
                if should_focus {
                    return operation::focus(self.display_id.clone());
                }
            }
            Message::UntrustedCertificate(fingerprint) => {
                self.phase = Phase::Disconnected(Some(fl!("certificate-not-trusted")));
                self.modal = Some(Modal::TrustCertificate(fingerprint));
            }
            Message::TrustCertificate { fingerprint, .. } => {
                self.modal = None;
                // A new policy changes the subscription's identity, so this
                // starts a fresh connection attempt.
                self.tls = TlsPolicy::AllowInsecure {
                    accepted: Some(fingerprint),
                };
                self.phase = Phase::Connecting;
            }
            Message::BlockCertificate(_) => {
                self.modal = None;
                self.phase = Phase::Disconnected(Some(fl!("certificate-blocked")));
            }
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
            Message::Invoke(action) => return self.invoke(action),
            Message::Confirm => {
                if let Some(Modal::Confirm(action)) = self.modal.take() {
                    return self.collect_and_send(action);
                }
            }
            Message::CloseModal => self.modal = None,
            Message::FilePicked { id, path } => {
                if let Some(path) = path {
                    self.send_action(id, Some(ActionValue::File(path)));
                }
            }
            Message::SendAction(id) => self.send_action(id, None),
            Message::OpenUrl(url) => {
                if let Err(error) = open::that_detached(url.as_ref()) {
                    self.modal = Some(Modal::Error {
                        title: fl!("error-open-link"),
                        message: format!("{url}: {error}"),
                    });
                }
            }
            // The application owns the tab list and the settings, so it
            // intercepts these; this arm only satisfies exhaustiveness.
            Message::Close
            | Message::OpenSettings(_)
            | Message::ChangeSetting(_)
            | Message::ChooseRecordingFolder => {}
            Message::ToggleVirtualKeyboard => {
                self.show_virtual_keyboard = !self.show_virtual_keyboard;
            }
            Message::SetKeyboardSize(size) => {
                self.keyboard_size = size;
                self.show_virtual_keyboard = true;
            }
            Message::ToggleVirtualTrackpad => {
                self.show_virtual_trackpad = !self.show_virtual_trackpad;
            }
            Message::VirtualKeyboard(message) => {
                for (keysym, pressed) in
                    virtual_keyboard::update(&mut self.virtual_keyboard, message)
                {
                    if let Some(controls) = &self.controls {
                        let _ = controls.input.try_send(InputEvent::Key { keysym, pressed });
                    }
                }
            }
            Message::VirtualTrackpad(message) => {
                if let Some((width, height)) = self.size {
                    let effects = virtual_trackpad::update(
                        &mut self.virtual_trackpad,
                        message,
                        (width as f32, height as f32),
                    );
                    for effect in effects {
                        match effect {
                            virtual_trackpad::Effect::Move(point) => self.send_pointer(point),
                            virtual_trackpad::Effect::Click(button, pressed) => {
                                if let Some(controls) = &self.controls {
                                    let _ = controls
                                        .input
                                        .try_send(InputEvent::PointerButton { button, pressed });
                                }
                            }
                        }
                    }
                }
            }
            Message::ToggleMenu(menu) => {
                self.menu = match self.menu.take() {
                    Some(open) if open.menu == menu => None,
                    _ => Some(OpenMenu {
                        menu,
                        submenu: None,
                    }),
                };
            }
            Message::OpenSubmenu(submenu) => {
                if let Some(open) = &mut self.menu {
                    open.submenu = Some(submenu);
                }
            }
            Message::CloseMenu => {}
        }
        Task::none()
    }

    fn apply(&mut self, event: SessionEvent) {
        match event {
            SessionEvent::Connected { size, capabilities } => {
                self.size = Some(size);
                self.set_capabilities(capabilities);
                self.phase = Phase::Connected;
            }
            SessionEvent::Frame(frame) => {
                self.size = Some((frame.width, frame.height));
                let generation = self.next_generation;
                self.next_generation += 1;
                self.frame = Some(DisplayFrame {
                    tab_id: self.id,
                    generation,
                    width: frame.width,
                    height: frame.height,
                    pixels: Arc::from(frame.pixels),
                });
            }
            SessionEvent::Resized { width, height } => self.size = Some((width, height)),
            SessionEvent::Capabilities(capabilities) => self.set_capabilities(capabilities),
            SessionEvent::Status(status) => {
                if let Some(traffic) = status.traffic {
                    self.traffic.sample(Instant::now(), traffic);
                }
                self.session_status = status;
                self.refresh_icons();
            }
            SessionEvent::Disconnected { reason } => {
                self.phase = Phase::Disconnected(reason);
                self.controls = None;
            }
        }
    }

    fn set_capabilities(&mut self, capabilities: Capabilities) {
        // A contributed menu that disappeared can't stay open.
        if let Some(OpenMenu {
            menu: ToolbarMenu::Contributed(id),
            ..
        }) = &self.menu
        {
            let still_there = capabilities.toolbar.iter().any(|item| {
                matches!(item, server_remote_core::ToolbarItem::Menu { id: menu_id, .. } if menu_id == id)
            });
            if !still_there {
                self.menu = None;
            }
        }
        self.capabilities = capabilities;
        self.refresh_icons();
    }

    fn refresh_icons(&mut self) {
        self.icons.refresh(
            icons_in_capabilities(&self.capabilities).chain(icons_in_status(&self.session_status)),
        );
    }

    /// Invoking an action: confirm it first if asked to, then collect its
    /// value and send it.
    fn invoke(&mut self, action: Action) -> Task<Message> {
        if !action.enabled {
            return Task::none();
        }
        if action.confirm.is_some() {
            self.modal = Some(Modal::Confirm(action));
            return Task::none();
        }
        self.collect_and_send(action)
    }

    fn collect_and_send(&mut self, action: Action) -> Task<Message> {
        match action.control {
            Control::Button => {
                self.send_action(action.id, None);
                Task::none()
            }
            Control::File { extensions } => {
                let id = action.id;
                Task::perform(pick_file(extensions), move |path| Message::FilePicked {
                    id,
                    path,
                })
            }
        }
    }

    fn send_action(&self, id: Cow<'static, str>, value: Option<ActionValue>) {
        if let Some(controls) = &self.controls {
            let _ = controls
                .commands
                .try_send(SessionCommand::Action { id, value });
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

    pub fn view<'a>(&'a self, settings: &'a Settings) -> Element<'a, Message> {
        let body: Element<'_, Message> = match (&self.phase, &self.frame) {
            (Phase::Connecting, _) => text(fl!("status-connecting-ellipsis")).into(),
            (Phase::Disconnected(reason), _) => {
                let reason = reason.clone().unwrap_or_else(|| fl!("status-disconnected"));
                Column::new()
                    .spacing(8)
                    .align_x(Alignment::Center)
                    .push(text(reason))
                    .push(
                        button(text(fl!("close-tab")))
                            .style(button::subtle)
                            .on_press(Message::Close),
                    )
                    .into()
            }
            (Phase::Connected, None) => text(fl!("waiting-for-frame")).into(),
            (Phase::Connected, Some(frame)) => kvm_display(
                frame.clone(),
                self.capabilities.pointer,
                self.display_id.clone(),
            ),
        };

        let panel = (self.show_virtual_keyboard || self.show_virtual_trackpad).then(|| {
            let mut panel = Row::new().spacing(6).align_y(Alignment::Center);
            if self.show_virtual_keyboard {
                panel = panel.push(
                    virtual_keyboard::view(self.keyboard_size, &self.virtual_keyboard)
                        .map(Message::VirtualKeyboard),
                );
            }
            if self.show_virtual_trackpad {
                panel = panel.push(virtual_trackpad::view().map(Message::VirtualTrackpad));
            }
            center(panel).height(Length::Shrink)
        });

        let content = Column::new()
            .spacing(4)
            .push((!matches!(self.phase, Phase::Disconnected(_))).then(|| self.toolbar(settings)))
            .push(center(body))
            .push(panel)
            .push(self.status_bar());

        match &self.modal {
            Some(modal) => stack![content, modal_view(modal, &self.target)].into(),
            None => content.into(),
        }
    }

    fn toolbar<'a>(&'a self, settings: &'a Settings) -> Element<'a, Message> {
        let is_open =
            |menu: &ToolbarMenu| self.menu.as_ref().is_some_and(|open| open.menu == *menu);
        let button_menu = |menu: ToolbarMenu, icon: icons::Icon, items| {
            menu::button_menu(
                icon.into(),
                items,
                is_open(&menu),
                Message::ToggleMenu(menu),
                Message::CloseMenu,
            )
        };
        let submenu_open = |submenu: Submenu| {
            self.menu
                .as_ref()
                .is_some_and(|open| open.submenu == Some(submenu.clone()))
        };
        let contributed_menus =
            contributed::toolbar_menus(&self.capabilities, &self.icons, self.menu.as_ref());
        let chips = contributed::chips(&self.session_status, &self.icons);
        let links = contributed::toolbar_links(&self.capabilities, &self.icons);

        Row::new()
            .spacing(style::SPACING)
            .align_y(Alignment::Center)
            .push(button_menu(
                ToolbarMenu::Display,
                icons::SCREEN,
                display_menu(settings, &submenu_open),
            ))
            .push(button_menu(
                ToolbarMenu::Input,
                icons::KEYBOARD,
                self.input_menu(settings, &submenu_open),
            ))
            .push(button_menu(
                ToolbarMenu::Recording,
                icons::CAMERA,
                recording_menu(settings, &submenu_open),
            ))
            .push(style::vertical_divider(20.0))
            .push(
                (!contributed_menus.is_empty())
                    .then(|| with_divider(Row::with_children(contributed_menus))),
            )
            .push(Row::with_children(chips).spacing(style::SPACING))
            .push(Space::new().width(Length::Fill))
            .push(style::vertical_divider(20.0))
            .push(Row::with_children(links).spacing(style::SPACING))
            .push(menu::button_popout(
                icons::INFORMATION.into(),
                || self.info(),
                is_open(&ToolbarMenu::Info),
                Message::ToggleMenu(ToolbarMenu::Info),
                Message::CloseMenu,
            ))
            .into()
    }

    /// Technical details about the connection, for the info popout.
    fn info(&self) -> Element<'_, Message> {
        let status = match &self.phase {
            Phase::Connecting => fl!("status-connecting"),
            Phase::Connected => fl!("status-connected"),
            Phase::Disconnected(reason) => {
                reason.clone().unwrap_or_else(|| fl!("status-disconnected"))
            }
        };
        let resolution = self.size.map_or_else(
            || "—".to_owned(),
            |(width, height)| format!("{width}×{height}"),
        );
        let pointer = match self.capabilities.pointer {
            PointerMode::Absolute => fl!("pointer-absolute"),
            PointerMode::Relative => fl!("pointer-relative"),
        };

        let encryption = self
            .session_status
            .encryption
            .as_deref()
            .map_or_else(|| fl!("encryption-none"), str::to_owned);
        let built_in = [
            (fl!("info-protocol"), self.protocol.name().to_owned()),
            (
                fl!("info-address"),
                format!("{}:{}", self.target.host, self.target.port),
            ),
            (fl!("info-status"), status),
            (fl!("info-resolution"), resolution),
            (fl!("info-pointer"), pointer),
            (fl!("info-frames"), self.next_generation.to_string()),
            (fl!("info-encryption"), encryption),
        ];
        let details = self
            .session_status
            .details
            .iter()
            .map(|detail| (detail.label.to_string(), detail.value.to_string()));

        Column::with_children(built_in.into_iter().chain(details).map(|(label, value)| {
            Row::new()
                .spacing(16)
                .push(text(label).width(110).style(|theme: &Theme| text::Style {
                    color: Some(theme.extended_palette().background.strongest.color),
                }))
                .push(text(value))
                .into()
        }))
        .spacing(6)
        .width(280)
        .into()
    }

    fn input_menu(
        &self,
        settings: &Settings,
        submenu_open: &impl Fn(Submenu) -> bool,
    ) -> Vec<Item<Message>> {
        let actions = self
            .capabilities
            .input_actions
            .iter()
            .map(|action| contributed::action_item(action, &self.icons));
        let separator = (!self.capabilities.input_actions.is_empty()).then_some(Item::Separator);

        let keyboard_styles = KeyboardSize::ALL
            .into_iter()
            .map(|size| {
                Item::entry(size.to_string(), Some(Message::SetKeyboardSize(size)))
                    .checked(self.show_virtual_keyboard && size == self.keyboard_size)
            })
            .collect();

        actions
            .chain(separator)
            .chain([
                Item::entry(
                    fl!("menu-virtual-keyboard"),
                    Some(Message::ToggleVirtualKeyboard),
                )
                .icon(icons::KEYBOARD)
                .checked(self.show_virtual_keyboard),
                Item::entry(
                    fl!("menu-virtual-trackpad"),
                    Some(Message::ToggleVirtualTrackpad),
                )
                .icon(icons::CURSOR_1)
                .checked(self.show_virtual_trackpad),
                Item::Separator,
                submenu(
                    fl!("setting-keyboard-layout"),
                    Submenu::KeyboardLayout,
                    settings_ui::keyboard_layouts(settings, |layout| {
                        Message::ChangeSetting(Change::KeyboardLayout(layout))
                    }),
                    submenu_open,
                ),
                submenu(
                    fl!("setting-keyboard-style"),
                    Submenu::KeyboardStyle,
                    keyboard_styles,
                    submenu_open,
                ),
                Item::Separator,
                Item::entry(
                    fl!("menu-input-settings"),
                    Some(Message::OpenSettings(Section::Input)),
                )
                .icon(icons::SETTINGS),
            ])
            .collect()
    }

    fn status_bar(&self) -> Element<'_, Message> {
        let (label, indicator) = match self.phase {
            Phase::Connecting => (fl!("status-connecting-ellipsis"), style::Indicator::Warning),
            Phase::Connected => (fl!("status-connected"), style::Indicator::Ok),
            Phase::Disconnected(_) => (fl!("status-disconnected"), style::Indicator::Error),
        };
        let phase = Row::new()
            .spacing(6)
            .align_y(Alignment::Center)
            .push(style::status_dot(indicator))
            .push(text(label));

        let traffic = self.traffic.rate.map(|rate| {
            Row::new()
                .spacing(6)
                .align_y(Alignment::Center)
                .push(status_icon(icons::DOWNLOAD))
                .push(text(format_rate(rate.received)))
                .push(Space::new().width(6))
                .push(status_icon(icons::UPLOAD))
                .push(text(format_rate(rate.sent)))
                .into()
        });
        let resolution = self
            .size
            .map(|(width, height)| text(format!("{width}×{height}")).into());
        let encryption = self.session_status.encryption.as_ref().map(|encryption| {
            Row::new()
                .spacing(6)
                .align_y(Alignment::Center)
                .push(status_icon(icons::LOCKED))
                .push(text(encryption.as_ref()))
                .into()
        });
        let items = contributed::status_items(&self.session_status, &self.icons);
        let connector_items = (!items.is_empty()).then(|| {
            Row::with_children(items)
                .spacing(16)
                .align_y(Alignment::Center)
                .into()
        });

        // Groups are separated by dividers; the connector's own items sit
        // together in the last group.
        let groups: Vec<Element<'_, Message>> = [
            Some(phase.into()),
            traffic,
            resolution,
            encryption,
            connector_items,
        ]
        .into_iter()
        .flatten()
        .collect();
        let count = groups.len();
        let row = groups
            .into_iter()
            .enumerate()
            .fold(Row::new(), |row, (index, group)| {
                row.push(group)
                    .push((index + 1 < count).then(|| style::vertical_divider(16.0)))
            });

        row.spacing(12)
            .padding([0, 8])
            .height(style::CONTROL_HEIGHT)
            .align_y(Alignment::Center)
            .into()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        if let Phase::Disconnected(_) = self.phase {
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
                credentials: self.credentials.clone(),
            },
            worker,
        )
    }
}

/// A submenu that opens as `which`.
fn submenu(
    label: String,
    which: Submenu,
    items: Vec<Item<Message>>,
    submenu_open: &impl Fn(Submenu) -> bool,
) -> Item<Message> {
    Item::submenu(
        label,
        items,
        submenu_open(which.clone()),
        Some(Message::OpenSubmenu(which)),
    )
}

/// Display options, backed by the app-wide display settings. Zooming isn't
/// built yet, so those entries stay disabled.
fn display_menu(
    settings: &Settings,
    submenu_open: &impl Fn(Submenu) -> bool,
) -> Vec<Item<Message>> {
    let display = &settings.display;
    let scaling = |scaling: Scaling| Some(Message::ChangeSetting(Change::Scaling(scaling)));
    vec![
        Item::entry(
            fl!("setting-keep-aspect-ratio"),
            Some(Message::ChangeSetting(Change::KeepAspectRatio(
                !display.keep_aspect_ratio,
            ))),
        )
        .checked(display.keep_aspect_ratio),
        submenu(
            fl!("setting-display-scale"),
            Submenu::DisplayScale,
            settings_ui::display_scales(settings, |scale| {
                Message::ChangeSetting(Change::DisplayScale(scale))
            }),
            submenu_open,
        ),
        Item::Separator,
        Item::entry(fl!("scaling-fit"), scaling(Scaling::Fit))
            .checked(display.scaling == Scaling::Fit),
        Item::Separator,
        Item::entry(fl!("menu-zoom-out"), None),
        Item::entry("100%", scaling(Scaling::Actual)).checked(display.scaling == Scaling::Actual),
        Item::entry(fl!("menu-zoom-in"), None),
        Item::Separator,
        Item::entry(
            fl!("menu-display-settings"),
            Some(Message::OpenSettings(Section::Display)),
        )
        .icon(icons::SETTINGS),
    ]
}

/// Recording options, backed by the app-wide recording settings. Recording
/// itself isn't built yet, so starting one stays disabled.
fn recording_menu(
    settings: &Settings,
    submenu_open: &impl Fn(Submenu) -> bool,
) -> Vec<Item<Message>> {
    vec![
        Item::entry(fl!("menu-start-recording"), None).icon(icons::CAMERA),
        Item::Separator,
        submenu(
            fl!("setting-recording-folder"),
            Submenu::RecordingFolder,
            settings_ui::recording_folders(
                settings,
                Message::ChangeSetting,
                Message::ChooseRecordingFolder,
            ),
            submenu_open,
        ),
        submenu(
            fl!("setting-recording-quality"),
            Submenu::RecordingQuality,
            settings_ui::recording_qualities(settings, |quality| {
                Message::ChangeSetting(Change::RecordingQuality(quality))
            }),
            submenu_open,
        ),
        submenu(
            fl!("setting-recording-format"),
            Submenu::RecordingFormat,
            settings_ui::recording_formats(settings, |format| {
                Message::ChangeSetting(Change::RecordingFormat(format))
            }),
            submenu_open,
        ),
        Item::Separator,
        Item::entry(
            fl!("menu-recording-settings"),
            Some(Message::OpenSettings(Section::Recording)),
        )
        .icon(icons::SETTINGS),
    ]
}

fn status_icon<'a>(icon: icons::Icon) -> Element<'a, Message> {
    super::icon::view(icon.into(), style::ICON_SIZE, |theme| {
        theme.extended_palette().background.base.text
    })
}

/// `row` followed by a divider, for a group of toolbar buttons.
fn with_divider(row: Row<'_, Message>) -> Element<'_, Message> {
    Row::new()
        .spacing(style::SPACING)
        .align_y(Alignment::Center)
        .push(row.spacing(style::SPACING))
        .push(style::vertical_divider(20.0))
        .into()
}

fn icons_in_capabilities(
    capabilities: &Capabilities,
) -> impl Iterator<Item = &server_remote_core::Icon> {
    fn in_menu(items: &[server_remote_core::MenuItem]) -> Vec<&server_remote_core::Icon> {
        items
            .iter()
            .flat_map(|item| match item {
                server_remote_core::MenuItem::Action(action) => action.icon.iter().collect(),
                server_remote_core::MenuItem::Submenu { items, .. } => in_menu(items),
                server_remote_core::MenuItem::Separator => Vec::new(),
            })
            .collect()
    }

    let toolbar = capabilities.toolbar.iter().flat_map(|item| match item {
        server_remote_core::ToolbarItem::Menu { icon, items, .. } => std::iter::once(icon)
            .chain(in_menu(items))
            .collect::<Vec<_>>(),
        server_remote_core::ToolbarItem::Link { icon, .. } => vec![icon],
    });
    capabilities
        .input_actions
        .iter()
        .filter_map(|action| action.icon.as_ref())
        .chain(toolbar)
}

fn icons_in_status(status: &SessionStatus) -> impl Iterator<Item = &server_remote_core::Icon> {
    let items = status.items.iter().filter_map(|item| match item {
        server_remote_core::StatusItem::Text { icon, .. } => icon.as_ref(),
        server_remote_core::StatusItem::Indicator { .. } => None,
    });
    let chips = status.chips.iter().filter_map(|chip| chip.icon.as_ref());
    items.chain(chips)
}

async fn pick_file(extensions: Vec<Cow<'static, str>>) -> Option<PathBuf> {
    let dialog = rfd::AsyncFileDialog::new();
    let dialog = if extensions.is_empty() {
        dialog
    } else {
        dialog.add_filter(fl!("file-dialog-supported"), &extensions)
    };
    dialog
        .pick_file()
        .await
        .map(|file| file.path().to_path_buf())
}

fn modal_view<'a>(modal: &'a Modal, target: &'a Target) -> Element<'a, Message> {
    match modal {
        Modal::Confirm(action) => {
            let confirmation = action
                .confirm
                .as_ref()
                .expect("only actions with a confirmation are confirmed");
            let kind = if confirmation.destructive {
                dialog::Kind::Destructive
            } else {
                dialog::Kind::Primary
            };
            dialog::view(
                confirmation.title.to_string(),
                confirmation.message.to_string(),
                vec![
                    dialog::Button {
                        label: fl!("button-cancel"),
                        on_press: Message::CloseModal,
                        kind: dialog::Kind::Secondary,
                    },
                    dialog::Button {
                        label: confirmation.confirm_label.to_string(),
                        on_press: Message::Confirm,
                        kind,
                    },
                ],
            )
        }
        Modal::TrustCertificate(fingerprint) => {
            let trust = |remember| Message::TrustCertificate {
                fingerprint: *fingerprint,
                remember,
            };
            dialog::view_with(
                fl!("certificate-title"),
                fl!(
                    "certificate-message",
                    host = target.host.clone(),
                    fingerprint = fingerprint_lines(&fingerprint.to_string())
                ),
                vec![dialog::Button {
                    label: fl!("certificate-block"),
                    on_press: Message::BlockCertificate(*fingerprint),
                    kind: dialog::Kind::Secondary,
                }],
                vec![
                    dialog::Button {
                        label: fl!("button-cancel"),
                        on_press: Message::CloseModal,
                        kind: dialog::Kind::Secondary,
                    },
                    dialog::Button {
                        label: fl!("certificate-connect-once"),
                        on_press: trust(false),
                        kind: dialog::Kind::Secondary,
                    },
                    dialog::Button {
                        label: fl!("certificate-always-trust"),
                        on_press: trust(true),
                        kind: dialog::Kind::Primary,
                    },
                ],
                dialog::WIDE,
            )
        }
        Modal::Error { title, message } => dialog::view(
            title.clone(),
            message.clone(),
            vec![dialog::Button {
                label: fl!("button-ok"),
                on_press: Message::CloseModal,
                kind: dialog::Kind::Primary,
            }],
        ),
    }
}

/// Breaks a fingerprint ("AB:CD:…") onto a new line every 8 bytes: it has no
/// spaces, so it wouldn't otherwise wrap inside the dialog.
fn fingerprint_lines(fingerprint: &str) -> String {
    fingerprint
        .split(':')
        .collect::<Vec<_>>()
        .chunks(8)
        .map(|bytes| bytes.join(":"))
        .collect::<Vec<_>>()
        .join("\n")
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

pub(crate) fn character_keysym(character: char) -> u32 {
    let code = character as u32;
    // Latin-1 code points are their own keysyms; the rest use the Unicode form.
    if code <= 0xFF {
        code
    } else {
        0x0100_0000 + code
    }
}

pub(crate) fn named_keysym(named: Named) -> Option<u32> {
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
        Named::PrintScreen => 0xFF61,
        Named::ScrollLock => 0xFF14,
        Named::Pause => 0xFF13,
        Named::ContextMenu => 0xFF67,
        Named::NumLock => 0xFF7F,
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
    credentials: Credentials,
}

// `use<>` declares the returned stream captures nothing from `&Params`, so it
// is `'static` and the fn coerces to the `fn(&D) -> S` that `run_with` wants.
fn worker(params: &Params) -> impl Stream<Item = Message> + use<> {
    let protocol = params.protocol;
    let target = params.target.clone();
    let tls = params.tls.clone();
    let Credentials { username, password } = params.credentials.clone();

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
                Protocol::Ilo => {
                    IloConnector::new(username, password)
                        .connect(&target, &tls)
                        .await
                }
            };

            let handle = match result {
                Ok(handle) => handle,
                Err(ConnectError::UntrustedCertificate(fingerprint)) => {
                    let _ = output
                        .send(Message::UntrustedCertificate(fingerprint))
                        .await;
                    return;
                }
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
            Credentials::default(),
            &settings::Input::default(),
        )
    }

    #[test]
    fn new_connections_take_the_keyboard_defaults() {
        let cases = [
            (
                "defaults",
                settings::Input::default(),
                false,
                KeyboardSize::Medium,
            ),
            (
                "full size, shown on connect",
                settings::Input {
                    keyboard_style: KeyboardSize::Large,
                    open_keyboard_on_connect: true,
                    ..settings::Input::default()
                },
                true,
                KeyboardSize::Large,
            ),
        ];
        for (name, input, shown, size) in cases {
            let state = State::new(
                0,
                Arc::from("test"),
                Protocol::Dummy,
                Target::new("localhost", 0),
                TlsPolicy::default(),
                Credentials::default(),
                &input,
            );
            assert_eq!(state.show_virtual_keyboard, shown, "{name}");
            assert_eq!(state.keyboard_size, size, "{name}");
        }
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
            ..Capabilities::default()
        };
        state.apply(SessionEvent::Connected {
            size: (800, 600),
            capabilities,
        });

        assert_eq!(state.size, Some((800, 600)));
        assert_eq!(state.capabilities.pointer, PointerMode::Relative);
        assert_eq!(state.phase, Phase::Connected);
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
            state.phase,
            Phase::Disconnected(Some("connection reset".to_owned()))
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

    fn confirmed(action: Action) -> Action {
        action.confirm(server_remote_core::Confirmation {
            title: Cow::Borrowed("Sure?"),
            message: Cow::Borrowed("Really?"),
            confirm_label: Cow::Borrowed("Yes"),
            destructive: false,
        })
    }

    /// Actions sent to the session, as `(id, value)`.
    type Sent = Vec<(String, Option<ActionValue>)>;

    fn sent(command_rx: &mut mpsc::Receiver<SessionCommand>) -> Sent {
        std::iter::from_fn(|| command_rx.try_recv().ok())
            .map(|SessionCommand::Action { id, value }| (id.into_owned(), value))
            .collect()
    }

    #[test]
    fn action_flows() {
        let iso = PathBuf::from("/images/disk.iso");
        let cases: Vec<(&str, Vec<Message>, Sent)> = vec![
            (
                "button is sent straight away",
                vec![Message::Invoke(Action::button("cad", "Ctrl+Alt+Del"))],
                vec![("cad".into(), None)],
            ),
            (
                "disabled action is ignored",
                vec![Message::Invoke(
                    Action::button("cad", "Ctrl+Alt+Del").enabled(false),
                )],
                vec![],
            ),
            (
                "confirmation holds the action back",
                vec![Message::Invoke(confirmed(Action::button(
                    "off",
                    "Power off",
                )))],
                vec![],
            ),
            (
                "confirming sends it",
                vec![
                    Message::Invoke(confirmed(Action::button("off", "Power off"))),
                    Message::Confirm,
                ],
                vec![("off".into(), None)],
            ),
            (
                "cancelling drops it",
                vec![
                    Message::Invoke(confirmed(Action::button("off", "Power off"))),
                    Message::CloseModal,
                    Message::Confirm,
                ],
                vec![],
            ),
            (
                "picked file is sent as the value",
                vec![Message::FilePicked {
                    id: Cow::Borrowed("mount"),
                    path: Some(iso.clone()),
                }],
                vec![("mount".into(), Some(ActionValue::File(iso.clone())))],
            ),
            (
                "cancelled file picker sends nothing",
                vec![Message::FilePicked {
                    id: Cow::Borrowed("mount"),
                    path: None,
                }],
                vec![],
            ),
            (
                "chip close sends its action",
                vec![Message::SendAction(Cow::Borrowed("eject"))],
                vec![("eject".into(), None)],
            ),
        ];

        for (name, messages, expected) in cases {
            let mut state = state();
            let (controls, _input_rx, mut command_rx) = wired_controls();
            state.controls = Some(controls);
            for message in messages {
                let _ = state.update(message);
            }
            assert_eq!(sent(&mut command_rx), expected, "{name}");
        }
    }

    #[test]
    fn an_untrusted_certificate_asks_and_trusting_it_reconnects() {
        let mut state = state();
        let fingerprint = CertFingerprint([7; 32]);
        let _ = state.update(Message::UntrustedCertificate(fingerprint));
        assert!(matches!(state.phase, Phase::Disconnected(Some(_))));
        assert!(matches!(state.modal, Some(Modal::TrustCertificate(f)) if f == fingerprint));

        let _ = state.update(Message::TrustCertificate {
            fingerprint,
            remember: false,
        });
        assert!(state.modal.is_none());
        assert_eq!(state.phase, Phase::Connecting);
        assert_eq!(
            state.tls,
            TlsPolicy::AllowInsecure {
                accepted: Some(fingerprint)
            }
        );
    }

    #[test]
    fn fingerprints_break_every_eight_bytes() {
        let shown = fingerprint_lines(&CertFingerprint([0xAB; 32]).to_string());
        let lines: Vec<&str> = shown.lines().collect();
        assert_eq!(lines.len(), 4);
        assert!(lines.iter().all(|line| *line == "AB:AB:AB:AB:AB:AB:AB:AB"));
    }

    #[test]
    fn declining_a_certificate_stays_disconnected() {
        let mut state = state();
        let _ = state.update(Message::UntrustedCertificate(CertFingerprint([1; 32])));
        let _ = state.update(Message::CloseModal);
        assert!(state.modal.is_none());
        assert!(matches!(state.phase, Phase::Disconnected(_)));
        assert_eq!(state.tls, TlsPolicy::default());
    }

    #[test]
    fn blocking_a_certificate_stays_disconnected_and_says_why() {
        let mut state = state();
        let fingerprint = CertFingerprint([1; 32]);
        let _ = state.update(Message::UntrustedCertificate(fingerprint));
        let _ = state.update(Message::BlockCertificate(fingerprint));
        assert!(state.modal.is_none());
        assert_eq!(
            state.phase,
            Phase::Disconnected(Some(fl!("certificate-blocked")))
        );
        assert_eq!(state.tls, TlsPolicy::default());
    }

    #[test]
    fn invoking_a_confirmed_action_opens_the_dialog() {
        let mut state = state();
        let _ = state.update(Message::Invoke(confirmed(Action::button(
            "off",
            "Power off",
        ))));
        assert!(matches!(state.modal, Some(Modal::Confirm(_))));
    }

    #[test]
    fn status_event_is_stored() {
        let mut state = state();
        state.apply(SessionEvent::Status(SessionStatus {
            encryption: Some(Cow::Borrowed("RC4")),
            ..SessionStatus::default()
        }));
        assert_eq!(state.session_status.encryption.as_deref(), Some("RC4"));
    }

    #[test]
    fn traffic_rates() {
        let start = Instant::now();
        let at = |millis| start + Duration::from_millis(millis);
        let traffic = |received, sent| Traffic { received, sent };
        let cases = [
            ("one sample has no rate", vec![(0, traffic(0, 0))], None),
            (
                "rate over one second",
                vec![(0, traffic(0, 0)), (1_000, traffic(1_000, 500))],
                Some(Rate {
                    received: 1_000.0,
                    sent: 500.0,
                }),
            ),
            (
                "too-close samples are skipped",
                vec![(0, traffic(0, 0)), (100, traffic(900, 900))],
                None,
            ),
            (
                "rate measured from the last kept sample",
                vec![
                    (0, traffic(0, 0)),
                    (100, traffic(900, 900)),
                    (2_000, traffic(4_000, 2_000)),
                ],
                Some(Rate {
                    received: 2_000.0,
                    sent: 1_000.0,
                }),
            ),
        ];

        for (name, samples, expected) in cases {
            let mut meter = TrafficMeter::default();
            for (millis, traffic) in samples {
                meter.sample(at(millis), traffic);
            }
            assert_eq!(meter.rate, expected, "{name}");
        }
    }

    #[test]
    fn rate_formatting() {
        let cases = [
            (0.0, "0 kb/s"),
            (18_750.0, "150 kb/s"),
            (124_999.0, "1000 kb/s"),
            (125_000.0, "1.0 Mb/s"),
            (1_500_000.0, "12.0 Mb/s"),
        ];
        for (bytes_per_second, expected) in cases {
            assert_eq!(
                format_rate(bytes_per_second),
                expected,
                "{bytes_per_second}"
            );
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
        assert_eq!(named_keysym(Named::PrintScreen), Some(0xFF61));
        assert_eq!(named_keysym(Named::ScrollLock), Some(0xFF14));
        assert_eq!(named_keysym(Named::Pause), Some(0xFF13));
        assert_eq!(named_keysym(Named::ContextMenu), Some(0xFF67));
        assert_eq!(named_keysym(Named::NumLock), Some(0xFF7F));
        assert_eq!(named_keysym(Named::Fn), None);
    }

    #[test]
    fn keysym_dispatches_on_key_kind() {
        assert_eq!(keysym(&keyboard::Key::Character("z".into())), Some(0x7A));
        assert_eq!(keysym(&keyboard::Key::Named(Named::Escape)), Some(0xFF1B));
        assert_eq!(keysym(&keyboard::Key::Unidentified), None);
    }
}
