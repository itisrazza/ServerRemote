use iced::{
    Alignment, Element, Length, Task,
    widget::{Column, Row, Space, button, container, text, text_input, tooltip},
};
use server_remote_icons as icons;

use super::menu::{self, Item};
use super::{dialog, icon, style};
use crate::i18n::fl;
use crate::persistence::sessions::SavedSession;
use crate::tooltip_box::tooltip_box;
use server_remote_app::Protocol;
use server_remote_app::connect::Request;

/// The Connect tab's panel.
const PANEL_WIDTH: f32 = 245.0;
const PANEL_PADDING: f32 = 8.0;
/// Labels sit in a column this wide, left of their fields.
const LABEL_WIDTH: f32 = 75.0;
const FIELD_LINE_HEIGHT: f32 = 18.0;

/// The connection form: the Connect tab, and the connection manager's Add
/// and Edit dialogs.
pub struct State {
    protocol: Protocol,
    /// Whether the protocol dropdown is open.
    choosing_protocol: bool,
    /// The name being typed in the Save dialog, while it's open.
    naming: Option<String>,
    name: String,
    group: String,
    hostname: String,
    username: String,
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

#[derive(Debug, Clone)]
pub enum IMessage {
    ToggleProtocols,
    CloseProtocols,
    Protocol(Protocol),
    Name(String),
    Hostname(String),
    Username(String),
    Password(String),
    /// Enter was pressed in a field; whoever shows the form decides what
    /// that does.
    Submit,
    /// Opens the dialog asking what to save the connection as.
    AskName,
    SaveAs(String),
    /// Closes the Save dialog, cancelled or done.
    CloseNaming,
}

impl State {
    pub fn new() -> State {
        State {
            protocol: Protocol::Dummy,
            choosing_protocol: false,
            naming: None,
            name: String::new(),
            group: String::new(),
            hostname: String::new(),
            username: String::new(),
            password: String::new(),
        }
    }

    /// The form filled in from a saved session, for editing it.
    pub fn from_session(session: &SavedSession) -> State {
        State {
            protocol: session.protocol,
            name: session.name.clone(),
            group: session.group.clone(),
            hostname: session.hostname.clone(),
            username: session.username.clone(),
            password: session.password.clone(),
            ..State::new()
        }
    }

    /// A blank form that saves into `group`, the group on show when adding.
    pub fn in_group(group: Option<&str>) -> State {
        State {
            group: group.unwrap_or_default().to_owned(),
            ..State::new()
        }
    }

    pub fn update(&mut self, message: IMessage) -> Task<Message> {
        match message {
            IMessage::ToggleProtocols => self.choosing_protocol = !self.choosing_protocol,
            IMessage::CloseProtocols => self.choosing_protocol = false,
            IMessage::Protocol(protocol) => {
                self.protocol = protocol;
                self.choosing_protocol = false;
            }
            IMessage::Name(name) => self.name = name,
            IMessage::Hostname(hostname) => self.hostname = hostname,
            IMessage::Username(username) => self.username = username,
            IMessage::Password(password) => self.password = password,
            IMessage::Submit => {}
            IMessage::AskName => self.naming = Some(String::new()),
            IMessage::SaveAs(name) => {
                if let Some(naming) = &mut self.naming {
                    *naming = name;
                }
            }
            IMessage::CloseNaming => self.naming = None,
        }
        Task::none()
    }

    pub fn request(&self) -> Request {
        Request {
            protocol: self.protocol,
            name: self.name.clone(),
            group: self.group.trim().to_owned(),
            hostname: self.hostname.clone(),
            username: self.username.clone(),
            password: self.password.clone(),
        }
    }

    /// The form's fields, without buttons, `width` wide. `with_name` adds a
    /// Name field, for saved sessions.
    pub fn fields(&self, width: f32, with_name: bool) -> Element<'_, IMessage> {
        // TODO: generate the prompt from the plugin's connection builder
        let protocols = Protocol::ALL
            .iter()
            .map(|&protocol| {
                Item::entry(protocol.full_name(), Some(IMessage::Protocol(protocol)))
                    .icon(protocol.icon())
            })
            .collect();
        let protocol = menu::dropdown(
            self.protocol.icon().into(),
            self.protocol.full_name().to_owned(),
            width,
            protocols,
            self.choosing_protocol,
            IMessage::ToggleProtocols,
            IMessage::CloseProtocols,
        );

        // A blank name saves as the hostname, so that's the hint.
        let name_hint = if self.hostname.is_empty() {
            self.protocol.full_name()
        } else {
            &self.hostname
        };
        let port = self.protocol.default_port();
        let host_hint = if port == 0 {
            fl!("connect-hostname-placeholder")
        } else {
            fl!("connect-hostname-placeholder-port", port = port.to_string())
        };

        Column::new()
            .spacing(style::SPACING)
            .push(protocol)
            .push(with_name.then(|| {
                row(
                    fl!("connect-name"),
                    field(name_hint, &self.name).on_input(IMessage::Name),
                )
            }))
            .push(row(
                fl!("connect-hostname"),
                field(&host_hint, &self.hostname).on_input(IMessage::Hostname),
            ))
            .push((self.protocol == Protocol::Ilo).then(|| {
                row(
                    fl!("connect-username"),
                    field(&fl!("connect-username-placeholder"), &self.username)
                        .on_input(IMessage::Username),
                )
            }))
            .push(row(
                fl!("connect-password"),
                field("", &self.password)
                    .secure(true)
                    .on_input(IMessage::Password),
            ))
            .width(width)
            .into()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let request = self.request();
        let fields = self
            .fields(PANEL_WIDTH - 2.0 * PANEL_PADDING, false)
            .map(move |message| match message {
                IMessage::Submit => Message::Connect(request.clone()),
                message => Message::Internal(message),
            });

        let save_icon = icon::view(icons::SAVE.into(), style::ICON_SIZE, |theme| {
            theme.extended_palette().background.base.text
        });
        let save = tooltip(
            button(container(save_icon).center(Length::Fill))
                .width(style::CONTROL_HEIGHT)
                .height(style::CONTROL_HEIGHT)
                .padding(0)
                .style(style::inset_button)
                .on_press(Message::Internal(IMessage::AskName)),
            tooltip_box(text(fl!("connect-save-tooltip"))),
            tooltip::Position::Bottom,
        );
        let connect = button(container(text(fl!("connect-connect"))).center_y(Length::Fill))
            .height(style::CONTROL_HEIGHT)
            .padding([0, 12])
            .style(|theme, status| style::dialog_button(theme, status, false))
            .on_press(Message::Connect(self.request()));
        let buttons = Row::new()
            .push(save)
            .push(Space::new().width(Length::Fill))
            .push(connect);

        container(
            container(
                Column::new()
                    .spacing(style::SPACING)
                    .push(fields)
                    .push(buttons),
            )
            .width(PANEL_WIDTH)
            .padding(PANEL_PADDING)
            .style(style::panel),
        )
        .center(Length::Fill)
        .into()
    }
}

impl State {
    /// The dialog asking what to save the connection as, while it's open,
    /// to be layered over the whole window.
    pub fn modal(&self) -> Option<Element<'_, Message>> {
        let name = self.naming.as_ref()?;
        // A blank name saves as the hostname.
        let hint = if self.hostname.is_empty() {
            self.protocol.full_name()
        } else {
            &self.hostname
        };
        let save = Message::Save(Request {
            name: name.trim().to_owned(),
            ..self.request()
        });
        let field = field(hint, name)
            .on_input(|name| Message::Internal(IMessage::SaveAs(name)))
            .on_submit(save.clone());
        Some(dialog::view_content(
            fl!("connect-save-title"),
            Row::new()
                .spacing(style::SPACING)
                .align_y(Alignment::Center)
                .push(text(fl!("connect-name")).width(LABEL_WIDTH))
                .push(field),
            Vec::new(),
            vec![
                dialog::Button {
                    label: fl!("button-cancel"),
                    on_press: Message::Internal(IMessage::CloseNaming),
                    kind: dialog::Kind::Secondary,
                    icon: None,
                    enabled: true,
                },
                dialog::Button {
                    label: fl!("connect-save"),
                    on_press: save,
                    kind: dialog::Kind::Primary,
                    icon: None,
                    enabled: true,
                },
            ],
            dialog::WIDTH,
        ))
    }
}

/// A field with its label to the left.
fn row<'a>(label: String, field: text_input::TextInput<'a, IMessage>) -> Element<'a, IMessage> {
    Row::new()
        .spacing(style::SPACING)
        .align_y(Alignment::Center)
        .push(text(label).width(LABEL_WIDTH))
        .push(field.on_submit(IMessage::Submit))
        .into()
}

pub(super) fn field<'a, M: Clone + 'a>(
    placeholder: &str,
    value: &str,
) -> text_input::TextInput<'a, M> {
    // A whole-pixel line makes the field exactly control height, so nothing
    // in or below it lands between pixels.
    text_input(placeholder, value)
        .width(Length::Fill)
        .line_height(text::LineHeight::Absolute(FIELD_LINE_HEIGHT.into()))
        .padding([5, 8])
        .style(style::text_field)
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
    fn choosing_a_protocol_closes_the_dropdown() {
        let mut state = State::new();
        let _ = state.update(IMessage::ToggleProtocols);
        assert!(state.choosing_protocol);
        let _ = state.view();

        let _ = state.update(IMessage::Protocol(Protocol::Vnc));
        assert_eq!(state.protocol, Protocol::Vnc);
        assert!(!state.choosing_protocol);
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
        let _ = state.update(IMessage::Username("admin".to_owned()));
        let _ = state.update(IMessage::Password("hunter2".to_owned()));

        let request = state.request();

        assert_eq!(request.protocol, Protocol::Vnc);
        assert_eq!(request.name, "Home NAS");
        assert_eq!(request.hostname, "example.com");
        assert_eq!(request.username, "admin");
        assert_eq!(request.password, "hunter2");
    }
}
