//! Renders the UI a connector contributes — toolbar menus, links, chips,
//! status bar items — from its declarative [`Capabilities`] and [`Status`].

use std::borrow::Cow;

use iced::widget::{Row, button, container, text, tooltip};
use iced::{Alignment, Element, Length};
use server_remote_core::{
    Action, Capabilities, Chip, IndicatorState, MenuItem, Status, StatusItem, ToolbarItem,
};
use server_remote_icons as icons;

use super::connection::{Message, OpenMenu, Submenu, ToolbarMenu};
use super::style::{self, ICON_SIZE};
use super::{icon, menu};
use crate::tooltip_box::tooltip_box;

const CHIP_WIDTH: f32 = 150.0;
/// Characters of a chip label shown before it's cut short with an ellipsis.
const CHIP_LABEL_CHARS: usize = 16;

/// A menu entry that invokes `action`.
pub fn action_item(action: &Action, cache: &icon::Cache) -> menu::Item<Message> {
    let item = menu::Item::entry(
        action.label.to_string(),
        action.enabled.then(|| Message::Invoke(action.clone())),
    )
    .checked(action.checked);
    match &action.icon {
        Some(icon) => item.icon(cache.handle(icon)),
        None => item,
    }
}

/// The dropdown menus a connector adds to the toolbar.
pub fn toolbar_menus<'a>(
    capabilities: &'a Capabilities,
    cache: &icon::Cache,
    open: Option<&OpenMenu>,
) -> Vec<Element<'a, Message>> {
    capabilities
        .toolbar
        .iter()
        .filter_map(|item| match item {
            ToolbarItem::Menu {
                id,
                icon,
                label: _,
                items,
            } => {
                let menu = ToolbarMenu::Contributed(id.clone());
                let open_submenu =
                    open.filter(|open| open.menu == menu)
                        .map(|open| match &open.submenu {
                            Some(Submenu::Contributed(path)) => path.as_slice(),
                            // The built-in menus' submenus.
                            Some(_) | None => &[],
                        });
                Some(menu::button_menu(
                    cache.handle(icon),
                    menu_items(items, cache, open_submenu, &[]),
                    open_submenu.is_some(),
                    Message::ToggleMenu(menu),
                    Message::CloseMenu,
                ))
            }
            ToolbarItem::Link { .. } => None,
        })
        .collect()
}

/// Converts a connector's menu tree. `open` is the path of the open submenu
/// within it, if any; `path` is where `items` sit in the tree.
fn menu_items(
    items: &[MenuItem],
    cache: &icon::Cache,
    open: Option<&[usize]>,
    path: &[usize],
) -> Vec<menu::Item<Message>> {
    items
        .iter()
        .enumerate()
        .map(|(index, item)| match item {
            MenuItem::Action(action) => action_item(action, cache),
            MenuItem::Separator => menu::Item::Separator,
            MenuItem::Submenu { label, items } => {
                let child: Vec<usize> = path.iter().copied().chain([index]).collect();
                let is_open = open.is_some_and(|open| open.starts_with(&child));
                menu::Item::submenu(
                    label.to_string(),
                    menu_items(items, cache, open, &child),
                    is_open,
                    Some(Message::OpenSubmenu(Submenu::Contributed(child))),
                )
            }
        })
        .collect()
}

/// The link buttons a connector adds to the toolbar's right-hand side.
pub fn toolbar_links<'a>(
    capabilities: &'a Capabilities,
    cache: &icon::Cache,
) -> Vec<Element<'a, Message>> {
    capabilities
        .toolbar
        .iter()
        .filter_map(|item| match item {
            ToolbarItem::Link { icon, label, url } => Some(
                tooltip(
                    button(container(icon_view(cache.handle(icon))).center(Length::Fill))
                        .width(style::CONTROL_HEIGHT)
                        .height(style::CONTROL_HEIGHT)
                        .padding(0)
                        .style(|theme, status| style::bordered_button(theme, status, false))
                        .on_press(Message::OpenUrl(url.clone())),
                    tooltip_box(text(label.as_ref())),
                    tooltip::Position::Bottom,
                )
                .into(),
            ),
            ToolbarItem::Menu { .. } => None,
        })
        .collect()
}

pub fn chips<'a>(status: &'a Status, cache: &icon::Cache) -> Vec<Element<'a, Message>> {
    status
        .chips
        .iter()
        .map(|chip| chip_view(chip, cache))
        .collect()
}

fn chip_view<'a>(chip: &'a Chip, cache: &icon::Cache) -> Element<'a, Message> {
    let close = chip.close_action.as_ref().map(|id| {
        button(container(icon_view(icons::CLOSE.into())).center(Length::Fill))
            .width(18)
            .height(18)
            .padding(0)
            .style(|theme, status| style::tab_close_button(theme, status, true))
            .on_press(Message::SendAction(id.clone()))
    });

    container(
        Row::new()
            .spacing(4)
            .height(Length::Fill)
            .align_y(Alignment::Center)
            .push(chip.icon.as_ref().map(|icon| icon_view(cache.handle(icon))))
            .push(
                text(truncate(&chip.label, CHIP_LABEL_CHARS))
                    .width(Length::Fill)
                    .wrapping(text::Wrapping::None),
            )
            .push(close),
    )
    .width(CHIP_WIDTH)
    .height(style::CONTROL_HEIGHT)
    .padding([0, 5])
    .style(style::chip)
    .into()
}

/// The connector's own status bar entries.
pub fn status_items<'a>(status: &'a Status, cache: &icon::Cache) -> Vec<Element<'a, Message>> {
    status
        .items
        .iter()
        .map(|item| match item {
            StatusItem::Indicator { label, state } => Row::new()
                .spacing(6)
                .align_y(Alignment::Center)
                .push(style::status_dot(indicator_color(*state)))
                .push(text(label.as_ref()))
                .into(),
            StatusItem::Text { icon, text: label } => Row::new()
                .spacing(6)
                .align_y(Alignment::Center)
                .push(icon.as_ref().map(|icon| icon_view(cache.handle(icon))))
                .push(text(label.as_ref()))
                .into(),
        })
        .collect()
}

fn indicator_color(state: IndicatorState) -> style::Indicator {
    match state {
        IndicatorState::Ok => style::Indicator::Ok,
        IndicatorState::Warning => style::Indicator::Warning,
        IndicatorState::Error => style::Indicator::Error,
        IndicatorState::Inactive => style::Indicator::Inactive,
    }
}

fn icon_view<'a>(handle: icon::Handle) -> Element<'a, Message> {
    icon::view(handle, ICON_SIZE, |theme| {
        theme.extended_palette().background.base.text
    })
}

/// `label`, cut to `max_chars` characters with an ellipsis if it's longer.
fn truncate(label: &str, max_chars: usize) -> Cow<'_, str> {
    if label.chars().count() <= max_chars {
        return Cow::Borrowed(label);
    }
    let kept: String = label.chars().take(max_chars.saturating_sub(1)).collect();
    Cow::Owned(format!("{kept}…"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncation() {
        let cases = [
            ("short label is kept", "disk.iso", 16, "disk.iso"),
            ("exact fit is kept", "abcd", 4, "abcd"),
            (
                "long label is cut",
                "proxmox-ve_9.2-1.iso",
                16,
                "proxmox-ve_9.2-…",
            ),
            ("counts characters, not bytes", "ŵŵŵŵŵ", 4, "ŵŵŵ…"),
        ];
        for (name, label, max, expected) in cases {
            assert_eq!(truncate(label, max), expected, "{name}");
        }
    }
}
