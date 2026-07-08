use iced::{
    Element, Length, Task,
    widget::{Column, Row, button, checkbox, container, pick_list, text_input},
};

pub struct State {
    protocol: &'static str,
}

#[derive(Debug, Clone)]
pub enum Message {
    Internal(IMessage),
}

#[derive(Debug, Clone)]
pub enum IMessage {
    None,
    SetProtocol(&'static str),
    Save,
    Connect,
}

impl State {
    pub fn new() -> State {
        State { protocol: "vnc" }
    }

    pub fn update(&mut self, message: IMessage) -> Task<Message> {
        match message {
            IMessage::None => Task::none(),
            IMessage::SetProtocol(protocol) => {
                self.protocol = protocol;
                Task::none()
            }
            IMessage::Save => todo!(),
            IMessage::Connect => todo!(),
        }
    }

    pub fn view<'state>(&'state self) -> Element<'state, Message> {
        container(
            Column::new()
                .push(pick_list(
                    ["dummy", "vnc", "spice", "ilo"],
                    Some(self.protocol),
                    |value| Message::Internal(IMessage::SetProtocol(value)),
                ))
                .push(
                    // TOOD: generate the prompt from the plugin's connection builder
                    Column::new()
                        .spacing(4)
                        .push(text_input("Hostname", ""))
                        .push(text_input("Password", ""))
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
                                .on_press(Message::Internal(IMessage::Connect)),
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
