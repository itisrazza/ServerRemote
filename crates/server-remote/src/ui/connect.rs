use iced::{
    Element, Length, Task,
    widget::{Column, Row, button, checkbox, container, pick_list, text_input},
};

use crate::ui::connection::Protocol;

pub struct State {
    protocol: Protocol,
    hostname: String,
    password: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    Internal(IMessage),
    /// Bubbles up to the application to open a connection tab.
    Connect(Request),
}

#[derive(Debug, Clone)]
pub struct Request {
    pub protocol: Protocol,
    pub hostname: String,
    pub password: String,
}

#[derive(Debug, Clone)]
pub enum IMessage {
    SetProtocol(Protocol),
    SetHostname(String),
    SetPassword(String),
    Save,
}

impl State {
    pub fn new() -> State {
        State {
            protocol: Protocol::Dummy,
            hostname: String::new(),
            password: String::new(),
        }
    }

    pub fn update(&mut self, message: IMessage) -> Task<Message> {
        match message {
            IMessage::SetProtocol(protocol) => {
                self.protocol = protocol;
                Task::none()
            }
            IMessage::SetHostname(hostname) => {
                self.hostname = hostname;
                Task::none()
            }
            IMessage::SetPassword(password) => {
                self.password = password;
                Task::none()
            }
            IMessage::Save => todo!(),
        }
    }

    pub fn view<'state>(&'state self) -> Element<'state, Message> {
        container(
            Column::new()
                .push(pick_list(Protocol::ALL, Some(self.protocol), |value| {
                    Message::Internal(IMessage::SetProtocol(value))
                }))
                .push(
                    // TOOD: generate the prompt from the plugin's connection builder
                    Column::new()
                        .spacing(4)
                        .push(
                            text_input("Hostname", &self.hostname)
                                .on_input(|value| Message::Internal(IMessage::SetHostname(value))),
                        )
                        .push(
                            text_input("Password", &self.password)
                                .secure(true)
                                .on_input(|value| Message::Internal(IMessage::SetPassword(value))),
                        )
                        .push(Row::new().push(checkbox(false)).push("Read-only")),
                )
                .push(
                    Row::new()
                        .push(
                            button("Save")
                                .style(button::subtle)
                                .width(Length::Fill)
                                .on_press(Message::Internal(IMessage::Save)),
                        )
                        .push(
                            button("Connect")
                                .style(button::primary)
                                .width(Length::Fill)
                                .on_press(Message::Connect(Request {
                                    protocol: self.protocol,
                                    hostname: self.hostname.clone(),
                                    password: self.password.clone(),
                                })),
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
        let _ = state.update(IMessage::SetProtocol(Protocol::Vnc));
        assert_eq!(state.protocol, Protocol::Vnc);
    }

    #[test]
    fn set_hostname_updates_the_hostname() {
        let mut state = State::new();
        let _ = state.update(IMessage::SetHostname("example.com".to_owned()));
        assert_eq!(state.hostname, "example.com");
    }

    #[test]
    fn set_password_updates_the_password() {
        let mut state = State::new();
        let _ = state.update(IMessage::SetPassword("hunter2".to_owned()));
        assert_eq!(state.password, "hunter2");
    }
}
