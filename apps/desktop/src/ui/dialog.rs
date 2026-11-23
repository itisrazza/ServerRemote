//! Modal dialogs: a titled message with buttons, centred over a dimmed
//! backdrop that swallows clicks to everything underneath.

use iced::widget::{Column, Row, Space, button, container, opaque, svg, text, tooltip};
use iced::{Element, Length};
use server_remote_icons::Icon;

use super::{style, themed_icon};
use crate::tooltip_box::tooltip_box;

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
    /// Shown instead of the label, which becomes the button's tooltip.
    pub icon: Option<Icon>,
    /// A disabled button is greyed out and ignores presses.
    pub enabled: bool,
}

pub const WIDTH: f32 = 320.0;
pub const PADDING: f32 = 16.0;
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
    view_content(title, text(message), leading, buttons, width)
}

/// A dialog with any `content` (a form, say) in place of the message.
pub fn view_content<'a, Message: Clone + 'a>(
    title: String,
    content: impl Into<Element<'a, Message>>,
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
            .push(content)
            .push(
                Row::new()
                    .spacing(8)
                    .push(leading)
                    .push(Space::new().width(Length::Fill))
                    .push(buttons),
            ),
    )
    .width(width)
    .padding(PADDING)
    .style(style::dialog);

    opaque(
        container(dialog)
            .center(Length::Fill)
            .style(style::backdrop),
    )
}

fn button_view<'a, Message: Clone + 'a>(spec: Button<Message>) -> Element<'a, Message> {
    let kind = spec.kind;
    let styled = |content: Element<'a, Message>, padding: u16| {
        button(container(content).center_y(Length::Fill))
            .height(style::CONTROL_HEIGHT)
            .padding([0, padding])
            .style(move |theme, status| match kind {
                Kind::Secondary => style::bordered_button(theme, status, false),
                Kind::Primary => style::dialog_button(theme, status, false),
                Kind::Destructive => style::dialog_button(theme, status, true),
            })
            .on_press_maybe(spec.enabled.then(|| spec.on_press.clone()))
    };
    let Some(icon) = spec.icon else {
        return styled(text(spec.label).into(), 12).into();
    };
    let icon = themed_icon(svg(svg::Handle::from_memory(icon.svg())), |palette| {
        palette.background.base.text
    });
    tooltip(
        styled(icon.into(), 8),
        tooltip_box(text(spec.label)),
        tooltip::Position::Top,
    )
    .into()
}
