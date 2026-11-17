//! Styles shared across the window chrome: title bar, toolbar and status bar.

use iced::widget::{button, container, rule};
use iced::{Background, Border, Element, Theme};

/// A bordered, rounded button — tabs and toolbar buttons. `active` keeps it
/// highlighted, e.g. the selected tab or a toolbar button with its menu open.
pub fn bordered_button(theme: &Theme, status: button::Status, active: bool) -> button::Style {
    let palette = theme.extended_palette();
    let background = match (active, status) {
        (true, _) | (_, button::Status::Pressed) => Some(palette.background.weak.color),
        (_, button::Status::Hovered) => Some(palette.background.weaker.color),
        _ => None,
    };
    button::Style {
        background: background.map(Background::Color),
        text_color: palette.background.base.text,
        border: Border {
            color: palette.background.strong.color,
            width: 1.0,
            radius: 8.0.into(),
        },
        ..button::Style::default()
    }
}

/// A borderless button that only shows a background when hovered.
pub fn flat_button(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => Some(palette.background.strong.color),
        button::Status::Active | button::Status::Disabled => None,
    };
    button::Style {
        background: background.map(Background::Color),
        text_color: palette.background.base.text,
        border: iced::border::rounded(4),
        ..button::Style::default()
    }
}

pub fn divider(theme: &Theme) -> rule::Style {
    rule::Style {
        color: theme.extended_palette().background.strong.color,
        ..rule::default(theme)
    }
}

/// A short vertical line separating groups in a row.
pub fn vertical_divider<'a, Message: 'a>(height: f32) -> Element<'a, Message> {
    container(rule::vertical(1).style(divider))
        .height(height)
        .into()
}
