//! The connection manager: saved connections as cards in a grid, filtered
//! by the group chosen in the sidebar. A click on a card connects; a right
//! click opens a menu to edit, group, export or delete it.
//!
//! The sessions themselves belong to the application
//! ([`crate::persistence::sessions::Sessions`]); this keeps only what the
//! view needs on top: the chosen group, the open menu or dialog, and the
//! cards' thumbnails.

use std::collections::HashMap;
use std::path::Path;

use iced::widget::{
    Column, Grid, Row, button, container, grid, image, mouse_area, scrollable, text,
};
use iced::{Alignment, ContentFit, Element, Length, Theme};
use server_remote_icons as icons;

use crate::i18n::fl;
use crate::persistence::sessions::{SavedSession, Sessions, thumbnail};

use super::connect::{self, Request};
use super::menu::{self, Item};
use super::popover::{Placement, popover};
use super::{dialog, icon, style};

const SIDEBAR_WIDTH: f32 = 200.0;
/// Cards are at most this wide; the grid adds a column when they'd be wider.
const CARD_MAX_WIDTH: f32 = 240.0;
/// The card's 1pt border and the 4pt inset of its thumbnail.
const CARD_PADDING: f32 = 5.0;
/// The protocol icon standing in for a missing thumbnail.
const PLACEHOLDER_SIZE: f32 = 64.0;
const DETAIL_SIZE: f32 = 12.0;
const TOOLBAR_ICON_SIZE: f32 = 20.0;

// Not `Debug`: the editor holds a password.
#[derive(Default)]
pub struct State {
    /// The group shown, or `None` for all of them.
    group: Option<String>,
    /// The session whose context menu is open.
    menu: Option<u64>,
    /// Whether that menu's Group submenu is open.
    group_menu: bool,
    modal: Option<Modal>,
    /// Decoded once, since a raster handle made in `view` never draws.
    thumbnails: HashMap<u64, image::Handle>,
}

enum Modal {
    /// Adding a session (`editing: None`) or editing one.
    Editor {
        editing: Option<u64>,
        form: connect::State,
    },
    ConfirmDelete(u64),
    /// Naming a new group to move a session into.
    NewGroup {
        id: u64,
        name: String,
    },
}

#[derive(Debug, Clone)]
pub enum Message {
    Internal(IMessage),
    /// Bubbles up to open a connection to a saved session.
    Connect(u64),
    /// Bubbles up to save the editor's session: a new one, or over `editing`.
    Save {
        editing: Option<u64>,
        request: Request,
    },
    /// Bubbles up to move a session into a group (empty for none).
    SetGroup {
        id: u64,
        group: String,
    },
    /// Bubbles up to delete a saved session, once confirmed.
    Delete(u64),
    /// Bubbles up to ask where to export a session to.
    Export(u64),
    /// Bubbles up to ask which file to import sessions from.
    Import,
}

#[derive(Debug, Clone)]
pub enum IMessage {
    SelectGroup(Option<String>),
    OpenMenu(u64),
    CloseMenu,
    OpenGroupMenu,
    AskNewGroup(u64),
    GroupName(String),
    Add,
    Edit(u64),
    Form(connect::IMessage),
    AskDelete(u64),
    CloseModal,
}

impl State {
    /// Loads the thumbnails of `sessions` from `dir`.
    pub fn load_thumbnails(&mut self, dir: Option<&Path>, sessions: &Sessions) {
        let Some(dir) = dir else {
            return;
        };
        for session in &sessions.saved {
            self.set_thumbnail(session.id, thumbnail::load(dir, session.id));
        }
    }

    /// Shows `png` on the session's card, or its protocol icon for `None`.
    pub fn set_thumbnail(&mut self, id: u64, png: Option<Vec<u8>>) {
        match png {
            Some(png) => {
                self.thumbnails.insert(id, image::Handle::from_bytes(png));
            }
            None => {
                self.thumbnails.remove(&id);
            }
        }
    }

    /// Closes any menu or dialog, once its action has been taken.
    pub fn dismiss(&mut self) {
        self.menu = None;
        self.group_menu = false;
        self.modal = None;
    }

    pub fn update(&mut self, message: IMessage, sessions: &Sessions) {
        match message {
            IMessage::SelectGroup(group) => self.group = group,
            IMessage::OpenMenu(id) => {
                self.menu = Some(id);
                self.group_menu = false;
            }
            IMessage::CloseMenu => {
                self.menu = None;
                self.group_menu = false;
            }
            IMessage::OpenGroupMenu => self.group_menu = true,
            IMessage::AskNewGroup(id) => {
                self.dismiss();
                self.modal = Some(Modal::NewGroup {
                    id,
                    name: String::new(),
                });
            }
            IMessage::GroupName(new) => {
                if let Some(Modal::NewGroup { name, .. }) = &mut self.modal {
                    *name = new;
                }
            }
            IMessage::Add => {
                self.dismiss();
                self.modal = Some(Modal::Editor {
                    editing: None,
                    form: connect::State::in_group(self.shown_group(sessions)),
                });
            }
            IMessage::Edit(id) => {
                self.dismiss();
                if let Some(session) = sessions.get(id) {
                    self.modal = Some(Modal::Editor {
                        editing: Some(id),
                        form: connect::State::from_session(session),
                    });
                }
            }
            IMessage::Form(message) => {
                if let Some(Modal::Editor { form, .. }) = &mut self.modal {
                    let _ = form.update(message);
                }
            }
            IMessage::AskDelete(id) => {
                self.dismiss();
                self.modal = Some(Modal::ConfirmDelete(id));
            }
            IMessage::CloseModal => self.modal = None,
        }
    }

    /// The chosen group, unless its last session has gone since.
    fn shown_group<'a>(&'a self, sessions: &Sessions) -> Option<&'a str> {
        self.group
            .as_deref()
            .filter(|group| sessions.saved.iter().any(|s| s.group == *group))
    }
}

pub fn view<'a>(state: &'a State, sessions: &'a Sessions) -> Element<'a, Message> {
    let shown = state.shown_group(sessions);

    let group_item = |label: String, group: Option<&str>| -> Element<'a, Message> {
        let active = shown == group;
        button(container(text(label)).center_y(Length::Fill))
            .width(Length::Fill)
            .height(style::CONTROL_HEIGHT)
            .padding([0, 8])
            .style(move |theme, status| style::sidebar_item(theme, status, active))
            .on_press(Message::Internal(IMessage::SelectGroup(
                group.map(str::to_owned),
            )))
            .into()
    };
    let sidebar = Column::new()
        .spacing(style::SPACING)
        .push(group_item(
            fl!("manager-all", count = sessions.saved.len()),
            None,
        ))
        .extend(sessions.groups().into_iter().map(|(group, count)| {
            group_item(
                fl!("manager-group", group = group, count = count),
                Some(group),
            )
        }));

    let toolbar = Row::new()
        .spacing(style::SPACING)
        .push(toolbar_button(
            Some(icons::ADD),
            fl!("manager-add"),
            Message::Internal(IMessage::Add),
        ))
        .push(toolbar_button(None, fl!("manager-import"), Message::Import));

    let cards: Vec<_> = sessions
        .saved
        .iter()
        .filter(|session| shown.is_none_or(|group| session.group == group))
        .map(|session| card(state, sessions, session))
        .collect();
    let content: Element<'a, Message> = if cards.is_empty() {
        container(text(fl!("manager-empty")))
            .center(Length::Fill)
            .into()
    } else {
        scrollable(
            Grid::with_children(cards)
                .fluid(CARD_MAX_WIDTH)
                .spacing(style::SPACING)
                .height(Length::Shrink),
        )
        .height(Length::Fill)
        .into()
    };

    Row::new()
        .spacing(style::SPACING)
        .push(scrollable(sidebar).width(SIDEBAR_WIDTH))
        .push(
            container(
                Column::new()
                    .spacing(style::SPACING)
                    .push(toolbar)
                    .push(content),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(style::SPACING)
            .style(style::panel),
        )
        .into()
}

/// The editor or delete confirmation, to be layered over the whole window.
pub fn modal<'a>(state: &'a State, sessions: &'a Sessions) -> Option<Element<'a, Message>> {
    let cancel = || dialog::Button {
        label: fl!("button-cancel"),
        on_press: Message::Internal(IMessage::CloseModal),
        kind: dialog::Kind::Secondary,
        icon: None,
        enabled: true,
    };
    match state.modal.as_ref()? {
        Modal::Editor { editing, form } => {
            let title = match editing {
                Some(_) => fl!("manager-edit-title"),
                None => fl!("manager-add-title"),
            };
            let save = dialog::Button {
                label: fl!("connect-save"),
                on_press: Message::Save {
                    editing: *editing,
                    request: form.request(),
                },
                kind: dialog::Kind::Primary,
                icon: None,
                enabled: true,
            };
            let submit = save.on_press.clone();
            Some(dialog::view_content(
                title,
                form.fields(dialog::WIDTH - 2.0 * dialog::PADDING, true)
                    .map(move |message| match message {
                        connect::IMessage::Submit => submit.clone(),
                        message => Message::Internal(IMessage::Form(message)),
                    }),
                Vec::new(),
                vec![cancel(), save],
                dialog::WIDTH,
            ))
        }
        Modal::ConfirmDelete(id) => {
            let session = sessions.get(*id)?;
            let delete = dialog::Button {
                label: fl!("manager-delete-confirm"),
                on_press: Message::Delete(*id),
                kind: dialog::Kind::Destructive,
                icon: None,
                enabled: true,
            };
            Some(dialog::view(
                fl!("manager-delete-title", name = session.name.as_str()),
                fl!("manager-delete-message"),
                vec![cancel(), delete],
            ))
        }
        Modal::NewGroup { id, name } => {
            let create = dialog::Button {
                label: fl!("manager-new-group-create"),
                on_press: Message::SetGroup {
                    id: *id,
                    group: name.trim().to_owned(),
                },
                kind: dialog::Kind::Primary,
                icon: None,
                enabled: !name.trim().is_empty(),
            };
            let field = connect::field(&fl!("manager-group-name"), name)
                .on_input(|name| Message::Internal(IMessage::GroupName(name)))
                .on_submit_maybe(create.enabled.then(|| create.on_press.clone()));
            Some(dialog::view_content(
                fl!("manager-new-group-title"),
                field,
                Vec::new(),
                vec![cancel(), create],
                dialog::WIDTH,
            ))
        }
    }
}

fn toolbar_button<'a>(
    icon: Option<icons::Icon>,
    label: String,
    on_press: Message,
) -> Element<'a, Message> {
    // The design's plus is drawn larger than the usual 16pt icon; its box
    // then needs less room either side to keep the label where it was.
    let content = Row::new()
        .spacing(2)
        .align_y(Alignment::Center)
        .push(icon.map(|icon| icon::view(icon.into(), TOOLBAR_ICON_SIZE, text_color)))
        .push(text(label));
    let left = if icon.is_some() { 3.0 } else { 8.0 };
    button(container(content).center_y(Length::Fill))
        .height(style::CONTROL_HEIGHT)
        .padding(iced::Padding {
            top: 0.0,
            right: 8.0,
            bottom: 0.0,
            left,
        })
        .style(style::inset_button)
        .on_press(on_press)
        .into()
}

fn card<'a>(
    state: &'a State,
    sessions: &'a Sessions,
    session: &'a SavedSession,
) -> Element<'a, Message> {
    let id = session.id;
    let details = Column::new()
        .push(
            text(session.name.as_str())
                .font(style::BOLD)
                .wrapping(text::Wrapping::None),
        )
        .push(
            Column::new()
                .spacing(2.5)
                .push(detail(session.hostname.as_str()))
                .push(detail(session.protocol.full_name())),
        )
        .spacing(3)
        .padding(iced::Padding {
            top: 4.5,
            right: 4.0,
            bottom: 2.5,
            left: 4.0,
        })
        .clip(true);

    let card = button(
        Column::new()
            .push(thumbnail_view(state, session))
            .push(details),
    )
    .width(Length::Fill)
    .padding(CARD_PADDING)
    .style(style::card)
    .on_press(Message::Connect(id));
    let card = mouse_area(card).on_right_press(Message::Internal(IMessage::OpenMenu(id)));

    let menu = (state.menu == Some(id)).then(|| {
        let set_group = |group: &str| Message::SetGroup {
            id,
            group: group.to_owned(),
        };
        let groups = std::iter::once(
            Item::entry(fl!("manager-group-none"), Some(set_group("")))
                .checked(session.group.is_empty()),
        )
        .chain(std::iter::once(Item::Separator))
        .chain(sessions.groups().into_iter().map(|(group, _)| {
            Item::entry(group, Some(set_group(group))).checked(session.group == group)
        }))
        .chain([
            Item::Separator,
            Item::entry(
                fl!("manager-new-group"),
                Some(Message::Internal(IMessage::AskNewGroup(id))),
            ),
        ])
        .collect();
        menu::view(vec![
            Item::entry(fl!("connect-connect"), Some(Message::Connect(id))),
            Item::Separator,
            Item::entry(
                fl!("manager-edit"),
                Some(Message::Internal(IMessage::Edit(id))),
            ),
            Item::submenu(
                fl!("manager-group-menu"),
                groups,
                state.group_menu,
                Some(Message::Internal(IMessage::OpenGroupMenu)),
            ),
            Item::Separator,
            Item::entry(fl!("manager-export"), Some(Message::Export(id))),
            Item::entry(
                fl!("manager-delete"),
                Some(Message::Internal(IMessage::AskDelete(id))),
            ),
        ])
    });
    popover(
        card,
        menu,
        Placement::Cursor,
        Some(Message::Internal(IMessage::CloseMenu)),
    )
    .into()
}

/// The session's last screen, or its protocol's icon if there isn't one,
/// always 16:9.
fn thumbnail_view<'a>(state: &'a State, session: &'a SavedSession) -> Element<'a, Message> {
    let content: Element<'a, Message> = match state.thumbnails.get(&session.id) {
        Some(handle) => image(handle.clone())
            .width(Length::Fill)
            .height(Length::Fill)
            .content_fit(ContentFit::Cover)
            .border_radius(4)
            .into(),
        // Halfway between the text and the fill: an SVG's tint ignores
        // alpha, so it's mixed here.
        None => container(icon::view(
            session.protocol.icon().into(),
            PLACEHOLDER_SIZE,
            |theme| {
                let text = text_color(theme);
                let fill = theme.extended_palette().background.neutral.color;
                iced::Color::from_rgb(
                    (text.r + fill.r) / 2.0,
                    (text.g + fill.g) / 2.0,
                    (text.b + fill.b) / 2.0,
                )
            },
        ))
        .center(Length::Fill)
        .style(style::thumbnail_placeholder)
        .into(),
    };
    // A one-cell grid is the simplest way to size a box by its width.
    Grid::new()
        .columns(1)
        .height(grid::aspect_ratio(16, 9))
        .push(content)
        .into()
}

fn detail<'a>(content: &'a str) -> Element<'a, Message> {
    text(content)
        .size(DETAIL_SIZE)
        .wrapping(text::Wrapping::None)
        .style(|theme: &Theme| text::Style {
            color: Some(text_color(theme).scale_alpha(0.8)),
        })
        .into()
}

fn text_color(theme: &Theme) -> iced::Color {
    theme.extended_palette().background.base.text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::connection::Protocol;

    fn sessions(groups: &[&str]) -> (Sessions, Vec<u64>) {
        let mut sessions = Sessions::default();
        let ids = groups
            .iter()
            .enumerate()
            .map(|(index, group)| {
                sessions.add(SavedSession {
                    id: 0,
                    name: format!("session {index}"),
                    group: (*group).to_owned(),
                    protocol: Protocol::Vnc,
                    hostname: "example.com".to_owned(),
                    username: String::new(),
                    password: "hunter2".to_owned(),
                })
            })
            .collect();
        (sessions, ids)
    }

    #[test]
    fn view_renders_without_panicking_when_empty() {
        let state = State::default();
        let _ = view(&state, &Sessions::default());
    }

    #[test]
    fn view_renders_without_panicking_with_sessions() {
        let (sessions, ids) = sessions(&["Lab", ""]);
        let mut state = State::default();
        state.update(IMessage::OpenMenu(ids[0]), &sessions);
        let _ = view(&state, &sessions);
    }

    #[test]
    fn a_group_with_no_sessions_left_shows_all() {
        let (sessions, _) = sessions(&["Lab"]);
        let mut state = State::default();
        state.update(IMessage::SelectGroup(Some("Gone".to_owned())), &sessions);
        assert_eq!(state.shown_group(&sessions), None);

        state.update(IMessage::SelectGroup(Some("Lab".to_owned())), &sessions);
        assert_eq!(state.shown_group(&sessions), Some("Lab"));
    }

    #[test]
    fn adding_starts_in_the_shown_group() {
        let (sessions, _) = sessions(&["Lab"]);
        let mut state = State::default();
        state.update(IMessage::SelectGroup(Some("Lab".to_owned())), &sessions);
        state.update(IMessage::Add, &sessions);

        let Some(Modal::Editor { editing, form }) = &state.modal else {
            panic!("the editor is open");
        };
        assert_eq!(*editing, None);
        assert_eq!(form.request().group, "Lab");
    }

    #[test]
    fn editing_fills_in_the_session() {
        let (sessions, ids) = sessions(&["Lab", ""]);
        let mut state = State::default();
        state.update(IMessage::OpenMenu(ids[1]), &sessions);
        state.update(IMessage::Edit(ids[1]), &sessions);

        assert_eq!(state.menu, None);
        let Some(Modal::Editor { editing, form }) = &state.modal else {
            panic!("the editor is open");
        };
        assert_eq!(*editing, Some(ids[1]));
        assert_eq!(form.request().name, "session 1");
        assert!(modal(&state, &sessions).is_some());
    }

    #[test]
    fn new_group_closes_the_menu_and_asks_for_a_name() {
        let (sessions, ids) = sessions(&[""]);
        let mut state = State::default();
        state.update(IMessage::OpenMenu(ids[0]), &sessions);
        state.update(IMessage::OpenGroupMenu, &sessions);
        let _ = view(&state, &sessions);

        state.update(IMessage::AskNewGroup(ids[0]), &sessions);
        state.update(IMessage::GroupName("Lab".to_owned()), &sessions);

        assert_eq!(state.menu, None);
        assert!(!state.group_menu);
        assert!(
            matches!(&state.modal, Some(Modal::NewGroup { id, name }) if *id == ids[0] && name == "Lab")
        );
        assert!(modal(&state, &sessions).is_some());
    }

    #[test]
    fn delete_asks_first() {
        let (sessions, ids) = sessions(&[""]);
        let mut state = State::default();
        state.update(IMessage::AskDelete(ids[0]), &sessions);
        assert!(matches!(state.modal, Some(Modal::ConfirmDelete(id)) if id == ids[0]));

        state.update(IMessage::CloseModal, &sessions);
        assert!(state.modal.is_none());
    }
}
