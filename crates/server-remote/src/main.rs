use std::sync::Arc;

use iced::{
    Element, Length, Settings, Subscription, Task,
    widget::{Column, Row, Scrollable, button, text, tooltip},
    window,
};
use server_remote_core::{Target, TlsPolicy};

use crate::tooltip_box::tooltip_box;

mod tooltip_box;
mod ui;

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
}

#[derive(Clone, PartialEq, Eq)]
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

    fn open_connection(&mut self, request: ui::connect::Request) -> Task<Message> {
        let id = self.next_id;
        self.next_id += 1;

        let name: Arc<str> = if request.hostname.is_empty() {
            format!("{} #{id}", request.protocol).into()
        } else {
            Arc::from(request.hostname.as_str())
        };
        let host = if request.hostname.is_empty() {
            "localhost"
        } else {
            &request.hostname
        };
        let target = Target::new(host, 0);

        self.connections.push(ui::connection::State::new(
            id,
            name,
            request.protocol,
            target,
            TlsPolicy::default(),
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
                Tab::Manager => Into::<Element<'state, Message>>::into(text("Manager")),
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

        Subscription::batch(std::iter::once(close_requests).chain(connections))
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

        let connection_tabs = Row::from_iter(self.connections.iter().map(|connection| {
            let name = connection.name().as_ref();
            tab_button(Tab::Connection(connection.id()), name, name).into()
        }));

        Row::new()
            .push(tab_button(Tab::Manager, "M", "Connection manager"))
            .push(tab_button(Tab::Connect, "+", "Connect to..."))
            .push(
                Scrollable::new(connection_tabs)
                    .horizontal()
                    .width(Length::Fill),
            )
            .push(tab_button(Tab::Settings, "S", "Settings"))
            .spacing(4)
            .into()
    }
}
