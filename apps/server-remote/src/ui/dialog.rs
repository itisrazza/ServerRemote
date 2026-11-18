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

pub struct Button<Message> {
    pub label: String,
    pub on_press: Message,
    pub kind: Kind,
}

const WIDTH: f32 = 320.0;
/// For dialogs with more choices than fit the usual width.
pub const WIDE: f32 = 460.0;

/// A dialog meant to be layered over the view it interrupts (e.g. with
/// `stack!`). `buttons` are laid out left to right, right-aligned.
pub fn view<'a, Message: Clone + 'a>(
    title: String,
    message: String,
    buttons: Vec<Button<Message>>,
) -> Element<'a, Message> {
    view_with(title, message, Vec::new(), buttons, WIDTH)
}

/// Like [`view`], with `leading` buttons set apart at the left, `width` wide.
pub fn view_with<'a, Message: Clone + 'a>(
    title: String,
    message: String,
    leading: Vec<Button<Message>>,
    buttons: Vec<Button<Message>>,
    width: f32,
) -> Element<'a, Message> {
    let leading = Row::with_children(leading.into_iter().map(button_view)).spacing(8);
    let buttons = Row::with_children(buttons.into_iter().map(button_view)).spacing(8);
    let dialog = container(
        Column::new()
            .spacing(12)
            .push(text(title).size(14).font(style::BOLD))
            .push(text(message))
            .push(
                Row::new()
                    .spacing(8)
                    .push(leading)
                    .push(Space::new().width(Length::Fill))
                    .push(buttons),
            ),
    )
    .width(width)
    .padding(16)
    .style(style::dialog);

    opaque(
        container(dialog)
            .center(Length::Fill)
            .style(style::backdrop),
    )
}

fn button_view<'a, Message: Clone + 'a>(spec: Button<Message>) -> Element<'a, Message> {
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
