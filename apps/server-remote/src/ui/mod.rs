mod arrow_cursor;
pub mod connect;
pub mod connection;
mod contributed;
pub mod dialog;
mod icon;
mod kvm_display;
pub mod manager;
mod menu;
mod pixel_snap;
mod popover;
pub mod settings;
pub mod style;
mod virtual_keyboard;
mod virtual_trackpad;
pub mod window_controls;

pub use arrow_cursor::arrow_cursor;
pub use pixel_snap::pixel_snap;

/// Sizes an icon and tints it to match the given button's text color —
/// `Svg` ignores the ambient text color buttons apply to their other
/// children, so without this it renders in its SVG source's own (black)
/// color regardless of the button's style.
pub(crate) fn themed_icon<'a>(
    svg: iced::widget::svg::Svg<'a, iced::Theme>,
    color: impl Fn(&iced::theme::palette::Extended) -> iced::Color + 'a,
) -> iced::widget::svg::Svg<'a, iced::Theme> {
    svg.width(16.0)
        .height(16.0)
        .style(move |theme, _status| iced::widget::svg::Style {
            color: Some(color(theme.extended_palette())),
        })
}
