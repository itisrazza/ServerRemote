//! Modal dialogs: a titled message with buttons, centred over a dimmed
//! backdrop that swallows clicks to everything underneath.

use iced::widget::{Column, Row, Space, button, container, opaque, text};
use iced::{Element, Length};

use super::style;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Bordered, e.g. Cancel.
    Secondary,
    /// The main way forward.
    Primary,
    /// The main way forward, when it can lose work.
    Destructive,
}

pub struct Button<'a, Message> {
    pub label: &'a str,
    pub on_press: Message,
    pub kind: Kind,
}

/// A dialog meant to be layered over the view it interrupts (e.g. with
/// `stack!`). `buttons` are laid out left to right, right-aligned.
pub fn view<'a, Message: Clone + 'a>(
    title: &'a str,
    message: &'a str,
    buttons: Vec<Button<'a, Message>>,
) -> Element<'a, Message> {
    let buttons = Row::with_children(buttons.into_iter().map(button_view)).spacing(8);
    let dialog = container(
        Column::new()
            .spacing(12)
            .push(text(title).size(14).font(style::BOLD))
            .push(text(message))
            .push(
                Row::new()
                    .push(Space::new().width(Length::Fill))
                    .push(buttons),
            ),
    )
    .width(320)
    .padding(16)
    .style(style::dialog);

    opaque(
        container(dialog)
            .center(Length::Fill)
            .style(style::backdrop),
    )
}

fn button_view<'a, Message: Clone + 'a>(spec: Button<'a, Message>) -> Element<'a, Message> {
    let kind = spec.kind;
    button(container(text(spec.label)).center_y(Length::Fill))
        .height(style::CONTROL_HEIGHT)
        .padding([0, 12])
        .style(move |theme, status| match kind {
            Kind::Secondary => style::bordered_button(theme, status, false),
            Kind::Primary => style::dialog_button(theme, status, false),
            Kind::Destructive => style::dialog_button(theme, status, true),
        })
        .on_press(spec.on_press)
        .into()
}
