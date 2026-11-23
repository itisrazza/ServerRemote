//! Plain copies of the app's state for the front end. UniFFI passes these
//! by value, so they hold only what the front end shows.

use server_remote_app::app;
use server_remote_app::connection::{self, Connection, Phase, format_rate};
use server_remote_app::persistence::sessions::SavedSession;
use server_remote_app::{App, i18n};
use server_remote_core::{IndicatorState, MenuItem, StatusItem, ToolbarItem};

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Page {
    Manager,
    Connect,
    Connection { id: u64 },
    Settings,
}

impl From<app::Page> for Page {
    fn from(page: app::Page) -> Self {
        match page {
            app::Page::Manager => Page::Manager,
            app::Page::Connect => Page::Connect,
            app::Page::Connection(id) => Page::Connection { id },
            app::Page::Settings => Page::Settings,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ConnectionProtocol {
    Dummy,
    Vnc,
    Spice,
    Ilo,
}

impl From<server_remote_app::Protocol> for ConnectionProtocol {
    fn from(protocol: server_remote_app::Protocol) -> Self {
        match protocol {
            server_remote_app::Protocol::Dummy => ConnectionProtocol::Dummy,
            server_remote_app::Protocol::Vnc => ConnectionProtocol::Vnc,
            server_remote_app::Protocol::Spice => ConnectionProtocol::Spice,
            server_remote_app::Protocol::Ilo => ConnectionProtocol::Ilo,
        }
    }
}

impl From<ConnectionProtocol> for server_remote_app::Protocol {
    fn from(protocol: ConnectionProtocol) -> Self {
        match protocol {
            ConnectionProtocol::Dummy => server_remote_app::Protocol::Dummy,
            ConnectionProtocol::Vnc => server_remote_app::Protocol::Vnc,
            ConnectionProtocol::Spice => server_remote_app::Protocol::Spice,
            ConnectionProtocol::Ilo => server_remote_app::Protocol::Ilo,
        }
    }
}

/// The protocol's name as shown to people, e.g. "HPE iLO".
#[uniffi::export]
pub fn protocol_name(protocol: ConnectionProtocol) -> String {
    server_remote_app::Protocol::from(protocol)
        .full_name()
        .to_owned()
}

/// Every protocol, in the order the connect form offers them.
#[uniffi::export]
pub fn protocols() -> Vec<ConnectionProtocol> {
    server_remote_app::Protocol::ALL
        .into_iter()
        .map(ConnectionProtocol::from)
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ConnectRequest {
    pub protocol: ConnectionProtocol,
    pub name: String,
    pub group: String,
    pub hostname: String,
    /// Only iLO uses one.
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct Screen {
    /// Counts up with each frame, to tell whether a copy is stale.
    pub generation: u64,
    pub width: u32,
    pub height: u32,
    /// `width × height` pixels, four bytes each.
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AppState {
    pub page: Page,
    pub connections: Vec<ConnectionState>,
    pub sessions: Vec<Session>,
    pub dialog: Option<AppDialog>,
}

impl From<&App> for AppState {
    fn from(app: &App) -> Self {
        AppState {
            page: app.page().into(),
            connections: app
                .connections()
                .iter()
                .map(ConnectionState::from)
                .collect(),
            sessions: app.sessions().saved.iter().map(Session::from).collect(),
            dialog: app.dialog().map(AppDialog::from),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Session {
    pub id: u64,
    pub name: String,
    pub group: String,
    pub protocol: ConnectionProtocol,
    pub hostname: String,
    pub username: String,
    pub password: String,
}

impl From<&SavedSession> for Session {
    fn from(session: &SavedSession) -> Self {
        Session {
            id: session.id,
            name: session.name.clone(),
            group: session.group.clone(),
            protocol: session.protocol.into(),
            hostname: session.hostname.clone(),
            username: session.username.clone(),
            password: session.password.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum AppDialog {
    Error {
        title: String,
        message: String,
    },
    /// A connection failed before it ever connected; `details` is for the
    /// clipboard.
    ConnectionFailed {
        message: String,
        details: String,
    },
}

impl From<&app::Dialog> for AppDialog {
    fn from(dialog: &app::Dialog) -> Self {
        match dialog.clone() {
            app::Dialog::Error { title, message } => AppDialog::Error { title, message },
            app::Dialog::ConnectionFailed { message, details } => {
                AppDialog::ConnectionFailed { message, details }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum ConnectionPhase {
    Connecting,
    Connected,
    Disconnected { reason: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum ConnectionDialog {
    Confirm {
        title: String,
        message: String,
        confirm_label: String,
        destructive: bool,
    },
    TrustCertificate {
        host: String,
        fingerprint: String,
    },
    Error {
        title: String,
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ConnectionState {
    pub id: u64,
    pub name: String,
    pub protocol: ConnectionProtocol,
    /// `host:port`.
    pub address: String,
    pub phase: ConnectionPhase,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Frames received so far: when it changes, fetch the screen again.
    pub frames: u64,
    /// Absolute pointers follow the touch; relative ones aren't supported
    /// yet.
    pub absolute_pointer: bool,
    pub dialog: Option<ConnectionDialog>,
    /// The connector's menus, with the input actions first.
    pub menus: Vec<ActionMenu>,
    pub links: Vec<WebLink>,
    pub chips: Vec<Chip>,
    pub status: Vec<StatusEntry>,
    pub encryption: Option<String>,
    /// Received and sent, e.g. "150 kb/s".
    pub traffic: Option<Traffic>,
}

impl From<&Connection> for ConnectionState {
    fn from(connection: &Connection) -> Self {
        let capabilities = connection.capabilities();
        let status = connection.status();
        let target = connection.target();
        let input = (!capabilities.input_actions.is_empty()).then(|| ActionMenu {
            label: i18n::LOADER.get("section-input"),
            actions: capabilities
                .input_actions
                .iter()
                .map(MenuAction::from)
                .collect(),
        });
        let menus = capabilities.toolbar.iter().filter_map(|item| match item {
            ToolbarItem::Menu { label, items, .. } => Some(ActionMenu {
                label: label.to_string(),
                actions: flatten(items, ""),
            }),
            ToolbarItem::Link { .. } => None,
        });
        ConnectionState {
            id: connection.id(),
            name: connection.name().to_string(),
            protocol: connection.protocol().into(),
            address: format!("{}:{}", target.host, target.port),
            phase: match connection.phase() {
                Phase::Connecting => ConnectionPhase::Connecting,
                Phase::Connected => ConnectionPhase::Connected,
                Phase::Disconnected(reason) => ConnectionPhase::Disconnected {
                    reason: reason.clone(),
                },
            },
            width: connection.size().map(|(width, _)| width),
            height: connection.size().map(|(_, height)| height),
            frames: connection.frames(),
            absolute_pointer: capabilities.pointer == server_remote_core::PointerMode::Absolute,
            dialog: connection
                .dialog()
                .map(|dialog| ConnectionDialog::new(dialog, &target.host)),
            menus: input.into_iter().chain(menus).collect(),
            links: capabilities
                .toolbar
                .iter()
                .filter_map(|item| match item {
                    ToolbarItem::Link { label, url, .. } => Some(WebLink {
                        label: label.to_string(),
                        url: url.to_string(),
                    }),
                    ToolbarItem::Menu { .. } => None,
                })
                .collect(),
            chips: status
                .chips
                .iter()
                .map(|chip| Chip {
                    label: chip.label.to_string(),
                    close_action: chip.close_action.as_ref().map(|id| id.to_string()),
                })
                .collect(),
            status: status.items.iter().map(StatusEntry::from).collect(),
            encryption: status.encryption.as_ref().map(|e| e.to_string()),
            traffic: connection.traffic().map(|rate| Traffic {
                received: format_rate(rate.received),
                sent: format_rate(rate.sent),
            }),
        }
    }
}

impl ConnectionDialog {
    fn new(dialog: &connection::Dialog, host: &str) -> Self {
        match dialog {
            connection::Dialog::Confirm(action) => {
                let confirmation = action
                    .confirm
                    .as_ref()
                    .expect("only actions with a confirmation are confirmed");
                ConnectionDialog::Confirm {
                    title: confirmation.title.to_string(),
                    message: confirmation.message.to_string(),
                    confirm_label: confirmation.confirm_label.to_string(),
                    destructive: confirmation.destructive,
                }
            }
            connection::Dialog::TrustCertificate(fingerprint) => {
                ConnectionDialog::TrustCertificate {
                    host: host.to_owned(),
                    fingerprint: fingerprint.to_string(),
                }
            }
            connection::Dialog::Error { title, message } => ConnectionDialog::Error {
                title: title.clone(),
                message: message.clone(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ActionMenu {
    pub label: String,
    pub actions: Vec<MenuAction>,
}

/// An action in a menu. Submenus are flattened, their label prefixed.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MenuAction {
    pub id: String,
    pub label: String,
    pub enabled: bool,
    pub checked: bool,
}

impl From<&server_remote_core::Action> for MenuAction {
    fn from(action: &server_remote_core::Action) -> Self {
        MenuAction {
            id: action.id.to_string(),
            label: action.label.to_string(),
            enabled: action.enabled,
            checked: action.checked,
        }
    }
}

fn flatten(items: &[MenuItem], prefix: &str) -> Vec<MenuAction> {
    items
        .iter()
        .flat_map(|item| match item {
            MenuItem::Action(action) => vec![MenuAction {
                label: format!("{prefix}{}", action.label),
                ..MenuAction::from(action)
            }],
            MenuItem::Submenu { label, items } => flatten(items, &format!("{prefix}{label} › ")),
            MenuItem::Separator => Vec::new(),
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct WebLink {
    pub label: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Chip {
    pub label: String,
    /// Sent with [`crate::Core::send_action`] when the chip is closed.
    pub close_action: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Indicator {
    Ok,
    Warning,
    Error,
    Inactive,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct StatusEntry {
    pub text: String,
    /// A coloured dot before the text, if any.
    pub indicator: Option<Indicator>,
}

impl From<&StatusItem> for StatusEntry {
    fn from(item: &StatusItem) -> Self {
        match item {
            StatusItem::Indicator { label, state } => StatusEntry {
                text: label.to_string(),
                indicator: Some(match state {
                    IndicatorState::Ok => Indicator::Ok,
                    IndicatorState::Warning => Indicator::Warning,
                    IndicatorState::Error => Indicator::Error,
                    IndicatorState::Inactive => Indicator::Inactive,
                }),
            },
            StatusItem::Text { text, .. } => StatusEntry {
                text: text.to_string(),
                indicator: None,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Traffic {
    pub received: String,
    pub sent: String,
}
