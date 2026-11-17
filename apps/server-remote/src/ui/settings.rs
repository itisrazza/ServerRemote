//! The Settings tab: a list of sections on the left and the chosen section's
//! options on the right. Every change is a [`Change`], applied by
//! [`apply`] and saved straight away by the application.
//!
//! The connection menus offer some of the same settings, so the choice lists
//! here ([`recording_folders`] and friends) are shared with them.

use std::path::{Path, PathBuf};

use iced::widget::{Column, Row, button, container, responsive, scrollable, text, toggler};
use iced::{Alignment, Element, Length, Theme};

use crate::persistence::settings::{
    DisplayScale, GraphicsBackend, KeyboardLayout, RecordingFormat, RecordingQuality, Scaling,
    Settings,
};

use super::connection::KeyboardSize;
use super::menu::{self, Item};
use super::style;

const SIDEBAR_WIDTH: f32 = 180.0;
const SIDEBAR_GAP: f32 = 24.0;
/// The widest the settings column grows, so labels stay near their controls.
const CONTENT_WIDTH: f32 = 560.0;
/// Below this, each control drops beneath its description instead of
/// sitting beside it.
const SIDE_BY_SIDE_WIDTH: f32 = 480.0;
/// Keeps controls off the window's right edge when the column fills it.
const CONTENT_END_PADDING: f32 = 12.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Section {
    #[default]
    General,
    Display,
    Input,
    Recording,
}

impl Section {
    const ALL: [Section; 4] = [
        Section::General,
        Section::Display,
        Section::Input,
        Section::Recording,
    ];

    fn label(self) -> &'static str {
        match self {
            Section::General => "General",
            Section::Display => "Display",
            Section::Input => "Input",
            Section::Recording => "Recording",
        }
    }
}

/// The dropdowns on the page, so one can be open at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Select {
    GraphicsBackend,
    Scaling,
    DisplayScale,
    KeyboardStyle,
    KeyboardLayout,
    RecordingFolder,
    RecordingFormat,
    RecordingQuality,
}

/// A change to one setting.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    ConfirmQuit(bool),
    GraphicsBackend(GraphicsBackend),
    Scaling(Scaling),
    KeepAspectRatio(bool),
    DisplayScale(DisplayScale),
    KeyboardStyle(KeyboardSize),
    OpenKeyboardOnConnect(bool),
    KeyboardLayout(KeyboardLayout),
    /// Save recordings to the system's videos folder.
    DefaultRecordingFolder,
    /// Save recordings to this folder, remembering it among the recent ones.
    RecordingFolder(PathBuf),
    RecordingFormat(RecordingFormat),
    RecordingQuality(RecordingQuality),
}

pub fn apply(settings: &mut Settings, change: Change) {
    match change {
        Change::ConfirmQuit(on) => settings.general.confirm_quit = on,
        Change::GraphicsBackend(backend) => settings.general.graphics_backend = backend,
        Change::Scaling(scaling) => settings.display.scaling = scaling,
        Change::KeepAspectRatio(on) => settings.display.keep_aspect_ratio = on,
        Change::DisplayScale(scale) => settings.display.scale = scale,
        Change::KeyboardStyle(style) => settings.input.keyboard_style = style,
        Change::OpenKeyboardOnConnect(on) => settings.input.open_keyboard_on_connect = on,
        Change::KeyboardLayout(layout) => settings.input.keyboard_layout = layout,
        Change::DefaultRecordingFolder => settings.recording.folder = None,
        Change::RecordingFolder(folder) => settings.recording.choose_folder(folder),
        Change::RecordingFormat(format) => settings.recording.format = format,
        Change::RecordingQuality(quality) => settings.recording.quality = quality,
    }
}

#[derive(Debug, Default)]
pub struct State {
    section: Section,
    open: Option<Select>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Section(Section),
    Toggle(Select),
    Dismiss,
    /// Handled by the application, which owns and saves the settings.
    Change(Change),
    /// Handled by the application, which opens the folder picker.
    ChooseRecordingFolder,
}

impl State {
    pub fn open_section(&mut self, section: Section) {
        self.section = section;
        self.open = None;
    }

    pub fn update(&mut self, message: &Message) {
        match message {
            Message::Section(section) => self.open_section(*section),
            Message::Toggle(select) => {
                self.open = (self.open != Some(*select)).then_some(*select);
            }
            Message::Dismiss | Message::Change(_) | Message::ChooseRecordingFolder => {
                self.open = None;
            }
        }
    }
}

pub fn view<'a>(state: &'a State, settings: &'a Settings) -> Element<'a, Message> {
    let sidebar = Column::with_children(Section::ALL.into_iter().map(|section| {
        let active = state.section == section;
        button(container(text(section.label())).center_y(Length::Fill))
            .width(Length::Fill)
            .height(style::CONTROL_HEIGHT)
            .padding([0, 10])
            .style(move |theme, status| style::sidebar_item(theme, status, active))
            .on_press(Message::Section(section))
            .into()
    }))
    .spacing(2)
    .width(SIDEBAR_WIDTH);

    // Laid out against the space actually available, so a narrow window
    // stacks each control under its description rather than squashing it.
    let content = responsive(move |size| {
        let width = (size.width - CONTENT_END_PADDING).min(CONTENT_WIDTH);
        let side_by_side = width >= SIDE_BY_SIDE_WIDTH;
        let settings = match state.section {
            Section::General => general(state, settings),
            Section::Display => display(state, settings),
            Section::Input => input(state, settings),
            Section::Recording => recording(state, settings),
        };
        let column = Column::new()
            .spacing(16)
            .width(width)
            .push(text(state.section.label()).size(16).font(style::BOLD))
            .push(
                Column::with_children(
                    settings
                        .into_iter()
                        .map(|setting| setting.view(side_by_side)),
                )
                .spacing(16),
            );
        // A vertical scrollable stretches its content to its own width, so
        // the column sits in a full-width container instead.
        scrollable(
            container(column)
                .width(Length::Fill)
                .padding(iced::padding::right(CONTENT_END_PADDING)),
        )
        .width(Length::Fill)
        .into()
    });

    Row::new()
        .spacing(SIDEBAR_GAP)
        .padding([8, 0])
        .push(sidebar)
        .push(content)
        .into()
}

fn general<'a>(state: &'a State, settings: &'a Settings) -> Vec<Setting<'a>> {
    let confirm_quit = setting(
        "Confirm before quitting",
        "Ask before closing the window while connections are open.",
        switch(settings.general.confirm_quit, Change::ConfirmQuit),
    );
    // Only Windows offers a choice of graphics API worth making.
    let backend = cfg!(windows).then(|| {
        setting(
            "Graphics backend",
            "DirectX 12 keeps up with window resizing better; try Vulkan if \
             drawing misbehaves. Takes effect when the app is next opened.",
            select(
                state,
                Select::GraphicsBackend,
                settings.general.graphics_backend.label(),
                menu::choices(
                    &GraphicsBackend::ALL,
                    settings.general.graphics_backend,
                    GraphicsBackend::label,
                    |backend| Message::Change(Change::GraphicsBackend(backend)),
                ),
            ),
        )
    });
    [Some(confirm_quit), backend]
        .into_iter()
        .flatten()
        .collect()
}

fn display<'a>(state: &'a State, settings: &'a Settings) -> Vec<Setting<'a>> {
    vec![
        setting(
            "Scaling",
            "How the remote display fits the window.",
            select(
                state,
                Select::Scaling,
                settings.display.scaling.label(),
                menu::choices(
                    &Scaling::ALL,
                    settings.display.scaling,
                    Scaling::label,
                    |scaling| Message::Change(Change::Scaling(scaling)),
                ),
            ),
        ),
        setting(
            "Keep aspect ratio",
            "Letterbox the remote display rather than stretch it.",
            switch(settings.display.keep_aspect_ratio, Change::KeepAspectRatio),
        ),
        setting(
            "Display scale",
            "How remote pixels map to this screen's.",
            select(
                state,
                Select::DisplayScale,
                settings.display.scale.label(),
                display_scales(settings, |scale| {
                    Message::Change(Change::DisplayScale(scale))
                }),
            ),
        ),
    ]
}

fn input<'a>(state: &'a State, settings: &'a Settings) -> Vec<Setting<'a>> {
    vec![
        setting(
            "Keyboard style",
            "The virtual keyboard's layout when a connection opens.",
            select(
                state,
                Select::KeyboardStyle,
                settings.input.keyboard_style.label(),
                menu::choices(
                    &KeyboardSize::ALL,
                    settings.input.keyboard_style,
                    KeyboardSize::label,
                    |style| Message::Change(Change::KeyboardStyle(style)),
                ),
            ),
        ),
        setting(
            "Show virtual keyboard on connect",
            "Open the virtual keyboard as soon as a connection opens.",
            switch(
                settings.input.open_keyboard_on_connect,
                Change::OpenKeyboardOnConnect,
            ),
        ),
        setting(
            "Keyboard layout",
            "The remote machine's keyboard layout.",
            select(
                state,
                Select::KeyboardLayout,
                settings.input.keyboard_layout.label(),
                keyboard_layouts(settings, |layout| {
                    Message::Change(Change::KeyboardLayout(layout))
                }),
            ),
        ),
    ]
}

fn recording<'a>(state: &'a State, settings: &'a Settings) -> Vec<Setting<'a>> {
    let folder = settings
        .recording
        .folder
        .as_deref()
        .map_or_else(default_recording_folder_label, folder_label);
    vec![
        setting(
            "Save to",
            "Where new recordings are saved.",
            select(
                state,
                Select::RecordingFolder,
                folder,
                recording_folders(settings, Message::Change, Message::ChooseRecordingFolder),
            ),
        ),
        setting(
            "Format",
            "ServerRemote recordings also keep keyboard and mouse input.",
            select(
                state,
                Select::RecordingFormat,
                settings.recording.format.label(),
                recording_formats(settings, |format| {
                    Message::Change(Change::RecordingFormat(format))
                }),
            ),
        ),
        setting(
            "Quality",
            "Higher quality makes larger files.",
            select(
                state,
                Select::RecordingQuality,
                settings.recording.quality.label(),
                recording_qualities(settings, |quality| {
                    Message::Change(Change::RecordingQuality(quality))
                }),
            ),
        ),
    ]
}

pub fn display_scales<Message>(
    settings: &Settings,
    on_choose: impl Fn(DisplayScale) -> Message,
) -> Vec<Item<Message>> {
    menu::choices(
        &DisplayScale::ALL,
        settings.display.scale,
        DisplayScale::label,
        on_choose,
    )
}

pub fn keyboard_layouts<Message>(
    settings: &Settings,
    on_choose: impl Fn(KeyboardLayout) -> Message,
) -> Vec<Item<Message>> {
    menu::choices(
        &KeyboardLayout::ALL,
        settings.input.keyboard_layout,
        KeyboardLayout::label,
        on_choose,
    )
}

pub fn recording_formats<Message>(
    settings: &Settings,
    on_choose: impl Fn(RecordingFormat) -> Message,
) -> Vec<Item<Message>> {
    menu::choices(
        &RecordingFormat::ALL,
        settings.recording.format,
        RecordingFormat::label,
        on_choose,
    )
}

pub fn recording_qualities<Message>(
    settings: &Settings,
    on_choose: impl Fn(RecordingQuality) -> Message,
) -> Vec<Item<Message>> {
    menu::choices(
        &RecordingQuality::ALL,
        settings.recording.quality,
        RecordingQuality::label,
        on_choose,
    )
}

/// The system videos folder, the recently chosen ones, and "Other…".
pub fn recording_folders<Message>(
    settings: &Settings,
    on_change: impl Fn(Change) -> Message,
    on_other: Message,
) -> Vec<Item<Message>> {
    let current = settings.recording.folder.as_deref();
    let default = Item::entry(
        default_recording_folder_label(),
        Some(on_change(Change::DefaultRecordingFolder)),
    )
    .checked(current.is_none());
    let recent = settings.recording.recent_folders.iter().map(|folder| {
        Item::entry(
            folder_label(folder),
            Some(on_change(Change::RecordingFolder(folder.clone()))),
        )
        .checked(current == Some(folder.as_path()))
    });

    std::iter::once(default)
        .chain(recent)
        .chain([
            Item::Separator,
            Item::entry("Other…", Some(on_other)).icon(server_remote_icons::FOLDER),
        ])
        .collect()
}

fn default_recording_folder_label() -> String {
    directories::UserDirs::new()
        .and_then(|dirs| dirs.video_dir().map(folder_label))
        .unwrap_or_else(|| "Videos".to_owned())
}

/// `folder` with the home directory shortened to `~`.
fn folder_label(folder: &Path) -> String {
    let home = directories::UserDirs::new().map(|dirs| dirs.home_dir().to_path_buf());
    abbreviate_home(folder, home.as_deref())
}

fn abbreviate_home(folder: &Path, home: Option<&Path>) -> String {
    match home.and_then(|home| folder.strip_prefix(home).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Some(rest) => Path::new("~").join(rest).display().to_string(),
        None => folder.display().to_string(),
    }
}

/// One setting: its name, an explanation, and the control that changes it.
struct Setting<'a> {
    label: &'a str,
    description: &'a str,
    control: Control<'a>,
}

/// How a setting's control sits beside its label.
enum Control<'a> {
    /// A dropdown or button: beside the label when there's room, else below.
    Block(Element<'a, Message>),
    /// A switch: always on the setting's own line, at the right-hand end.
    Switch(Element<'a, Message>),
}

impl<'a> From<Element<'a, Message>> for Control<'a> {
    fn from(element: Element<'a, Message>) -> Self {
        Control::Block(element)
    }
}

fn setting<'a>(
    label: &'a str,
    description: &'a str,
    control: impl Into<Control<'a>>,
) -> Setting<'a> {
    Setting {
        label,
        description,
        control: control.into(),
    }
}

impl<'a> Setting<'a> {
    /// The label and description with the control beside them, or with a
    /// block control underneath when `side_by_side` is false (space is
    /// tight). Switches always stay beside.
    fn view(self, side_by_side: bool) -> Element<'a, Message> {
        let description = text(self.description)
            .size(11)
            .style(|theme: &Theme| text::Style {
                color: Some(theme.extended_palette().background.strongest.color),
            });
        let words = Column::new()
            .spacing(2)
            .width(Length::Fill)
            .push(text(self.label))
            .push(description);

        let (control, beside) = match self.control {
            Control::Switch(control) => (control, true),
            Control::Block(control) => (control, side_by_side),
        };
        if beside {
            Row::new()
                .spacing(16)
                .align_y(Alignment::Center)
                .push(words)
                .push(control)
                .into()
        } else {
            Column::new().spacing(8).push(words).push(control).into()
        }
    }
}

fn select<'a>(
    state: &State,
    select: Select,
    value: impl Into<String>,
    items: Vec<Item<Message>>,
) -> Element<'a, Message> {
    menu::select(
        value.into(),
        items,
        state.open == Some(select),
        Message::Toggle(select),
        Message::Dismiss,
    )
}

fn switch<'a>(on: bool, change: fn(bool) -> Change) -> Control<'a> {
    Control::Switch(
        toggler(on)
            .on_toggle(move |on| Message::Change(change(on)))
            .into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_apply_to_their_setting() {
        let folder = PathBuf::from("/recordings");
        type Check = fn(&Settings) -> bool;
        let cases: Vec<(&str, Change, Check)> = vec![
            ("confirm quit", Change::ConfirmQuit(false), |s| {
                !s.general.confirm_quit
            }),
            (
                "graphics backend",
                Change::GraphicsBackend(GraphicsBackend::Vulkan),
                |s| s.general.graphics_backend == GraphicsBackend::Vulkan,
            ),
            ("scaling", Change::Scaling(Scaling::Actual), |s| {
                s.display.scaling == Scaling::Actual
            }),
            ("aspect ratio", Change::KeepAspectRatio(false), |s| {
                !s.display.keep_aspect_ratio
            }),
            (
                "display scale",
                Change::DisplayScale(DisplayScale::Percent150),
                |s| s.display.scale == DisplayScale::Percent150,
            ),
            (
                "keyboard style",
                Change::KeyboardStyle(KeyboardSize::Large),
                |s| s.input.keyboard_style == KeyboardSize::Large,
            ),
            (
                "keyboard on connect",
                Change::OpenKeyboardOnConnect(true),
                |s| s.input.open_keyboard_on_connect,
            ),
            (
                "keyboard layout",
                Change::KeyboardLayout(KeyboardLayout::GermanQwertz),
                |s| s.input.keyboard_layout == KeyboardLayout::GermanQwertz,
            ),
            (
                "recording folder",
                Change::RecordingFolder(folder.clone()),
                |s| s.recording.folder.as_deref() == Some(Path::new("/recordings")),
            ),
            (
                "recording format",
                Change::RecordingFormat(RecordingFormat::Mkv),
                |s| s.recording.format == RecordingFormat::Mkv,
            ),
            (
                "recording quality",
                Change::RecordingQuality(RecordingQuality::Lossless),
                |s| s.recording.quality == RecordingQuality::Lossless,
            ),
        ];

        for (name, change, check) in cases {
            let mut settings = Settings::default();
            apply(&mut settings, change);
            assert!(check(&settings), "{name}");
        }
    }

    #[test]
    fn default_folder_change_keeps_the_recent_list() {
        let mut settings = Settings::default();
        apply(&mut settings, Change::RecordingFolder(PathBuf::from("/a")));
        apply(&mut settings, Change::DefaultRecordingFolder);
        assert_eq!(settings.recording.folder, None);
        assert_eq!(settings.recording.recent_folders, vec![PathBuf::from("/a")]);
    }

    #[test]
    fn home_is_abbreviated() {
        let home = Path::new("/Users/someone");
        let cases = [
            ("inside home", "/Users/someone/Movies", "~/Movies"),
            ("home itself", "/Users/someone", "~"),
            ("outside home", "/Volumes/Recordings", "/Volumes/Recordings"),
        ];
        for (name, folder, expected) in cases {
            assert_eq!(
                abbreviate_home(Path::new(folder), Some(home)),
                expected,
                "{name}"
            );
        }
        assert_eq!(abbreviate_home(Path::new("/x"), None), "/x");
    }

    #[test]
    fn one_dropdown_open_at_a_time() {
        let mut state = State::default();
        state.update(&Message::Toggle(Select::Scaling));
        assert_eq!(state.open, Some(Select::Scaling));
        state.update(&Message::Toggle(Select::DisplayScale));
        assert_eq!(state.open, Some(Select::DisplayScale));
        state.update(&Message::Toggle(Select::DisplayScale));
        assert_eq!(state.open, None);
        state.update(&Message::Toggle(Select::Scaling));
        state.update(&Message::Change(Change::Scaling(Scaling::Actual)));
        assert_eq!(state.open, None);
    }
}
