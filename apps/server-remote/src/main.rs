use std::path::PathBuf;
use std::sync::Arc;

use iced::{
    Alignment, Element, Length, Subscription, Task, padding,
    widget::{Column, Row, Scrollable, Space, button, container, mouse_area, stack, text, tooltip},
    window,
};
use server_remote_core::{Target, TlsPolicy};
use server_remote_icons as icons;

use crate::persistence::settings::{GraphicsBackend, Settings};
use crate::tooltip_box::tooltip_box;

mod persistence;
mod tooltip_box;
mod ui;
mod update;

/// Split a `host[:port]` string, falling back to the protocol's default port.
fn parse_target(hostname: &str, protocol: ui::connection::Protocol) -> Target {
    let default_port = protocol.default_port();
    if hostname.is_empty() {
        return Target::new("localhost", default_port);
    }
    match hostname.rsplit_once(':') {
        Some((host, port)) => match port.parse::<u16>() {
            Ok(port) => Target::new(host, port),
            Err(_) => Target::new(hostname, default_port),
        },
        None => Target::new(hostname, default_port),
    }
}

/// Builds a [`persistence::sessions::SavedSession`] from a connect-tab
/// request, falling back to the hostname and then the protocol label when the
/// user left the name field blank.
fn saved_session_from_request(
    request: ui::connect::Request,
) -> persistence::sessions::SavedSession {
    let name = if !request.name.is_empty() {
        request.name
    } else if !request.hostname.is_empty() {
        request.hostname.clone()
    } else {
        request.protocol.to_string()
    };

    persistence::sessions::SavedSession {
        name,
        protocol: request.protocol,
        hostname: request.hostname,
        password: request.password,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_hostname_falls_back_to_localhost_and_default_port() {
        let target = parse_target("", ui::connection::Protocol::Vnc);
        assert_eq!(target, Target::new("localhost", 5900));
    }

    #[test]
    fn host_without_port_uses_the_protocol_default() {
        let target = parse_target("example.com", ui::connection::Protocol::Ilo);
        assert_eq!(target, Target::new("example.com", 443));
    }

    #[test]
    fn host_with_valid_port_overrides_the_default() {
        let target = parse_target("example.com:2222", ui::connection::Protocol::Vnc);
        assert_eq!(target, Target::new("example.com", 2222));
    }

    #[test]
    fn host_with_unparsable_port_keeps_the_whole_string_as_host() {
        let target = parse_target("example.com:not-a-port", ui::connection::Protocol::Vnc);
        assert_eq!(target, Target::new("example.com:not-a-port", 5900));
    }

    #[test]
    fn host_with_out_of_range_port_keeps_the_whole_string_as_host() {
        let target = parse_target("example.com:99999", ui::connection::Protocol::Vnc);
        assert_eq!(target, Target::new("example.com:99999", 5900));
    }

    fn request(hostname: &str) -> ui::connect::Request {
        ui::connect::Request {
            protocol: ui::connection::Protocol::Dummy,
            name: String::new(),
            hostname: hostname.to_owned(),
            password: String::new(),
        }
    }

    #[test]
    fn open_connection_assigns_increasing_ids_and_switches_to_the_new_tab() {
        let (mut app, _) = Application::new(None, Settings::default());

        let _ = app.open_connection(request("first"));
        let _ = app.open_connection(request("second"));

        assert_eq!(app.connections.len(), 2);
        assert_eq!(app.connections[0].id(), 0);
        assert_eq!(app.connections[1].id(), 1);
        assert_eq!(app.tab, Tab::Connection(1));
    }

    #[test]
    fn open_connection_names_an_empty_hostname_after_the_protocol_and_id() {
        let (mut app, _) = Application::new(None, Settings::default());

        let _ = app.open_connection(request(""));

        assert_eq!(app.connections[0].name().as_ref(), "dummy #0");
    }

    #[test]
    fn close_connection_removes_it_and_returns_to_the_connect_tab() {
        let (mut app, _) = Application::new(None, Settings::default());
        let _ = app.open_connection(request("only"));
        assert_eq!(app.tab, Tab::Connection(0));

        app.close_connection(0);

        assert!(app.connections.is_empty());
        assert_eq!(app.tab, Tab::Connect);
    }

    #[test]
    fn close_connection_leaves_an_unrelated_active_tab_alone() {
        let (mut app, _) = Application::new(None, Settings::default());
        let _ = app.open_connection(request("first"));
        let _ = app.open_connection(request("second"));
        app.tab = Tab::Manager;

        app.close_connection(0);

        assert_eq!(app.connections.len(), 1);
        assert_eq!(app.tab, Tab::Manager);
    }

    #[test]
    fn saved_session_uses_the_provided_name_when_set() {
        let mut req = request("example.com");
        req.name = "Home NAS".to_owned();

        let saved = saved_session_from_request(req);

        assert_eq!(saved.name, "Home NAS");
        assert_eq!(saved.hostname, "example.com");
    }

    #[test]
    fn saved_session_falls_back_to_hostname_when_name_is_blank() {
        let saved = saved_session_from_request(request("example.com"));
        assert_eq!(saved.name, "example.com");
    }

    /// Renders the window chrome to PNGs for comparing against the design.
    /// Run with `SNAPSHOT_DIR=<dir> cargo test -p server-remote -- --ignored`.
    /// Capabilities shaped like the iLO mockups, for review snapshots.
    fn ilo_like_capabilities() -> server_remote_core::Capabilities {
        use server_remote_core::{
            Action, Capabilities, Confirmation, Control, MenuItem, ToolbarItem,
        };
        use server_remote_icons as icons;

        let power = |id: &'static str, label: &'static str, icon, verb: &'static str, hard| {
            MenuItem::Action(Action::button(id, label).icon(icon).confirm(Confirmation {
                title: format!("{verb}?").into(),
                message: if hard {
                    "Cuts power immediately. Unsaved work will be lost.".into()
                } else {
                    "Asks the operating system to do this gracefully.".into()
                },
                confirm_label: verb.into(),
                destructive: hard,
            }))
        };
        Capabilities {
            input_actions: vec![Action::button("cad", "Ctrl+Alt+Delete").icon(icons::RESTART)],
            toolbar: vec![
                ToolbarItem::Menu {
                    id: "power".into(),
                    icon: icons::POWER.into(),
                    label: "Power".into(),
                    items: vec![
                        power(
                            "power.shut-down",
                            "Shut down…",
                            icons::POWER,
                            "Shut down",
                            false,
                        ),
                        power(
                            "power.restart",
                            "Restart…",
                            icons::RESTART,
                            "Restart",
                            false,
                        ),
                        MenuItem::Separator,
                        power("power.off", "Power off…", icons::POWER, "Power off", true),
                        power(
                            "power.cold-boot",
                            "Cold boot…",
                            icons::RESTART,
                            "Cold boot",
                            true,
                        ),
                    ],
                },
                ToolbarItem::Menu {
                    id: "media".into(),
                    icon: icons::VMDK_DISK.into(),
                    label: "Virtual media".into(),
                    items: vec![MenuItem::Action(
                        Action::button("media.mount", "Mount image…").control(Control::File {
                            extensions: vec!["iso".into()],
                        }),
                    )],
                },
                ToolbarItem::Link {
                    icon: icons::LAUNCH.into(),
                    label: "Open iLO in web browser".into(),
                    url: "https://ilo.invalid".into(),
                },
            ],
            ..Capabilities::default()
        }
    }

    /// Connects the first tab to an iLO-like session, with two status
    /// samples far enough apart for traffic rates to show.
    fn feed_ilo_like_session(app: &mut Application) {
        use server_remote_core::{Chip, IndicatorState, SessionEvent, Status, StatusItem, Traffic};
        use server_remote_icons as icons;

        let status = |received, sent| Status {
            encryption: Some("RC4".into()),
            traffic: Some(Traffic { received, sent }),
            items: vec![
                StatusItem::Indicator {
                    label: "Powered on".into(),
                    state: IndicatorState::Ok,
                },
                StatusItem::Indicator {
                    label: "POST: 5000".into(),
                    state: IndicatorState::Inactive,
                },
                StatusItem::Indicator {
                    label: "ACT".into(),
                    state: IndicatorState::Ok,
                },
                StatusItem::Indicator {
                    label: "Health".into(),
                    state: IndicatorState::Warning,
                },
            ],
            chips: vec![Chip {
                icon: Some(icons::PROGRESS_BAR_ROUND.into()),
                label: "proxmox-ve_9.2-1.iso".into(),
                close_action: Some("media.eject".into()),
            }],
            details: Vec::new(),
        };
        let session = |event| Message::Connection(0, ui::connection::Message::Session(event));

        let _ = app.update(session(SessionEvent::Connected {
            size: (1024, 768),
            capabilities: ilo_like_capabilities(),
        }));
        let _ = app.update(session(SessionEvent::Status(status(0, 0))));
        // 150 kb/s down and 95 kb/s up over the 0.6 s between samples.
        std::thread::sleep(std::time::Duration::from_millis(600));
        let _ = app.update(session(SessionEvent::Status(status(11_250, 7_125))));
    }

    #[test]
    #[ignore = "writes review snapshots; needs SNAPSHOT_DIR"]
    fn render_review_snapshots() {
        let dir =
            std::path::PathBuf::from(std::env::var("SNAPSHOT_DIR").expect("SNAPSHOT_DIR is set"));
        let settings = ui::style::settings();
        let theme = ui::style::theme();

        use ui::connection::{Message as Conn, ToolbarMenu};

        let power_off = ilo_like_capabilities()
            .toolbar
            .into_iter()
            .find_map(|item| match item {
                server_remote_core::ToolbarItem::Menu { items, .. } => {
                    items.into_iter().find_map(|item| match item {
                        server_remote_core::MenuItem::Action(action)
                            if action.id == "power.off" =>
                        {
                            Some(action)
                        }
                        _ => None,
                    })
                }
                server_remote_core::ToolbarItem::Link { .. } => None,
            })
            .expect("the power menu has a power-off action");

        let conn = |message| Message::Connection(0, message);
        let settings_page = |section, select| {
            vec![
                Message::SelectTab(Tab::Settings),
                Message::Settings(ui::settings::Message::Section(section)),
                Message::Settings(ui::settings::Message::Toggle(select)),
            ]
        };

        // (name, whether a connector session is fed in first, messages)
        let mut states: Vec<(&str, bool, Vec<Message>)> = vec![
            ("window", false, vec![]),
            ("connector", true, vec![]),
            (
                "power-menu",
                true,
                vec![conn(Conn::ToggleMenu(ToolbarMenu::Contributed(
                    "power".into(),
                )))],
            ),
            ("confirm", true, vec![conn(Conn::Invoke(power_off))]),
            (
                "display-menu",
                false,
                vec![conn(Conn::ToggleMenu(ToolbarMenu::Display))],
            ),
            (
                "input-menu",
                false,
                vec![
                    conn(Conn::ToggleMenu(ToolbarMenu::Input)),
                    conn(Conn::OpenSubmenu(ui::connection::Submenu::KeyboardStyle)),
                ],
            ),
            (
                "keyboard",
                false,
                vec![conn(Conn::SetKeyboardSize(
                    ui::connection::KeyboardSize::Large,
                ))],
            ),
            (
                "recording-menu",
                false,
                vec![
                    conn(Conn::ToggleMenu(ToolbarMenu::Recording)),
                    conn(Conn::OpenSubmenu(ui::connection::Submenu::RecordingFormat)),
                ],
            ),
            (
                "settings-input",
                false,
                settings_page(
                    ui::settings::Section::Input,
                    ui::settings::Select::KeyboardLayout,
                ),
            ),
            (
                "settings-recording",
                false,
                settings_page(
                    ui::settings::Section::Recording,
                    ui::settings::Select::RecordingFolder,
                ),
            ),
            (
                "quit",
                false,
                vec![Message::RequestClose(window::Id::unique())],
            ),
            (
                // The Settings tab is shorter than the window; the prompt's
                // backdrop must still cover all of it.
                "quit-over-settings",
                false,
                vec![
                    Message::SelectTab(Tab::Settings),
                    Message::RequestClose(window::Id::unique()),
                ],
            ),
        ];

        let about = vec![
            Message::SelectTab(Tab::Settings),
            Message::Settings(ui::settings::Message::Section(ui::settings::Section::About)),
        ];
        states.push(("settings-about", false, about.clone()));
        states.push((
            "settings-about-update",
            false,
            about
                .into_iter()
                .chain([Message::UpdateChecked(Ok(update::Outcome::Available {
                    version: semver::Version::new(0, 2, 0),
                    url: "https://example.invalid/release".to_owned(),
                }))])
                .collect(),
        ));

        // Wide by default; these are also rendered at a narrow width.
        const NARROW: [&str; 1] = ["settings-display-narrow"];
        states.push((
            "settings-display-narrow",
            false,
            vec![
                Message::SelectTab(Tab::Settings),
                Message::Settings(ui::settings::Message::Section(
                    ui::settings::Section::Display,
                )),
            ],
        ));

        for (name, with_session, messages) in states {
            let size = if NARROW.contains(&name) {
                (640.0, 680.0)
            } else {
                (1024.0, 768.0)
            };
            let (mut app, _) = Application::new(None, Settings::default());
            let _ = app.open_connection(request("Connection #1"));
            let _ = app.open_connection(request("Connection #2"));
            app.tab = Tab::Connection(0);
            if with_session {
                feed_ilo_like_session(&mut app);
            }
            for message in messages {
                let _ = app.update(message);
            }
            let mut simulator = iced_test::Simulator::with_size(settings.clone(), size, app.view());
            let snapshot = simulator.snapshot(&theme).expect("renders");
            // `matches_image` only writes when no snapshot exists yet, and
            // names the file after the renderer too, so clear old ones first.
            let prefix = format!("{name}-");
            for entry in std::fs::read_dir(&dir).expect("SNAPSHOT_DIR is readable") {
                let path = entry.expect("directory entry").path();
                let stale = path
                    .file_name()
                    .is_some_and(|file| file.to_string_lossy().starts_with(&prefix));
                if stale {
                    std::fs::remove_file(path).expect("removes stale snapshot");
                }
            }
            assert!(
                snapshot
                    .matches_image(dir.join(format!("{name}.png")))
                    .expect("writes png")
            );
        }
    }

    #[test]
    fn saved_session_falls_back_to_the_protocol_label_when_name_and_hostname_are_blank() {
        let saved = saved_session_from_request(request(""));
        assert_eq!(saved.name, "dummy");
    }
}

/// Whether the app draws its own window controls and resize edges. macOS keeps
/// its native traffic lights over a transparent titlebar instead.
const CUSTOM_DECORATIONS: bool = !cfg!(target_os = "macos");
/// Room left for the macOS traffic lights at the start of the title bar.
const TRAFFIC_LIGHTS_WIDTH: f32 = 72.0;
/// Thickness of the invisible resize handles around an undecorated window.
const RESIZE_BORDER: f32 = 4.0;
const TAB_WIDTH: f32 = 150.0;
/// Title bar height: the tab buttons plus an equal inset above and below.
const TITLE_BAR_HEIGHT: f32 = ui::style::CONTROL_HEIGHT + 2.0 * ui::style::SPACING;

fn main() -> iced::Result {
    let config_dir = persistence::settings::config_dir();
    let settings = Settings::load(config_dir.as_deref());
    apply_graphics_backend(settings.general.graphics_backend);

    let boot = move || Application::new(config_dir.clone(), settings.clone());
    iced::application::application(boot, Application::update, Application::view)
        .title(Application::title)
        .window(window::Settings {
            icon: None, // TODO: add window icon
            exit_on_close_request: false,
            decorations: !CUSTOM_DECORATIONS,
            platform_specific: platform_specific(),
            ..Default::default()
        })
        .settings(ui::style::settings())
        .subscription(Application::subscription)
        .theme(ui::style::theme())
        .run()
}

/// Tells wgpu which API to draw with on Windows, as chosen in Settings
/// (DX12 by default: its swapchain keeps up with live resizing far better
/// than Vulkan's). iced only takes the backend from the environment, so set
/// `WGPU_BACKEND` — unless the user already did, which wins.
fn apply_graphics_backend(backend: GraphicsBackend) {
    if !cfg!(windows) || std::env::var_os("WGPU_BACKEND").is_some() {
        return;
    }
    // SAFETY: runs at the start of `main`, before any other thread exists.
    unsafe { std::env::set_var("WGPU_BACKEND", backend.wgpu_name()) };
}

fn pick_recording_folder() -> Task<Message> {
    Task::perform(
        async {
            rfd::AsyncFileDialog::new()
                .set_title("Save recordings to")
                .pick_folder()
                .await
                .map(|folder| folder.path().to_path_buf())
        },
        Message::RecordingFolderPicked,
    )
}

#[cfg(target_os = "macos")]
fn platform_specific() -> window::settings::PlatformSpecific {
    window::settings::PlatformSpecific {
        title_hidden: true,
        titlebar_transparent: true,
        fullsize_content_view: true,
    }
}

/// Keeps the DWM frame on the undecorated window so Windows 11 still draws
/// its rounded corners, accent border and drop shadow.
#[cfg(target_os = "windows")]
fn platform_specific() -> window::settings::PlatformSpecific {
    window::settings::PlatformSpecific {
        undecorated_shadow: true,
        corner_preference: window::settings::platform::CornerPreference::Round,
        ..Default::default()
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn platform_specific() -> window::settings::PlatformSpecific {
    window::settings::PlatformSpecific::default()
}

struct Application {
    connections: Vec<ui::connection::State>,
    next_id: u64,
    tab: Tab,

    connect_tab: ui::connect::State,

    sessions: persistence::sessions::Sessions,

    settings: Settings,
    settings_tab: ui::settings::State,
    /// Where settings are saved; `None` keeps them in memory only.
    config_dir: Option<PathBuf>,

    /// Tracked so the title bar shows restore rather than maximise.
    maximized: bool,
    update: update::Status,
    /// A dialog over the whole window, if any.
    modal: Option<AppModal>,
}

#[derive(Debug, Clone)]
enum AppModal {
    /// "Quit?", for the window that asked to close.
    ConfirmQuit(window::Id),
    Error {
        title: String,
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tab {
    Manager,
    Connect,
    Connection(u64),
    Settings,
}

#[derive(Clone)]
enum Message {
    SelectTab(Tab),

    ConnectTab(ui::connect::Message),
    Manager(ui::manager::Message),
    Connection(u64, ui::connection::Message),
    Settings(ui::settings::Message),
    RecordingFolderPicked(Option<PathBuf>),
    UpdateChecked(Result<update::Outcome, update::Error>),

    Window(WindowAction),
    Resized(iced::window::Id),
    Maximized(bool),
    RequestClose(iced::window::Id),
    Quit(iced::window::Id),
    CloseModal,
}

/// Window management triggered from the custom title bar.
#[derive(Debug, Clone, Copy)]
enum WindowAction {
    Drag,
    ToggleMaximize,
    Minimize,
    Close,
    Resize(window::Direction),
}

impl Application {
    fn new(config_dir: Option<PathBuf>, settings: Settings) -> (Self, Task<Message>) {
        let check_on_launch = settings.updates.check_on_launch;
        let mut app = Self {
            connections: Vec::new(),
            next_id: 0,
            tab: Tab::Connect,
            connect_tab: ui::connect::State::new(),
            sessions: persistence::sessions::Sessions::load(),
            settings,
            settings_tab: ui::settings::State::default(),
            config_dir,
            maximized: false,
            update: update::Status::NotChecked,
            modal: None,
        };
        let task = if check_on_launch {
            app.check_for_updates()
        } else {
            Task::none()
        };
        (app, task)
    }

    fn check_for_updates(&mut self) -> Task<Message> {
        self.update = update::Status::Checking;
        Task::perform(update::check(), Message::UpdateChecked)
    }

    fn open_url(&mut self, url: &str) {
        if let Err(error) = open::that_detached(url) {
            self.modal = Some(AppModal::Error {
                title: "Couldn't open the link".to_owned(),
                message: format!("{url}: {error}"),
            });
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SelectTab(tab) => {
                self.tab = tab;
                Task::none()
            }
            Message::ConnectTab(message) => match message {
                ui::connect::Message::Internal(message) => {
                    self.connect_tab.update(message).map(Message::ConnectTab)
                }
                ui::connect::Message::Connect(request) => self.open_connection(request),
                ui::connect::Message::Save(request) => {
                    self.save_session(request);
                    Task::none()
                }
            },
            Message::Manager(message) => match message {
                ui::manager::Message::Connect(request) => self.open_connection(request),
                ui::manager::Message::Delete(index) => {
                    if index < self.sessions.saved.len() {
                        self.sessions.saved.remove(index);
                        self.sessions.save();
                    }
                    Task::none()
                }
            },
            Message::Connection(id, message) => {
                use ui::connection::Message as Conn;
                // Messages about app-wide state are the application's to handle.
                match message {
                    Conn::Close => {
                        self.close_connection(id);
                        return Task::none();
                    }
                    Conn::OpenSettings(section) => {
                        self.close_menu(id);
                        self.settings_tab.open_section(section);
                        self.tab = Tab::Settings;
                        return Task::none();
                    }
                    Conn::ChangeSetting(change) => {
                        self.close_menu(id);
                        self.change_setting(change);
                        return Task::none();
                    }
                    Conn::ChooseRecordingFolder => {
                        self.close_menu(id);
                        return pick_recording_folder();
                    }
                    _ => {}
                }
                let Some(connection) = self.connections.iter_mut().find(|c| c.id() == id) else {
                    return Task::none();
                };
                connection
                    .update(message)
                    .map(move |message| Message::Connection(id, message))
            }
            Message::Window(action) => window::latest().and_then(move |id| match action {
                WindowAction::Drag => window::drag(id),
                WindowAction::ToggleMaximize => window::toggle_maximize(id),
                WindowAction::Minimize => window::minimize(id, true),
                WindowAction::Close => Task::done(Message::RequestClose(id)),
                WindowAction::Resize(direction) => window::drag_resize(id, direction),
            }),
            Message::Resized(id) => window::is_maximized(id).map(Message::Maximized),
            Message::Maximized(maximized) => {
                self.maximized = maximized;
                Task::none()
            }
            Message::Settings(message) => {
                self.settings_tab.update(&message);
                match message {
                    ui::settings::Message::Change(change) => {
                        self.change_setting(change);
                        Task::none()
                    }
                    ui::settings::Message::ChooseRecordingFolder => pick_recording_folder(),
                    ui::settings::Message::CheckForUpdates => self.check_for_updates(),
                    ui::settings::Message::OpenUrl(url) => {
                        self.open_url(&url);
                        Task::none()
                    }
                    ui::settings::Message::Section(_)
                    | ui::settings::Message::Toggle(_)
                    | ui::settings::Message::Dismiss => Task::none(),
                }
            }
            Message::UpdateChecked(result) => {
                self.update = update::Status::Checked(result);
                Task::none()
            }
            Message::RecordingFolderPicked(folder) => {
                if let Some(folder) = folder {
                    self.change_setting(ui::settings::Change::RecordingFolder(folder));
                }
                Task::none()
            }
            Message::RequestClose(id) => {
                if self.settings.general.confirm_quit && !self.connections.is_empty() {
                    self.modal = Some(AppModal::ConfirmQuit(id));
                    return Task::none();
                }
                window::close(id)
            }
            Message::Quit(id) => window::close(id),
            Message::CloseModal => {
                self.modal = None;
                Task::none()
            }
        }
    }

    fn change_setting(&mut self, change: ui::settings::Change) {
        ui::settings::apply(&mut self.settings, change);
        self.settings.save(self.config_dir.as_deref());
    }

    fn close_menu(&mut self, id: u64) {
        if let Some(connection) = self.connections.iter_mut().find(|c| c.id() == id) {
            let _ = connection.update(ui::connection::Message::CloseMenu);
        }
    }

    /// Persists the connect tab's current fields as a saved session and
    /// switches to the Manager tab so the result is immediately visible.
    fn save_session(&mut self, request: ui::connect::Request) {
        self.sessions
            .saved
            .push(saved_session_from_request(request));
        self.sessions.save();
        self.tab = Tab::Manager;
    }

    fn open_connection(&mut self, request: ui::connect::Request) -> Task<Message> {
        let id = self.next_id;
        self.next_id += 1;

        let name: Arc<str> = if request.hostname.is_empty() {
            format!("{} #{id}", request.protocol).into()
        } else {
            Arc::from(request.hostname.as_str())
        };
        let target = parse_target(&request.hostname, request.protocol);

        self.connections.push(ui::connection::State::new(
            id,
            name,
            request.protocol,
            target,
            TlsPolicy::default(),
            request.password,
            &self.settings.input,
        ));
        self.tab = Tab::Connection(id);
        Task::none()
    }

    fn close_connection(&mut self, id: u64) {
        self.connections.retain(|connection| connection.id() != id);
        if self.tab == Tab::Connection(id) {
            self.tab = Tab::Connect;
        }
    }

    fn view(&self) -> Element<'_, Message> {
        // The header carries its own insets so the window controls can sit
        // flush in the corner; only the body gets the outer margin.
        // Fills the window whatever the tab holds: layers stacked over it
        // (the resize handles, the quit prompt) take their size from it.
        let content = Column::new()
            .width(Length::Fill)
            .height(Length::Fill)
            .push(self.header())
            .push(
                container(self.body())
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .padding(
                        padding::left(ui::style::SPACING)
                            .right(ui::style::SPACING)
                            .bottom(ui::style::SPACING),
                    ),
            );

        // A maximised window can't be resized, and the handles would steal
        // the screen corner from the close button.
        let window: Element<'_, Message> = if CUSTOM_DECORATIONS && !self.maximized {
            stack![content, resize_handles()].into()
        } else {
            content.into()
        };
        let window = match &self.modal {
            Some(modal) => stack![window, self.modal_view(modal)].into(),
            None => window,
        };
        ui::arrow_cursor(window)
    }

    fn modal_view<'a>(&'a self, modal: &'a AppModal) -> Element<'a, Message> {
        let id = match modal {
            AppModal::ConfirmQuit(id) => *id,
            AppModal::Error { title, message } => {
                return ui::dialog::view(
                    title,
                    message,
                    vec![ui::dialog::Button {
                        label: "OK",
                        on_press: Message::CloseModal,
                        kind: ui::dialog::Kind::Primary,
                    }],
                );
            }
        };
        let message = match self.connections.len() {
            1 => "The open connection will be closed.",
            _ => "All open connections will be closed.",
        };
        ui::dialog::view(
            "Quit?",
            message,
            vec![
                ui::dialog::Button {
                    label: "Cancel",
                    on_press: Message::CloseModal,
                    kind: ui::dialog::Kind::Secondary,
                },
                ui::dialog::Button {
                    label: "Quit",
                    on_press: Message::Quit(id),
                    kind: ui::dialog::Kind::Destructive,
                },
            ],
        )
    }

    fn body(&self) -> Element<'_, Message> {
        match &self.tab {
            Tab::Manager => ui::manager::view(&self.sessions.saved).map(Message::Manager),
            Tab::Connect => self.connect_tab.view().map(Message::ConnectTab),
            Tab::Connection(id) => match self.connections.iter().find(|c| c.id() == *id) {
                Some(connection) => {
                    let id = *id;
                    connection
                        .view(&self.settings)
                        .map(move |message| Message::Connection(id, message))
                }
                None => text("Connection closed").into(),
            },
            Tab::Settings => ui::settings::view(&self.settings_tab, &self.settings, &self.update)
                .map(Message::Settings),
        }
    }

    fn title(&self) -> String {
        // TODO: show connection
        "Server Remote".into()
    }

    fn subscription(&self) -> Subscription<Message> {
        let close_requests = window::close_requests().map(Message::RequestClose);
        // `Subscription::map` requires a non-capturing closure, so the id rides
        // along via `with` and is unpacked from the tuple instead of captured.
        let connections = self.connections.iter().map(|connection| {
            connection
                .subscription()
                .with(connection.id())
                .map(|(id, message)| Message::Connection(id, message))
        });

        // Keyboard input is captured directly by the focused `kvm_display`
        // widget on the active connection tab, not routed here.
        let resizes = window::resize_events().map(|(id, _size)| Message::Resized(id));
        Subscription::batch([close_requests, resizes].into_iter().chain(connections))
    }

    fn header(&self) -> Element<'_, Message> {
        let tabs = Row::with_children(
            self.connections
                .iter()
                .map(|connection| self.connection_tab(connection)),
        )
        .push(self.icon_tab(Tab::Connect, icons::ADD_LARGE, "Connect to…"))
        .spacing(ui::style::SPACING);

        // Empty space in the title bar moves the window, as a native one would.
        let drag_region = mouse_area(
            Row::new()
                .push((!CUSTOM_DECORATIONS).then(|| Space::new().width(TRAFFIC_LIGHTS_WIDTH)))
                .push(self.icon_tab(Tab::Manager, icons::VIRTUAL_DESKTOP, "Connection manager"))
                .push(Scrollable::new(tabs).horizontal().width(Length::Fill))
                .spacing(ui::style::SPACING),
        )
        .on_press(Message::Window(WindowAction::Drag))
        .on_double_click(Message::Window(WindowAction::ToggleMaximize));

        Row::new()
            .push(drag_region)
            .push(self.icon_tab(Tab::Settings, icons::SETTINGS, "Settings"))
            .push(CUSTOM_DECORATIONS.then(|| {
                ui::window_controls::view(
                    self.maximized,
                    Message::Window(WindowAction::Minimize),
                    Message::Window(WindowAction::ToggleMaximize),
                    Message::Window(WindowAction::Close),
                )
            }))
            .spacing(ui::style::SPACING)
            .align_y(Alignment::Center)
            // Tall enough for an even inset above, below and left of the
            // tabs; the window controls stretch to fill it, flush with the
            // top and right edges so the screen corner hits close.
            .height(TITLE_BAR_HEIGHT)
            .padding(
                padding::left(ui::style::SPACING).right(if CUSTOM_DECORATIONS {
                    0.0
                } else {
                    ui::style::SPACING
                }),
            )
            .into()
    }

    fn icon_tab(&self, tab: Tab, icon: icons::Icon, label: &str) -> Element<'_, Message> {
        let active = self.tab == tab;
        tooltip(
            button(container(themed(icon)).center(Length::Fill))
                .width(ui::style::CONTROL_HEIGHT)
                .height(ui::style::CONTROL_HEIGHT)
                .padding(0)
                .style(move |theme, status| ui::style::bordered_button(theme, status, active))
                .on_press(Message::SelectTab(tab)),
            tooltip_box(text(label.to_owned())),
            tooltip::Position::Bottom,
        )
        .into()
    }

    fn connection_tab<'a>(&self, connection: &'a ui::connection::State) -> Element<'a, Message> {
        let id = connection.id();
        let active = self.tab == Tab::Connection(id);

        let close = button(container(themed(icons::CLOSE)).center(Length::Fill))
            .width(18)
            .height(18)
            .padding(0)
            .style(move |theme, status| ui::style::tab_close_button(theme, status, active))
            .on_press(Message::Connection(id, ui::connection::Message::Close));

        let font = if active {
            ui::style::BOLD
        } else {
            ui::style::FONT
        };

        button(
            Row::new()
                .push(themed(connection.protocol().icon()))
                .push(
                    text(connection.name().as_ref())
                        .font(font)
                        .width(Length::Fill)
                        .wrapping(text::Wrapping::None),
                )
                .push(close)
                .spacing(4)
                .height(Length::Fill)
                .align_y(Alignment::Center),
        )
        .width(TAB_WIDTH)
        .height(ui::style::CONTROL_HEIGHT)
        .padding(padding::left(5).right(5))
        .style(move |theme, status| ui::style::bordered_button(theme, status, active))
        .on_press(Message::SelectTab(Tab::Connection(id)))
        .into()
    }
}

fn themed<'a>(icon: icons::Icon) -> iced::widget::svg::Svg<'a> {
    ui::themed_icon(icon.element(), |palette| palette.background.base.text)
}

/// Invisible handles along the window edges that resize an undecorated
/// window, since the OS no longer provides them. Layered over the content so
/// they sit in its outer margin rather than adding to it.
fn resize_handles<'a>() -> Element<'a, Message> {
    use iced::mouse::Interaction;
    use window::Direction;

    let handle = |direction: Direction, width: Length, height: Length| {
        let interaction = match direction {
            Direction::North | Direction::South => Interaction::ResizingVertically,
            Direction::East | Direction::West => Interaction::ResizingHorizontally,
            Direction::NorthWest | Direction::SouthEast => Interaction::ResizingDiagonallyDown,
            Direction::NorthEast | Direction::SouthWest => Interaction::ResizingDiagonallyUp,
        };
        mouse_area(Space::new().width(width).height(height))
            .interaction(interaction)
            .on_press(Message::Window(WindowAction::Resize(direction)))
    };
    let border = Length::Fixed(RESIZE_BORDER);

    Column::new()
        .push(
            Row::new()
                .push(handle(Direction::NorthWest, border, border))
                .push(handle(Direction::North, Length::Fill, border))
                .push(handle(Direction::NorthEast, border, border)),
        )
        .push(
            Row::new()
                .push(handle(Direction::West, border, Length::Fill))
                .push(Space::new().width(Length::Fill))
                .push(handle(Direction::East, border, Length::Fill))
                .height(Length::Fill),
        )
        .push(
            Row::new()
                .push(handle(Direction::SouthWest, border, border))
                .push(handle(Direction::South, Length::Fill, border))
                .push(handle(Direction::SouthEast, border, border)),
        )
        .into()
}
