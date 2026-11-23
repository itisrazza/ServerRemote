//! Dropdown menus: a declarative list of [`Item`]s rendered as a popup, and a
//! toolbar button that opens one.
//!
//! Which menu (and submenu) is open is owned by the caller, which passes it
//! back in as `open` flags — the same way the rest of the UI is driven.

use iced::widget::{Column, Row, Space, button, container, mouse_area, rule, text};
use iced::{Alignment, Background, Element, Length, Padding, Theme, border};
use server_remote_icons as icons;

use super::icon;
use super::popover::{Placement, popover};
use super::style::{self, ICON_SIZE};

const MENU_WIDTH: f32 = 200.0;
/// A [`select`] is as wide as the menu it opens.
pub const SELECT_WIDTH: f32 = MENU_WIDTH;
/// The popup's 1pt border, which iced counts as padding.
const MENU_BORDER: f32 = 1.0;
const ITEM_HEIGHT: f32 = 25.0;
/// Item content is inset 6pt on the left (to the icon) and 8pt on the right
/// (to the chevron), measured from the popup's outer edge.
const ITEM_PADDING: Padding = Padding {
    top: 0.0,
    right: 8.0 - MENU_BORDER,
    bottom: 0.0,
    left: 6.0 - MENU_BORDER,
};
/// Separators stop 10pt short of either edge.
const SEPARATOR_INSET: f32 = 10.0 - MENU_BORDER;

pub enum Item<Message> {
    Entry {
        icon: Option<icon::Handle>,
        label: String,
        checked: bool,
        /// `None` renders the entry disabled.
        on_press: Option<Message>,
    },
    Submenu {
        label: String,
        items: Vec<Item<Message>>,
        open: bool,
        /// `None` renders the submenu disabled.
        on_open: Option<Message>,
    },
    Separator,
}

impl<Message> Item<Message> {
    pub fn entry(label: impl Into<String>, on_press: Option<Message>) -> Self {
        Item::Entry {
            icon: None,
            label: label.into(),
            checked: false,
            on_press,
        }
    }

    pub fn icon(self, icon: impl Into<icon::Handle>) -> Self {
        let icon = icon.into();
        match self {
            Item::Entry {
                label,
                checked,
                on_press,
                ..
            } => Item::Entry {
                icon: Some(icon),
                label,
                checked,
                on_press,
            },
            other => other,
        }
    }

    pub fn checked(self, checked: bool) -> Self {
        match self {
            Item::Entry {
                icon,
                label,
                on_press,
                ..
            } => Item::Entry {
                icon,
                label,
                checked,
                on_press,
            },
            other => other,
        }
    }

    pub fn submenu(
        label: impl Into<String>,
        items: Vec<Item<Message>>,
        open: bool,
        on_open: Option<Message>,
    ) -> Self {
        Item::Submenu {
            label: label.into(),
            items,
            open,
            on_open,
        }
    }
}

/// A toolbar button showing `icon` and a chevron that opens `items` below it.
pub fn button_menu<'a, Message: Clone + 'a>(
    icon: icon::Handle,
    items: Vec<Item<Message>>,
    open: bool,
    on_toggle: Message,
    on_dismiss: Message,
) -> Element<'a, Message> {
    let content = Row::new()
        .spacing(2)
        .align_y(Alignment::Center)
        .push(tinted(icon, ICON_SIZE))
        .push(tinted(icons::CHEVRON_DOWN.into(), ICON_SIZE));
    let anchor = button(container(content).center_y(Length::Fill))
        .height(style::CONTROL_HEIGHT)
        .padding([0, 5])
        .style(move |theme, status| style::bordered_button(theme, status, open))
        .on_press(on_toggle);

    popover(
        anchor,
        open.then(|| view(items)),
        Placement::Below,
        Some(on_dismiss),
    )
    .into()
}

/// A dropdown field showing `value`; the menu below it holds the choices,
/// with the current one ticked.
pub fn select<'a, Message: Clone + 'a>(
    value: String,
    items: Vec<Item<Message>>,
    open: bool,
    on_toggle: Message,
    on_dismiss: Message,
) -> Element<'a, Message> {
    let content = Row::new()
        .spacing(4)
        .align_y(Alignment::Center)
        .push(
            text(value)
                .width(Length::Fill)
                .wrapping(text::Wrapping::None),
        )
        .push(tinted(icons::CHEVRON_DOWN.into(), ICON_SIZE));
    let anchor = button(container(content).center_y(Length::Fill))
        .width(SELECT_WIDTH)
        .height(style::CONTROL_HEIGHT)
        .padding(Padding {
            top: 0.0,
            right: 5.0,
            bottom: 0.0,
            left: 8.0,
        })
        .style(move |theme, status| style::bordered_button(theme, status, open))
        .on_press(on_toggle);

    popover(
        anchor,
        open.then(|| view(items)),
        Placement::Below,
        Some(on_dismiss),
    )
    .into()
}

/// A dropdown field `width` wide showing `icon` and `value`, whose menu
/// below it is just as wide.
pub fn dropdown<'a, Message: Clone + 'a>(
    icon: icon::Handle,
    value: String,
    width: f32,
    items: Vec<Item<Message>>,
    open: bool,
    on_toggle: Message,
    on_dismiss: Message,
) -> Element<'a, Message> {
    let content = Row::new()
        .spacing(4)
        .align_y(Alignment::Center)
        .push(tinted(icon, ICON_SIZE))
        .push(
            text(value)
                .width(Length::Fill)
                .wrapping(text::Wrapping::None),
        )
        .push(tinted(icons::CHEVRON_DOWN.into(), ICON_SIZE));
    let anchor = button(container(content).center_y(Length::Fill))
        .width(width)
        .height(style::CONTROL_HEIGHT)
        .padding([0, 5])
        .style(move |theme, status| {
            if open {
                style::inset_button(theme, button::Status::Pressed)
            } else {
                style::inset_button(theme, status)
            }
        })
        .on_press(on_toggle);

    popover(
        anchor,
        open.then(|| view_with_width(items, width)),
        Placement::Below,
        Some(on_dismiss),
    )
    .into()
}

/// One entry per choice in `all`, ticking `current`.
pub fn choices<T: Copy + PartialEq, Message>(
    all: &[T],
    current: T,
    label: fn(T) -> String,
    on_choose: impl Fn(T) -> Message,
) -> Vec<Item<Message>> {
    all.iter()
        .map(|&choice| {
            Item::entry(label(choice), Some(on_choose(choice))).checked(choice == current)
        })
        .collect()
}

/// A square toolbar button showing `icon` that opens `content` below it.
pub fn button_popout<'a, Message: Clone + 'a>(
    icon: icon::Handle,
    content: impl FnOnce() -> Element<'a, Message>,
    open: bool,
    on_toggle: Message,
    on_dismiss: Message,
) -> Element<'a, Message> {
    let anchor = button(container(tinted(icon, ICON_SIZE)).center(Length::Fill))
        .width(style::CONTROL_HEIGHT)
        .height(style::CONTROL_HEIGHT)
        .padding(0)
        .style(move |theme, status| style::bordered_button(theme, status, open))
        .on_press(on_toggle);

    let popup = open.then(|| container(content()).padding(8).style(style::popup).into());
    popover(anchor, popup, Placement::Below, Some(on_dismiss)).into()
}

/// The popup itself.
pub fn view<'a, Message: Clone + 'a>(items: Vec<Item<Message>>) -> Element<'a, Message> {
    view_with_width(items, MENU_WIDTH)
}

/// The popup, `width` wide rather than the usual width.
fn view_with_width<'a, Message: Clone + 'a>(
    items: Vec<Item<Message>>,
    width: f32,
) -> Element<'a, Message> {
    let column =
        Column::with_children(items.into_iter().map(item_view)).width(width - 2.0 * MENU_BORDER);

    container(column)
        .padding(MENU_BORDER)
        .style(style::popup)
        .into()
}

fn item_view<'a, Message: Clone + 'a>(item: Item<Message>) -> Element<'a, Message> {
    match item {
        Item::Separator => container(rule::horizontal(1).style(style::popup_divider))
            .padding([0.0, SEPARATOR_INSET])
            .into(),
        Item::Entry {
            icon,
            label,
            checked,
            on_press,
        } => {
            let leading = checked.then(|| icons::CHECKMARK.into()).or(icon);
            let enabled = on_press.is_some();
            button(item_row(leading, label, None, enabled))
                .width(Length::Fill)
                .height(ITEM_HEIGHT)
                .padding(ITEM_PADDING)
                .style(menu_item)
                .on_press_maybe(on_press)
                .into()
        }
        Item::Submenu {
            label,
            items,
            open,
            on_open,
        } => {
            let enabled = on_open.is_some();
            let row = button(item_row(
                None,
                label,
                Some(icons::CHEVRON_RIGHT.into()),
                enabled,
            ))
            .width(Length::Fill)
            .height(ITEM_HEIGHT)
            .padding(ITEM_PADDING)
            .style(move |theme, status| {
                if open {
                    menu_item(theme, button::Status::Hovered)
                } else {
                    menu_item(theme, status)
                }
            })
            .on_press_maybe(on_open.clone());

            let row: Element<'a, Message> = match on_open {
                Some(on_open) => mouse_area(row).on_enter(on_open).into(),
                None => row.into(),
            };

            popover(row, open.then(|| view(items)), Placement::Right, None).into()
        }
    }
}

fn item_row<'a, Message: 'a>(
    leading: Option<icon::Handle>,
    label: String,
    trailing: Option<icon::Handle>,
    enabled: bool,
) -> Element<'a, Message> {
    let slot = |handle: Option<icon::Handle>| -> Element<'a, Message> {
        match handle {
            Some(handle) => tinted_by(handle, ICON_SIZE, enabled),
            None => Space::new().width(ICON_SIZE).height(ICON_SIZE).into(),
        }
    };

    Row::new()
        .spacing(4)
        .height(Length::Fill)
        .align_y(Alignment::Center)
        .push(slot(leading))
        .push(text(label).width(Length::Fill))
        .push(trailing.map(|handle| tinted_by(handle, ICON_SIZE, enabled)))
        .into()
}

fn tinted<'a, Message: 'a>(handle: icon::Handle, size: f32) -> Element<'a, Message> {
    tinted_by(handle, size, true)
}

fn tinted_by<'a, Message: 'a>(
    handle: icon::Handle,
    size: f32,
    enabled: bool,
) -> Element<'a, Message> {
    icon::view(handle, size, move |theme| item_text_color(theme, enabled))
}

fn item_text_color(theme: &Theme, enabled: bool) -> iced::Color {
    let palette = theme.extended_palette();
    if enabled {
        palette.background.base.text
    } else {
        palette.background.strongest.color
    }
}

fn menu_item(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => Some(palette.background.strong.color),
        button::Status::Active | button::Status::Disabled => None,
    };
    button::Style {
        background: background.map(Background::Color),
        text_color: item_text_color(theme, status != button::Status::Disabled),
        border: border::rounded(4),
        ..button::Style::default()
    }
}
