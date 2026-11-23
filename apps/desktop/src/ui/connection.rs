//! A single connection tab: renders a [`Connection`] — its framebuffer, a
//! toolbar of dropdown menus and a status bar — and runs it on a background
//! subscription ([`connection::run`]).
//!
//! The session logic lives in [`server_remote_app::connection`]; this keeps
//! what only the desktop UI needs: open menus, keyboard focus, the virtual
//! keyboard and trackpad.

use crate::i18n::fl;
use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;

use iced::keyboard::key::Named;
use iced::{
    Alignment, Element, Length, Point, Subscription, Task, Theme,
    futures::{SinkExt, Stream, channel::mpsc as futures_mpsc},
    keyboard,
    widget::{Column, Id, Row, Space, button, center, operation, stack, text},
};
use server_remote_app::connection::{self, Connection, Dialog, Params, Phase, Report, format_rate};
use server_remote_core::{
    Action, Capabilities, CertFingerprint, PointerButton, PointerMode, SessionEvent,
    Status as SessionStatus, Target,
};
use server_remote_icons as icons;
use tokio::sync::mpsc;

use super::contributed;
use super::dialog;
use super::icon;
use super::kvm_display::{DisplayFrame, kvm_display};
use super::menu::{self, Item};
use super::settings::{self as settings_ui, Section};
use super::style;
use super::virtual_keyboard;
use super::virtual_trackpad;
use crate::persistence::settings::Change;
use crate::persistence::settings::{self as settings, KeyboardSize, Scaling, Settings};

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

pub struct State {
    core: Connection,
    /// Identifies the [`kvm_display`] widget so it can be focused
    /// programmatically (see the first frame's handling in `update`).
    display_id: Id,
    /// Whether the virtual on-screen keyboard panel is shown, and its layout.
    show_virtual_keyboard: bool,
    keyboard_size: KeyboardSize,
    virtual_keyboard: virtual_keyboard::State,
    /// Whether the virtual trackpad panel is shown.
    show_virtual_trackpad: bool,
    virtual_trackpad: virtual_trackpad::State,
    menu: Option<OpenMenu>,
    icons: icon::Cache,
}

#[derive(Debug, Clone)]
pub enum Message {
    /// From the session runner. The application refuses an untrusted
    /// certificate itself if settings say so.
    Report(Report),
    /// Trust the certificate the dialog showed, and connect again. The
    /// application also saves it when `remember` is set.
    TrustCertificate {
        fingerprint: CertFingerprint,
        remember: bool,
    },
    /// Refuse the certificate the dialog showed. Handled by the application,
    /// which adds it to the blocked list and closes the tab.
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
    pub fn new(core: Connection, input: &settings::Input) -> Self {
        Self {
            core,
            display_id: Id::unique(),
            show_virtual_keyboard: input.open_keyboard_on_connect,
            keyboard_size: input.keyboard_style,
            virtual_keyboard: virtual_keyboard::State::default(),
            show_virtual_trackpad: false,
            virtual_trackpad: virtual_trackpad::State::default(),
            menu: None,
            icons: icon::Cache::default(),
        }
    }

    pub fn core(&self) -> &Connection {
        &self.core
    }

    pub fn id(&self) -> u64 {
        self.core.id()
    }

    pub fn name(&self) -> &Arc<str> {
        self.core.name()
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        if message.closes_menu() {
            self.menu = None;
        }
        match message {
            Message::Report(report) => return self.report(report),
            Message::TrustCertificate {
                fingerprint,
                remember,
            } => {
                return self.send(connection::Event::TrustCertificate {
                    fingerprint,
                    remember,
                });
            }
            Message::Pointer(point) => {
                return self.send(connection::Event::Pointer {
                    x: point.x,
                    y: point.y,
                });
            }
            Message::Button { button, pressed } => {
                return self.send(connection::Event::Button { button, pressed });
            }
            Message::Key { key, pressed } => {
                if let Some(keysym) = keysym(&key) {
                    return self.send(connection::Event::Key { keysym, pressed });
                }
            }
            Message::Invoke(action) => return self.send(connection::Event::Invoke(action)),
            Message::Confirm => return self.send(connection::Event::Confirm),
            Message::CloseModal => return self.send(connection::Event::CloseDialog),
            Message::FilePicked { id, path } => {
                return self.send(connection::Event::FilePicked { id, path });
            }
            Message::SendAction(id) => return self.send(connection::Event::SendAction(id)),
            Message::OpenUrl(url) => {
                if let Err(error) = open::that_detached(url.as_ref()) {
                    return self.send(connection::Event::OpenUrlFailed {
                        url: url.into_owned(),
                        error: error.to_string(),
                    });
                }
            }
            // The application owns the tab list and the settings, so it
            // intercepts these; this arm only satisfies exhaustiveness.
            Message::Close
            | Message::BlockCertificate(_)
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
                    let _ = self.send(connection::Event::Key { keysym, pressed });
                }
            }
            Message::VirtualTrackpad(message) => {
                let Some((width, height)) = self.core.size() else {
                    return Task::none();
                };
                let effects = virtual_trackpad::update(
                    &mut self.virtual_trackpad,
                    message,
                    (width as f32, height as f32),
                );
                for effect in effects {
                    let event = match effect {
                        virtual_trackpad::Effect::Move(point) => connection::Event::Pointer {
                            x: point.x,
                            y: point.y,
                        },
                        virtual_trackpad::Effect::Click(button, pressed) => {
                            connection::Event::Button { button, pressed }
                        }
                    };
                    let _ = self.send(event);
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

    /// Hands `event` to the connection and carries out what it asks for.
    fn send(&mut self, event: connection::Event) -> Task<Message> {
        match self.core.update(event) {
            Some(connection::Effect::PickFile { id, extensions }) => {
                Task::perform(pick_file(extensions), move |path| Message::FilePicked {
                    id,
                    path,
                })
            }
            None => Task::none(),
        }
    }

    fn report(&mut self, report: Report) -> Task<Message> {
        // Grabs focus the moment the `kvm_display` widget first appears
        // (when the first frame arrives — before that, `view` still shows
        // "Waiting for first frame…" and there's nothing in the tree to
        // focus), so typing works immediately without an initial click.
        let first_frame = matches!(report, Report::Session(SessionEvent::Frame(_)))
            && self.core.screen().is_none();
        let refresh = matches!(
            report,
            Report::Session(
                SessionEvent::Connected { .. }
                    | SessionEvent::Capabilities(_)
                    | SessionEvent::Status(_)
            )
        );
        let _ = self.send(connection::Event::Report(report));
        if refresh {
            self.close_vanished_menu();
            self.refresh_icons();
        }
        if first_frame {
            return operation::focus(self.display_id.clone());
        }
        Task::none()
    }

    /// A contributed menu that disappeared can't stay open.
    fn close_vanished_menu(&mut self) {
        let Some(OpenMenu {
            menu: ToolbarMenu::Contributed(id),
            ..
        }) = &self.menu
        else {
            return;
        };
        let still_there = self.core.capabilities().toolbar.iter().any(|item| {
            matches!(item, server_remote_core::ToolbarItem::Menu { id: menu_id, .. } if menu_id == id)
        });
        if !still_there {
            self.menu = None;
        }
    }

    fn refresh_icons(&mut self) {
        self.icons.refresh(
            icons_in_capabilities(self.core.capabilities())
                .chain(icons_in_status(self.core.status())),
        );
    }

    /// The screen as the display widget takes it.
    fn display_frame(&self) -> Option<DisplayFrame> {
        self.core.screen().map(|screen| DisplayFrame {
            tab_id: self.core.id(),
            generation: screen.generation,
            width: screen.width,
            height: screen.height,
            pixels: screen.pixels.clone(),
        })
    }

    pub fn view<'a>(&'a self, settings: &'a Settings) -> Element<'a, Message> {
        let body: Element<'_, Message> = match (self.core.phase(), self.display_frame()) {
            (Phase::Connecting, _) => text(fl!("status-connecting-ellipsis")).into(),
            (Phase::Disconnected(reason), _) => {
                let reason = reason.clone().unwrap_or_else(|| fl!("status-disconnected"));
                Column::new()
                    .spacing(8)
                    .align_x(Alignment::Center)
                    .push(text(reason))
                    .push(
                        button(
                            text(fl!("close-tab"))
                                .height(Length::Fill)
                                .align_y(Alignment::Center),
                        )
                        .height(style::CONTROL_HEIGHT)
                        .style(button::subtle)
                        .on_press(Message::Close),
                    )
                    .into()
            }
            (Phase::Connected, None) => text(fl!("waiting-for-frame")).into(),
            (Phase::Connected, Some(frame)) => kvm_display(
                frame,
                self.core.capabilities().pointer,
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
            .push(
                (!matches!(self.core.phase(), Phase::Disconnected(_)))
                    .then(|| self.toolbar(settings)),
            )
            .push(center(body))
            .push(panel)
            .push(self.status_bar());

        match self.core.dialog() {
            Some(modal) => stack![content, modal_view(modal, self.core.target())].into(),
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
            contributed::toolbar_menus(self.core.capabilities(), &self.icons, self.menu.as_ref());
        let chips = contributed::chips(self.core.status(), &self.icons);
        let links = contributed::toolbar_links(self.core.capabilities(), &self.icons);

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
        let status = match self.core.phase() {
            Phase::Connecting => fl!("status-connecting"),
            Phase::Connected => fl!("status-connected"),
            Phase::Disconnected(reason) => {
                reason.clone().unwrap_or_else(|| fl!("status-disconnected"))
            }
        };
        let resolution = self.core.size().map_or_else(
            || "—".to_owned(),
            |(width, height)| format!("{width}×{height}"),
        );
        let pointer = match self.core.capabilities().pointer {
            PointerMode::Absolute => fl!("pointer-absolute"),
            PointerMode::Relative => fl!("pointer-relative"),
        };

        let encryption = self
            .core
            .status()
            .encryption
            .as_deref()
            .map_or_else(|| fl!("encryption-none"), str::to_owned);
        let built_in = [
            (fl!("info-protocol"), self.core.protocol().name().to_owned()),
            (
                fl!("info-address"),
                format!("{}:{}", self.core.target().host, self.core.target().port),
            ),
            (fl!("info-status"), status),
            (fl!("info-resolution"), resolution),
            (fl!("info-pointer"), pointer),
            (fl!("info-frames"), self.core.frames().to_string()),
            (fl!("info-encryption"), encryption),
        ];
        let details = self
            .core
            .status()
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
            .core
            .capabilities()
            .input_actions
            .iter()
            .map(|action| contributed::action_item(action, &self.icons));
        let separator =
            (!self.core.capabilities().input_actions.is_empty()).then_some(Item::Separator);

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
        let (label, indicator) = match self.core.phase() {
            Phase::Connecting => (fl!("status-connecting-ellipsis"), style::Indicator::Warning),
            Phase::Connected => (fl!("status-connected"), style::Indicator::Ok),
            Phase::Disconnected(_) => (fl!("status-disconnected"), style::Indicator::Error),
        };
        let phase = Row::new()
            .spacing(6)
            .align_y(Alignment::Center)
            .push(style::status_dot(indicator))
            .push(text(label));

        let traffic = self.core.traffic().map(|rate| {
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
            .core
            .size()
            .map(|(width, height)| text(format!("{width}×{height}")).into());
        let encryption = self.core.status().encryption.as_ref().map(|encryption| {
            Row::new()
                .spacing(6)
                .align_y(Alignment::Center)
                .push(status_icon(icons::LOCKED))
                .push(text(encryption.as_ref()))
                .into()
        });
        let items = contributed::status_items(self.core.status(), &self.icons);
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
        // `run_with` identifies the subscription by hashing `Params` (plus the
        // worker's function pointer); the connection's id is among them, so
        // each connection's subscription is distinct. The worker must be a
        // plain `fn`, so everything it needs travels in `Params`.
        match self.core.params() {
            Some(params) => Subscription::run_with(params, worker),
            None => Subscription::none(),
        }
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

fn modal_view<'a>(modal: &'a Dialog, target: &'a Target) -> Element<'a, Message> {
    match modal {
        Dialog::Confirm(action) => {
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
                        icon: None,
                        enabled: true,
                    },
                    dialog::Button {
                        label: confirmation.confirm_label.to_string(),
                        on_press: Message::Confirm,
                        kind,
                        icon: None,
                        enabled: true,
                    },
                ],
            )
        }
        Dialog::TrustCertificate(fingerprint) => {
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
                    icon: None,
                    enabled: true,
                }],
                vec![
                    dialog::Button {
                        label: fl!("button-cancel"),
                        on_press: Message::Close,
                        kind: dialog::Kind::Secondary,
                        icon: None,
                        enabled: true,
                    },
                    dialog::Button {
                        label: fl!("certificate-connect-once"),
                        on_press: trust(false),
                        kind: dialog::Kind::Secondary,
                        icon: None,
                        enabled: true,
                    },
                    dialog::Button {
                        label: fl!("certificate-always-trust"),
                        on_press: trust(true),
                        kind: dialog::Kind::Primary,
                        icon: None,
                        enabled: true,
                    },
                ],
                dialog::WIDE,
            )
        }
        Dialog::Error { title, message } => dialog::view(
            title.clone(),
            message.clone(),
            vec![dialog::Button {
                label: fl!("button-ok"),
                on_press: Message::CloseModal,
                kind: dialog::Kind::Primary,
                icon: None,
                enabled: true,
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

// `use<>` declares the returned stream captures nothing from `&Params`, so it
// is `'static` and the fn coerces to the `fn(&D) -> S` that `run_with` wants.
fn worker(params: &Params) -> impl Stream<Item = Message> + use<> {
    let params = params.clone();
    iced::stream::channel(
        32,
        move |mut output: futures_mpsc::Sender<Message>| async move {
            let (reports, mut received) = mpsc::channel(32);
            let forward = async move {
                while let Some(report) = received.recv().await {
                    if output.send(Message::Report(report)).await.is_err() {
                        break;
                    }
                }
            };
            // Dropping the subscription drops both, which ends the session.
            iced::futures::join!(connection::run(params, reports), forward);
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use server_remote_app::Protocol;
    use server_remote_app::connection::Credentials;
    use server_remote_core::TlsPolicy;

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
            let core = Connection::new(
                0,
                Arc::from("test"),
                Protocol::Dummy,
                Target::new("localhost", 0),
                TlsPolicy::default(),
                Credentials::default(),
            );
            let state = State::new(core, &input);
            assert_eq!(state.show_virtual_keyboard, shown, "{name}");
            assert_eq!(state.keyboard_size, size, "{name}");
        }
    }

    #[test]
    fn fingerprints_break_every_eight_bytes() {
        let shown = fingerprint_lines(&CertFingerprint([0xAB; 32]).to_string());
        let lines: Vec<&str> = shown.lines().collect();
        assert_eq!(lines.len(), 4);
        assert!(lines.iter().all(|line| *line == "AB:AB:AB:AB:AB:AB:AB:AB"));
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
