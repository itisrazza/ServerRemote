use std::sync::Arc;

use iced::{
    Element, Length, Settings, Subscription, Task,
    widget::{Column, Row, Scrollable, button, text, tooltip},
    window,
};
use server_remote_core::{Target, TlsPolicy};

use crate::tooltip_box::tooltip_box;

mod persistence;
mod tooltip_box;
mod ui;

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
        let (mut app, _) = Application::boot();

        let _ = app.open_connection(request("first"));
        let _ = app.open_connection(request("second"));

        assert_eq!(app.connections.len(), 2);
        assert_eq!(app.connections[0].id(), 0);
        assert_eq!(app.connections[1].id(), 1);
        assert_eq!(app.tab, Tab::Connection(1));
    }

    #[test]
    fn open_connection_names_an_empty_hostname_after_the_protocol_and_id() {
        let (mut app, _) = Application::boot();

        let _ = app.open_connection(request(""));

        assert_eq!(app.connections[0].name().as_ref(), "dummy #0");
    }

    #[test]
    fn close_connection_removes_it_and_returns_to_the_connect_tab() {
        let (mut app, _) = Application::boot();
        let _ = app.open_connection(request("only"));
        assert_eq!(app.tab, Tab::Connection(0));

        app.close_connection(0);

        assert!(app.connections.is_empty());
        assert_eq!(app.tab, Tab::Connect);
    }

    #[test]
    fn close_connection_leaves_an_unrelated_active_tab_alone() {
        let (mut app, _) = Application::boot();
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

    #[test]
    fn saved_session_falls_back_to_the_protocol_label_when_name_and_hostname_are_blank() {
        let saved = saved_session_from_request(request(""));
        assert_eq!(saved.name, "dummy");
    }
}

fn main() -> iced::Result {
    iced::application::application(Application::boot, Application::update, Application::view)
        .title(Application::title)
        .window(iced::window::Settings {
            icon: None, // TODO: add window icon
            exit_on_close_request: false,
            ..Default::default()
        })
        .settings(Settings {
            default_text_size: iced::Pixels(13.0),
            ..Default::default()
        })
        .subscription(Application::subscription)
        .run()
}

struct Application {
    connections: Vec<ui::connection::State>,
    next_id: u64,
    tab: Tab,

    connect_tab: ui::connect::State,

    sessions: persistence::sessions::Sessions,
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

    RequestClose(iced::window::Id),
}

impl Application {
    fn boot() -> (Self, Task<Message>) {
        (
            Self {
                connections: Vec::new(),
                next_id: 0,
                tab: Tab::Connect,
                connect_tab: ui::connect::State::new(),
                sessions: persistence::sessions::Sessions::load(),
            },
            Task::none(),
        )
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
                if matches!(message, ui::connection::Message::Close) {
                    self.close_connection(id);
                    return Task::none();
                }
                let Some(connection) = self.connections.iter_mut().find(|c| c.id() == id) else {
                    return Task::none();
                };
                connection
                    .update(message)
                    .map(move |message| Message::Connection(id, message))
            }
            Message::RequestClose(id) => {
                // TODO: check if there are any active connections
                window::close(id)
            }
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

    fn view<'state>(&'state self) -> Element<'state, Message> {
        Column::new()
            .push(self.header())
            .push(match &self.tab {
                Tab::Manager => ui::manager::view(&self.sessions.saved).map(Message::Manager),
                Tab::Connect => self.connect_tab.view().map(Message::ConnectTab),
                Tab::Connection(id) => match self.connections.iter().find(|c| c.id() == *id) {
                    Some(connection) => {
                        let id = *id;
                        connection
                            .view()
                            .map(move |message| Message::Connection(id, message))
                    }
                    None => text("Connection closed").into(),
                },
                Tab::Settings => text("Settings").into(),
            })
            .padding(4)
            .spacing(4)
            .into()
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
        Subscription::batch([close_requests].into_iter().chain(connections))
    }

    fn header<'state>(&'state self) -> Element<'state, Message> {
        let tab_button = |tab: Tab, short: &str, long: &str| {
            tooltip(
                button(text(short.to_owned()))
                    .style(if self.tab == tab {
                        button::primary
                    } else {
                        button::subtle
                    })
                    .on_press(Message::SelectTab(tab)),
                tooltip_box(text(long.to_owned())),
                tooltip::Position::Bottom,
            )
        };

        let icon_tab_button =
            |tab: Tab, icon: iced::widget::svg::Svg<'static, iced::Theme>, long: &str| {
                let active = self.tab == tab;
                tooltip(
                    button(ui::themed_icon(icon, move |palette| {
                        if active {
                            palette.primary.base.text
                        } else {
                            palette.background.weakest.text
                        }
                    }))
                    .style(if active {
                        button::primary
                    } else {
                        button::subtle
                    })
                    .on_press(Message::SelectTab(tab)),
                    tooltip_box(text(long.to_owned())),
                    tooltip::Position::Bottom,
                )
            };

        let connection_tabs = Row::from_iter(self.connections.iter().map(|connection| {
            let name = connection.name().as_ref();
            tab_button(Tab::Connection(connection.id()), name, name).into()
        }))
        .spacing(4);

        Row::new()
            .push(icon_tab_button(
                Tab::Manager,
                bootstrap_icons::iced::bi!(hdd - network),
                "Connection manager",
            ))
            .push(icon_tab_button(
                Tab::Connect,
                bootstrap_icons::iced::bi!(plus - lg),
                "Connect to...",
            ))
            .push(
                Scrollable::new(connection_tabs)
                    .horizontal()
                    .width(Length::Fill),
            )
            .push(icon_tab_button(
                Tab::Settings,
                bootstrap_icons::iced::bi!(gear - wide - connected),
                "Settings",
            ))
            .spacing(4)
            .into()
    }
}
