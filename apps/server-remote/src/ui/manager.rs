//! Lists saved connection profiles and offers to connect to or delete them.
//!
//! Has no state of its own beyond what the application already persists in
//! [`crate::persistence::sessions::Sessions`] — it's a straight projection of
//! that list, indexed by position for deletion.

use crate::i18n::fl;
use iced::{
    Alignment, Element, Length,
    widget::{Column, Row, Scrollable, button, text},
};

use crate::persistence::sessions::SavedSession;

use super::connect::Request;

#[derive(Debug, Clone)]
pub enum Message {
    /// Bubbles up to the application to open a connection tab.
    Connect(Request),
    /// Bubbles up to the application to remove a saved session by its index.
    Delete(usize),
}

pub fn view(sessions: &[SavedSession]) -> Element<'_, Message> {
    if sessions.is_empty() {
        return text(fl!("manager-empty")).into();
    }

    let rows = sessions.iter().enumerate().map(|(index, session)| {
        Row::new()
            .align_y(Alignment::Center)
            .spacing(8)
            .push(text(session.name.clone()).width(Length::Fill))
            .push(text(session.protocol.to_string()))
            .push(text(session.hostname.clone()))
            .push(
                button(text(fl!("connect-connect")))
                    .style(button::primary)
                    .on_press(Message::Connect(Request {
                        protocol: session.protocol,
                        name: session.name.clone(),
                        hostname: session.hostname.clone(),
                        username: session.username.clone(),
                        password: session.password.clone(),
                    })),
            )
            .push(
                button(text(fl!("manager-delete")))
                    .style(button::danger)
                    .on_press(Message::Delete(index)),
            )
            .into()
    });

    Scrollable::new(Column::with_children(rows).spacing(4))
        .width(Length::Fill)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::connection::Protocol;

    fn session(name: &str) -> SavedSession {
        SavedSession {
            name: name.to_owned(),
            protocol: Protocol::Vnc,
            hostname: "example.com".to_owned(),
            username: String::new(),
            password: "hunter2".to_owned(),
        }
    }

    #[test]
    fn view_renders_without_panicking_when_empty() {
        let _ = view(&[]);
    }

    #[test]
    fn view_renders_without_panicking_with_sessions() {
        let sessions = vec![session("home"), session("work")];
        let _ = view(&sessions);
    }
}
