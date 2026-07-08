use std::{ops::Deref, sync::Arc};

use iced::{
    Element, Length, Settings, Subscription, Task,
    widget::{Column, Row, Scrollable, button, column, text, tooltip},
    window,
};

use crate::{tooltip_box::tooltip_box, ui::connect::Message::Internal};

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
    tabs: Vec<Arc<str>>,
    tab: Tab,

    connect_tab: ui::connect::State,
}

#[derive(Clone, PartialEq, Eq)]
enum Tab {
    Manager,
    Connect,
    Connection(Arc<str>),
    Settings,
}

#[derive(Clone)]
enum Message {
    AddTab,
    SelectTab(Tab),

    ConnectTab(ui::connect::Message),

    RequestClose(iced::window::Id),
}

impl Application {
    fn boot() -> (Self, Task<Message>) {
        (
            Self {
                tabs: Vec::new(),
                tab: Tab::Connect,
                connect_tab: ui::connect::State::new(),
            },
            Task::none(),
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::AddTab => {
                self.tabs
                    .push(Arc::from(format!("Tab {}", self.tabs.len())));
                Task::none()
            }
            Message::SelectTab(tab) => {
                self.tab = tab;
                Task::none()
            }
            Message::ConnectTab(message) => match message {
                ui::connect::Message::Internal(message) => self
                    .connect_tab
                    .update(message)
                    .map(|message| Message::ConnectTab(message)),
            },
            Message::RequestClose(id) => {
                // TODO: check if there are any active connections
                window::close(id)
            }
        }
    }

    fn view<'state>(&'state self) -> Element<'state, Message> {
        Column::new()
            .push(self.header())
            .push(match &self.tab {
                Tab::Manager => Into::<Element<'state, Message>>::into(text("Manager")),
                Tab::Connect => self.connect_tab.view().map(Message::ConnectTab),
                Tab::Connection(name) => text(format!("Connection ({name})")).into(),
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
        let close_requests = window::close_requests().map(|id| Message::RequestClose(id));

        close_requests
    }

    fn header<'state>(&'state self) -> Element<'state, Message> {
        let tab_button = |tab: &Tab| {
            tooltip(
                button(text(
                    match tab {
                        Tab::Manager => "M",
                        Tab::Connect => "+",
                        Tab::Settings => "S",
                        Tab::Connection(name) => name,
                    }
                    .to_string(),
                ))
                .style(if self.tab == *tab {
                    button::primary
                } else {
                    button::subtle
                })
                .on_press(Message::SelectTab(tab.clone())),
                tooltip_box(text(
                    match &tab {
                        Tab::Manager => "Connection manager",
                        Tab::Connect => "Connect to...",
                        Tab::Connection(name) => name,
                        Tab::Settings => "Settings",
                    }
                    .to_string(),
                )),
                tooltip::Position::Bottom,
            )
        };

        Row::new()
            .push(tab_button(&Tab::Manager))
            .push(tab_button(&Tab::Connect))
            .push(
                Scrollable::new(Row::from_iter(
                    self.tabs
                        .iter()
                        .map(|tab| tab_button(&Tab::Connection(tab.clone())).into()),
                ))
                .horizontal()
                .width(Length::Fill),
            )
            .push(tab_button(&Tab::Settings))
            .spacing(4)
            .into()
    }
}
