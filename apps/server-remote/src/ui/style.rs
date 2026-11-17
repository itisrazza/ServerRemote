//! The app's theme and the styles shared across the window chrome: title bar,
//! toolbar, menus and status bar.
//!
//! Colours and sizes follow the Figma mockups.

use std::borrow::Cow;
use std::sync::LazyLock;

use iced::font::{Family, Stretch, Weight};
use iced::theme::{Palette, palette};
use iced::widget::{button, container, rule};
use iced::{Background, Border, Color, Element, Font, Settings, Shadow, Theme, Vector};

/// Height of every title bar and toolbar control, so they line up.
pub const CONTROL_HEIGHT: f32 = 28.0;
pub const ICON_SIZE: f32 = 16.0;
/// Gap between neighbouring controls, and the window's outer margin.
pub const SPACING: f32 = 4.0;
const RADIUS: f32 = 6.0;

/// Nunito Sans, pinned by `scripts/instance-fonts.py` to the design's axis
/// settings (maximum lowercase height, optical size 12).
const FONT_FILES: [&[u8]; 4] = [
    include_bytes!("../../assets/fonts/NunitoSans-Medium.ttf"),
    include_bytes!("../../assets/fonts/NunitoSans-Bold.ttf"),
    include_bytes!("../../assets/fonts/NunitoSans-CondensedMedium.ttf"),
    include_bytes!("../../assets/fonts/NunitoSans-SemiExpandedBlack.ttf"),
];

pub const FONT: Font = Font {
    family: Family::Name("Nunito Sans"),
    weight: Weight::Medium,
    stretch: Stretch::Normal,
    style: iced::font::Style::Normal,
};
pub const BOLD: Font = Font {
    weight: Weight::Bold,
    ..FONT
};
/// The narrowest width, for word legends on the virtual keyboard's keys.
pub const CONDENSED: Font = Font {
    stretch: Stretch::Condensed,
    ..FONT
};
/// Wide and heavy, for the app's name on the About page.
pub const DISPLAY: Font = Font {
    weight: Weight::Black,
    stretch: Stretch::SemiExpanded,
    ..FONT
};
pub const TEXT_SIZE: f32 = 13.0;
/// iced's default line height, as a multiple of the text size.
const LINE_HEIGHT: f32 = 1.3;
/// Nunito Sans's ascent and descent, in ems.
const ASCENT: f32 = 1.011;
const DESCENT: f32 = 0.353;

/// How far below the top of its line text of `size` has its baseline, so
/// text of different sizes in a row can share one.
pub fn baseline(size: f32) -> f32 {
    size * (LINE_HEIGHT + ASCENT - DESCENT) / 2.0
}

/// Application settings carrying the embedded fonts and default text style.
pub fn settings() -> Settings {
    Settings {
        fonts: FONT_FILES.into_iter().map(Cow::Borrowed).collect(),
        default_font: FONT,
        default_text_size: TEXT_SIZE.into(),
        ..Settings::default()
    }
}

const DARK_PALETTE: Palette = Palette {
    background: Color::from_rgb8(0x12, 0x12, 0x12),
    text: Color::WHITE,
    primary: Color::from_rgb8(0x4c, 0x8d, 0xff),
    success: Color::from_rgb8(0x00, 0xc9, 0x50),
    warning: Color::from_rgb8(0xff, 0xd2, 0x30),
    danger: Color::from_rgb8(0xc4, 0x2b, 0x1c),
};

const LIGHT_PALETTE: Palette = Palette {
    background: Color::from_rgb8(0xf0, 0xf0, 0xf0),
    text: Color::from_rgb8(0x1a, 0x1a, 0x1a),
    primary: Color::from_rgb8(0x2f, 0x6f, 0xeb),
    success: Color::from_rgb8(0x00, 0xa8, 0x43),
    warning: Color::from_rgb8(0xf0, 0xb4, 0x00),
    danger: Color::from_rgb8(0xc4, 0x2b, 0x1c),
};

// Themes are rebuilt from their palettes on every call otherwise.
static DARK: LazyLock<Theme> =
    LazyLock::new(|| Theme::custom_with_fn("Server Remote Dark", DARK_PALETTE, dark_extended));
static LIGHT: LazyLock<Theme> =
    LazyLock::new(|| Theme::custom_with_fn("Server Remote Light", LIGHT_PALETTE, light_extended));

pub fn dark_theme() -> Theme {
    DARK.clone()
}

pub fn light_theme() -> Theme {
    LIGHT.clone()
}

/// The background shades, from the window's own background outwards.
struct Greys {
    base: u8,
    weakest: u8,
    weaker: u8,
    weak: u8,
    neutral: u8,
    strong: u8,
    stronger: u8,
    strongest: u8,
}

/// iced derives background shades by mixing; the mockups use specific greys,
/// so they're set explicitly:
///
/// - `weak`: menu and panel fill
/// - `neutral`: hover
/// - `strong`: borders, dividers, active tab fill
/// - `stronger`: menu border and separators
/// - `strongest`: active tab border, disabled and secondary text
fn extended_palette(palette: Palette, greys: Greys) -> palette::Extended {
    let pair = |grey| palette::Pair::new(Color::from_rgb8(grey, grey, grey), palette.text);
    palette::Extended {
        background: palette::Background {
            base: pair(greys.base),
            weakest: pair(greys.weakest),
            weaker: pair(greys.weaker),
            weak: pair(greys.weak),
            neutral: pair(greys.neutral),
            strong: pair(greys.strong),
            stronger: pair(greys.stronger),
            strongest: pair(greys.strongest),
        },
        ..palette::Extended::generate(palette)
    }
}

fn dark_extended(palette: Palette) -> palette::Extended {
    extended_palette(
        palette,
        Greys {
            base: 0x12,
            weakest: 0x17,
            weaker: 0x1c,
            weak: 0x21,
            neutral: 0x2c,
            strong: 0x41,
            stronger: 0x4d,
            strongest: 0x68,
        },
    )
}

/// The dark greys' roles mirrored: panels and menus are the lightest
/// surface, and `strongest` is dark enough to read as secondary text.
fn light_extended(palette: Palette) -> palette::Extended {
    extended_palette(
        palette,
        Greys {
            base: 0xf0,
            weakest: 0xf4,
            weaker: 0xf8,
            weak: 0xff,
            neutral: 0xe6,
            strong: 0xd2,
            stronger: 0xc4,
            strongest: 0x76,
        },
    )
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

/// The small bordered close button inside a tab, outlined in the same colour
/// as the tab around it.
pub fn tab_close_button(theme: &Theme, status: button::Status, tab_active: bool) -> button::Style {
    let palette = theme.extended_palette();
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => Some(palette.background.neutral.color),
        button::Status::Active | button::Status::Disabled => None,
    };
    let border = if tab_active {
        palette.background.strongest.color
    } else {
        palette.background.strong.color
    };
    button::Style {
        background: background.map(Background::Color),
        text_color: palette.background.base.text,
        border: Border {
            color: border,
            width: 1.0,
            radius: 4.0.into(),
        },
        ..button::Style::default()
    }
}

/// An entry in the Settings tab's section list; `active` is the open one.
pub fn sidebar_item(theme: &Theme, status: button::Status, active: bool) -> button::Style {
    let palette = theme.extended_palette();
    // The open section shares the content panel's fill, as if part of it.
    let background = match (active, status) {
        (true, _) => Some(palette.background.weak.color),
        (false, button::Status::Hovered | button::Status::Pressed) => {
            Some(palette.background.weaker.color)
        }
        (false, button::Status::Active | button::Status::Disabled) => None,
    };
    button::Style {
        background: background.map(Background::Color),
        text_color: palette.background.base.text,
        border: iced::border::rounded(RADIUS),
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

/// A borderless button with a solid fill, e.g. the About page's links.
pub fn filled_button(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => palette.background.strong.color,
        button::Status::Active | button::Status::Disabled => palette.background.neutral.color,
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: palette.background.base.text,
        border: iced::border::rounded(RADIUS),
        ..button::Style::default()
    }
}

/// The rounded panel a tab's main content sits in, e.g. a Settings section.
pub fn panel(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.background.weak.color)),
        text_color: Some(palette.background.base.text),
        border: iced::border::rounded(8),
        ..container::Style::default()
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

/// An ongoing thing in the toolbar (a mounted image, a recording), drawn like
/// the active tab.
pub fn chip(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.background.strong.color)),
        text_color: Some(palette.background.base.text),
        border: Border {
            color: palette.background.strongest.color,
            width: 1.0,
            radius: RADIUS.into(),
        },
        ..container::Style::default()
    }
}

/// The states a status bar dot can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Indicator {
    Ok,
    Warning,
    Error,
    Inactive,
}

impl Indicator {
    fn color(self, theme: &Theme) -> Color {
        let palette = theme.extended_palette();
        match self {
            Indicator::Ok => palette.success.base.color,
            Indicator::Warning => palette.warning.base.color,
            Indicator::Error => Color::from_rgb8(0xfb, 0x2c, 0x36),
            Indicator::Inactive => Color::from_rgb8(0x62, 0x74, 0x8e),
        }
    }
}

/// The 8pt coloured dot beside a status bar label.
pub fn status_dot<'a, Message: 'a>(indicator: Indicator) -> Element<'a, Message> {
    container(iced::widget::Space::new())
        .width(8)
        .height(8)
        .style(move |theme| container::Style {
            background: Some(Background::Color(indicator.color(theme))),
            border: iced::border::rounded(4),
            ..container::Style::default()
        })
        .into()
}

/// The dimmed layer behind a modal dialog.
pub fn backdrop(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgba8(0, 0, 0, 0.5))),
        ..container::Style::default()
    }
}

/// A modal dialog's panel.
pub fn dialog(theme: &Theme) -> container::Style {
    container::Style {
        shadow: Shadow {
            color: Color::from_rgba8(0, 0, 0, 0.5),
            offset: Vector::new(0.0, 8.0),
            blur_radius: 24.0,
        },
        ..popup(theme)
    }
}

/// A dialog's main button; `destructive` paints it red.
pub fn dialog_button(theme: &Theme, status: button::Status, destructive: bool) -> button::Style {
    let palette = theme.extended_palette();
    let (base, strong) = if destructive {
        (palette.danger.base, palette.danger.strong)
    } else {
        (palette.primary.base, palette.primary.strong)
    };
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => strong.color,
        button::Status::Active | button::Status::Disabled => base.color,
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: base.text,
        border: iced::border::rounded(RADIUS),
        ..button::Style::default()
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
