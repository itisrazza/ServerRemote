//! The app's theme and the styles shared across the window chrome: title bar,
//! toolbar, menus and status bar.
//!
//! Colours and sizes follow the Figma mockups.

use iced::theme::{Palette, palette};
use iced::widget::{button, container, rule};
use iced::{Background, Border, Color, Element, Theme};

/// Height of every title bar and toolbar control, so they line up.
pub const CONTROL_HEIGHT: f32 = 28.0;
pub const ICON_SIZE: f32 = 16.0;
/// Gap between neighbouring controls, and the window's outer margin.
pub const SPACING: f32 = 4.0;
const RADIUS: f32 = 6.0;

const PALETTE: Palette = Palette {
    background: Color::from_rgb8(0x12, 0x12, 0x12),
    text: Color::WHITE,
    primary: Color::from_rgb8(0x4c, 0x8d, 0xff),
    success: Color::from_rgb8(0x00, 0xc9, 0x50),
    warning: Color::from_rgb8(0xff, 0xc8, 0x3d),
    danger: Color::from_rgb8(0xc4, 0x2b, 0x1c),
};

pub fn theme() -> Theme {
    Theme::custom_with_fn("Server Remote", PALETTE, extended_palette)
}

/// iced derives background shades by mixing; the mockups use specific greys,
/// so they're set explicitly:
///
/// - `weak`: menu fill
/// - `neutral`: hover
/// - `strong`: borders, dividers, active tab fill
/// - `stronger`: menu border and separators
/// - `strongest`: active tab border, disabled text
fn extended_palette(palette: Palette) -> palette::Extended {
    let pair = |r, g, b| palette::Pair::new(Color::from_rgb8(r, g, b), palette.text);
    palette::Extended {
        background: palette::Background {
            base: pair(0x12, 0x12, 0x12),
            weakest: pair(0x17, 0x17, 0x17),
            weaker: pair(0x1c, 0x1c, 0x1c),
            weak: pair(0x21, 0x21, 0x21),
            neutral: pair(0x2c, 0x2c, 0x2c),
            strong: pair(0x41, 0x41, 0x41),
            stronger: pair(0x4d, 0x4d, 0x4d),
            strongest: pair(0x68, 0x68, 0x68),
        },
        ..palette::Extended::generate(palette)
    }
}

/// A bordered, rounded button — tabs and toolbar buttons. `active` keeps it
/// highlighted, e.g. the selected tab or a toolbar button with its menu open.
pub fn bordered_button(theme: &Theme, status: button::Status, active: bool) -> button::Style {
    let palette = theme.extended_palette();
    let (background, border) = match (active, status) {
        (true, _) => (
            Some(palette.background.strong.color),
            palette.background.strongest.color,
        ),
        (false, button::Status::Pressed) => (
            Some(palette.background.strong.color),
            palette.background.strong.color,
        ),
        (false, button::Status::Hovered) => (
            Some(palette.background.neutral.color),
            palette.background.strong.color,
        ),
        (false, button::Status::Active | button::Status::Disabled) => {
            (None, palette.background.strong.color)
        }
    };
    button::Style {
        background: background.map(Background::Color),
        text_color: palette.background.base.text,
        border: Border {
            color: border,
            width: 1.0,
            radius: RADIUS.into(),
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

/// The floating panel menus and popouts are drawn in.
pub fn popup(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.background.weak.color)),
        text_color: Some(palette.background.base.text),
        border: Border {
            color: palette.background.stronger.color,
            width: 1.0,
            radius: 8.0.into(),
        },
        ..container::Style::default()
    }
}

pub fn divider(theme: &Theme) -> rule::Style {
    rule::Style {
        color: theme.extended_palette().background.strong.color,
        ..rule::default(theme)
    }
}

pub fn popup_divider(theme: &Theme) -> rule::Style {
    rule::Style {
        color: theme.extended_palette().background.stronger.color,
        ..rule::default(theme)
    }
}

/// A short vertical line separating groups in a row.
pub fn vertical_divider<'a, Message: 'a>(height: f32) -> Element<'a, Message> {
    container(rule::vertical(1).style(divider))
        .height(height)
        .into()
}
