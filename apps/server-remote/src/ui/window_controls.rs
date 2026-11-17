//! Minimise, maximise/restore and close buttons for platforms where the app
//! draws its own title bar.
//!
//! On Windows the glyphs come from Segoe Fluent Icons, the font the OS draws
//! its own caption buttons with. Elsewhere that font isn't available, so the
//! same shapes are drawn as small inline SVGs.

use iced::widget::{button, center, svg, text};
use iced::{Element, Font, Length, Theme};

use super::style;

/// Windows caption buttons are 46×32.
const WIDTH: f32 = 46.0;
const HEIGHT: f32 = 32.0;
const GLYPH_SIZE: f32 = 10.0;

const SEGOE_FLUENT_ICONS: Font = Font::with_name("Segoe Fluent Icons");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Glyph {
    Minimize,
    Maximize,
    Restore,
    Close,
}

impl Glyph {
    fn codepoint(self) -> char {
        match self {
            Glyph::Minimize => '\u{E921}',
            Glyph::Maximize => '\u{E922}',
            Glyph::Restore => '\u{E923}',
            Glyph::Close => '\u{E8BB}',
        }
    }

    fn svg(self) -> &'static [u8] {
        match self {
            Glyph::Minimize => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><path d="M0 5.5h10" stroke="white"/></svg>"#,
            Glyph::Maximize => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><rect x="0.5" y="0.5" width="9" height="9" fill="none" stroke="white"/></svg>"#,
            Glyph::Restore => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><rect x="0.5" y="2.5" width="7" height="7" fill="none" stroke="white"/><path d="M2.5 2.5V0.5h7v7h-2" fill="none" stroke="white"/></svg>"#,
            Glyph::Close => br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><path d="M0.5 0.5l9 9M9.5 0.5l-9 9" stroke="white"/></svg>"#,
        }
    }
}

pub fn view<'a, Message: Clone + 'a>(
    maximized: bool,
    on_minimize: Message,
    on_toggle_maximize: Message,
    on_close: Message,
) -> Element<'a, Message> {
    let maximize = if maximized {
        Glyph::Restore
    } else {
        Glyph::Maximize
    };

    iced::widget::Row::new()
        .push(control(Glyph::Minimize, on_minimize).style(style::flat_button))
        .push(control(maximize, on_toggle_maximize).style(style::flat_button))
        .push(control(Glyph::Close, on_close).style(close_button))
        .into()
}

fn control<'a, Message: 'a>(glyph: Glyph, on_press: Message) -> button::Button<'a, Message> {
    button(center(glyph_view(glyph)))
        .width(WIDTH)
        .height(HEIGHT)
        .padding(0)
        .on_press(on_press)
}

fn glyph_view<'a, Message: 'a>(glyph: Glyph) -> Element<'a, Message> {
    if cfg!(windows) {
        text(glyph.codepoint())
            .font(SEGOE_FLUENT_ICONS)
            .size(GLYPH_SIZE)
            .into()
    } else {
        svg(svg::Handle::from_memory(glyph.svg()))
            .width(Length::Fixed(GLYPH_SIZE))
            .height(Length::Fixed(GLYPH_SIZE))
            .style(|theme: &Theme, _status| svg::Style {
                color: Some(theme.extended_palette().background.base.text),
            })
            .into()
    }
}

/// Close turns red on hover, as the native caption button does.
fn close_button(theme: &Theme, status: button::Status) -> button::Style {
    let base = style::flat_button(theme, status);
    match status {
        button::Status::Hovered | button::Status::Pressed => {
            let danger = theme.extended_palette().danger.base;
            button::Style {
                text_color: danger.text,
                ..base.with_background(danger.color)
            }
        }
        button::Status::Active | button::Status::Disabled => base,
    }
}
