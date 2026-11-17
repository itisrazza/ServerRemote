use iced::{
    Element, Length, Task,
    widget::{Column, Row, button, checkbox, container, pick_list, text, text_input},
};

use crate::i18n::fl;
use crate::ui::connection::Protocol;

pub struct State {
    protocol: Protocol,
    name: String,
    hostname: String,
    password: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    Internal(IMessage),
    /// Bubbles up to the application to open a connection tab.
    Connect(Request),
    /// Bubbles up to the application to persist a saved session.
    Save(Request),
}

/// Everything needed to either open a connection tab or save a session.
/// `name` is only meaningful for [`Message::Save`] — the connect flow derives
/// a tab name from the protocol and hostname instead.
#[derive(Debug, Clone)]
pub struct Request {
    pub protocol: Protocol,
    pub name: String,
    pub hostname: String,
    pub password: String,
}

#[derive(Debug, Clone)]
pub enum IMessage {
    Protocol(Protocol),
    Name(String),
    Hostname(String),
    Password(String),
}

impl State {
    pub fn new() -> State {
        State {
            protocol: Protocol::Dummy,
            name: String::new(),
            hostname: String::new(),
            password: String::new(),
        }
    }

    pub fn update(&mut self, message: IMessage) -> Task<Message> {
        match message {
            IMessage::Protocol(protocol) => {
                self.protocol = protocol;
                Task::none()
            }
            IMessage::Name(name) => {
                self.name = name;
                Task::none()
            }
            IMessage::Hostname(hostname) => {
                self.hostname = hostname;
                Task::none()
            }
            IMessage::Password(password) => {
                self.password = password;
                Task::none()
            }
        }
    }

    fn request(&self) -> Request {
        Request {
            protocol: self.protocol,
            name: self.name.clone(),
            hostname: self.hostname.clone(),
            password: self.password.clone(),
        }
    }

    pub fn view<'state>(&'state self) -> Element<'state, Message> {
        container(
            Column::new()
                .push(pick_list(Protocol::ALL, Some(self.protocol), |value| {
                    Message::Internal(IMessage::Protocol(value))
                }))
                .push(
                    // TOOD: generate the prompt from the plugin's connection builder
                    Column::new()
                        .spacing(4)
                        .push(
                            text_input(&fl!("connect-name"), &self.name)
                                .on_input(|value| Message::Internal(IMessage::Name(value))),
                        )
                        .push(
                            text_input(&fl!("connect-hostname"), &self.hostname)
                                .on_input(|value| Message::Internal(IMessage::Hostname(value))),
                        )
                        .push(
                            text_input(&fl!("connect-password"), &self.password)
                                .secure(true)
                                .on_input(|value| Message::Internal(IMessage::Password(value))),
                        )
                        .push(
                            Row::new()
                                .push(checkbox(false))
                                .push(text(fl!("connect-read-only"))),
                        ),
                )
                .push(
                    Row::new()
                        .push(
                            button(text(fl!("connect-save")))
                                .style(button::subtle)
                                .width(Length::Fill)
                                .on_press(Message::Save(self.request())),
                        )
                        .push(
                            button(text(fl!("connect-connect")))
                                .style(button::primary)
                                .width(Length::Fill)
                                .on_press(Message::Connect(self.request())),
                        )
                        .spacing(4),
                )
                .spacing(4)
                .max_width(320),
        )
        .center(Length::Fill)
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_state_defaults_to_dummy_with_empty_fields() {
        let state = State::new();
        assert_eq!(state.protocol, Protocol::Dummy);
        assert_eq!(state.hostname, "");
        assert_eq!(state.password, "");
    }

    #[test]
    fn set_protocol_updates_the_selected_protocol() {
        let mut state = State::new();
        let _ = state.update(IMessage::Protocol(Protocol::Vnc));
        assert_eq!(state.protocol, Protocol::Vnc);
    }

    #[test]
    fn set_hostname_updates_the_hostname() {
        let mut state = State::new();
        let _ = state.update(IMessage::Hostname("example.com".to_owned()));
        assert_eq!(state.hostname, "example.com");
    }

    #[test]
    fn set_password_updates_the_password() {
        let mut state = State::new();
        let _ = state.update(IMessage::Password("hunter2".to_owned()));
        assert_eq!(state.password, "hunter2");
    }

    #[test]
    fn set_name_updates_the_name() {
        let mut state = State::new();
        let _ = state.update(IMessage::Name("Home NAS".to_owned()));
        assert_eq!(state.name, "Home NAS");
    }

    #[test]
    fn request_reflects_the_current_form_fields() {
        let mut state = State::new();
        let _ = state.update(IMessage::Protocol(Protocol::Vnc));
        let _ = state.update(IMessage::Name("Home NAS".to_owned()));
        let _ = state.update(IMessage::Hostname("example.com".to_owned()));
        let _ = state.update(IMessage::Password("hunter2".to_owned()));

        let request = state.request();

        assert_eq!(request.protocol, Protocol::Vnc);
        assert_eq!(request.name, "Home NAS");
        assert_eq!(request.hostname, "example.com");
        assert_eq!(request.password, "hunter2");
    }
}
