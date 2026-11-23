//! The Settings tab: a list of sections on the left and the chosen section's
//! options on the right. Every change is a [`Change`], applied by
//! [`apply`] and saved straight away by the application.
//!
//! The connection menus offer some of the same settings, so the choice lists
//! here ([`recording_folders`] and friends) are shared with them.

use std::borrow::Cow;
use std::path::Path;
use std::sync::LazyLock;

use iced::widget::{
    Column, Row, button, container, image, responsive, scrollable, svg, text, toggler,
};
use iced::{Alignment, Element, Length, Theme};
use server_remote_icons::{self as icons, Icon};

use crate::persistence::settings::{
    Appearance, CertificateChecking, Change, DisplayScale, GraphicsBackend, KeyboardLayout,
    KnownCertificate, Language, RecordingFormat, RecordingQuality, Scaling, Settings,
};

use super::menu::{self, Item};
use super::{style, themed_icon};
use crate::i18n::fl;
use crate::persistence::settings::KeyboardSize;
use crate::{contributors, update};

const SIDEBAR_WIDTH: f32 = 200.0;
/// The widest the settings column grows, so labels stay near their controls.
const CONTENT_WIDTH: f32 = 560.0;
/// The About page is narrower: it has no dropdowns to make room for.
const ABOUT_WIDTH: f32 = 464.0;
/// Below this, each control drops beneath its description instead of
/// sitting beside it.
const SIDE_BY_SIDE_WIDTH: f32 = 480.0;
/// Keeps the column off the panel's edges when it fills the panel.
const PANEL_PADDING: f32 = 16.0;

const WEBSITE_URL: &str = "https://serverremote.razza.io/";
const MIT_LICENSE_URL: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/blob/main/LICENSE-MIT");
const APACHE_LICENSE_URL: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/blob/main/LICENSE-APACHE");
const TITLE_SIZE: f32 = 24.0;
const VERSION_SIZE: f32 = 16.0;
const LOGO_SIZE: f32 = 64.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Section {
    #[default]
    General,
    Display,
    Input,
    Recording,
    Security,
    About,
}

impl Section {
    const ALL: [Section; 6] = [
        Section::General,
        Section::Display,
        Section::Input,
        Section::Recording,
        Section::Security,
        Section::About,
    ];

    fn label(self) -> String {
        match self {
            Section::General => fl!("section-general"),
            Section::Display => fl!("section-display"),
            Section::Input => fl!("section-input"),
            Section::Recording => fl!("section-recording"),
            Section::Security => fl!("section-security"),
            Section::About => fl!("section-about"),
        }
    }
}

/// The dropdowns on the page, so one can be open at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Select {
    Appearance,
    Language,
    GraphicsBackend,
    Scaling,
    DisplayScale,
    KeyboardStyle,
    KeyboardLayout,
    RecordingFolder,
    RecordingFormat,
    RecordingQuality,
    CertificateChecking,
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
    /// Handled by the application, which owns the update check.
    CheckForUpdates,
    /// Handled by the application, which opens it in the browser.
    OpenUrl(String),
    /// Handled by the application, which shows them in a dialog.
    ShowThirdPartyLicences,
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
            Message::Dismiss
            | Message::Change(_)
            | Message::ChooseRecordingFolder
            | Message::CheckForUpdates
            | Message::OpenUrl(_)
            | Message::ShowThirdPartyLicences => {
                self.open = None;
            }
        }
    }
}

pub fn view<'a>(
    state: &'a State,
    settings: &'a Settings,
    update: &'a update::Status,
) -> Element<'a, Message> {
    let sidebar = Column::with_children(Section::ALL.into_iter().map(|section| {
        let active = state.section == section;
        button(container(text(section.label())).center_y(Length::Fill))
            .width(Length::Fill)
            .height(style::CONTROL_HEIGHT)
            .padding([0, 8])
            .style(move |theme, status| style::sidebar_item(theme, status, active))
            .on_press(Message::Section(section))
            .into()
    }))
    .spacing(style::SPACING)
    .width(SIDEBAR_WIDTH);

    // Laid out against the space actually available, so a narrow window
    // stacks each control under its description rather than squashing it.
    let content = responsive(move |size| {
        let available = size.width - 2.0 * PANEL_PADDING;
        let side_by_side = available.min(CONTENT_WIDTH) >= SIDE_BY_SIDE_WIDTH;
        let (column, max_width) = match state.section {
            Section::About => (about(settings, update, side_by_side), ABOUT_WIDTH),
            _ => (section_view(state, settings, side_by_side), CONTENT_WIDTH),
        };
        // A vertical scrollable stretches its content to its own width, so
        // the column is centred in a full-width container instead.
        scrollable(
            container(container(column).width(available.min(max_width)))
                .center_x(Length::Fill)
                .padding([8.0, PANEL_PADDING]),
        )
        .width(Length::Fill)
        .into()
    });

    Row::new()
        .spacing(style::SPACING)
        .push(sidebar)
        .push(
            container(content)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(style::panel),
        )
        .into()
}

/// A section's heading over its settings.
fn section_view<'a>(
    state: &'a State,
    settings: &'a Settings,
    side_by_side: bool,
) -> Element<'a, Message> {
    let (settings, extra) = match state.section {
        Section::General => (general(state, settings), None),
        Section::Display => (display(state, settings), None),
        Section::Input => (input(state, settings), None),
        Section::Recording => (recording(state, settings), None),
        Section::Security => (
            vec![certificate_checking(state, settings)],
            Some(known_certificates(settings)),
        ),
        Section::About => (Vec::new(), None),
    };
    Column::new()
        .spacing(16)
        .push(text(state.section.label()).size(16).font(style::BOLD))
        .push(
            Column::with_children(
                settings
                    .into_iter()
                    .map(|setting| setting.view(side_by_side)),
            )
            .spacing(16),
        )
        .push(extra)
        .into()
}

fn general<'a>(state: &'a State, settings: &'a Settings) -> Vec<Setting<'a>> {
    let confirm_quit = setting(
        fl!("setting-confirm-quit"),
        fl!("setting-confirm-quit-description"),
        switch(settings.general.confirm_quit, Change::ConfirmQuit),
    );
    let appearance = setting(
        fl!("setting-appearance"),
        fl!("setting-appearance-description"),
        select(
            state,
            Select::Appearance,
            settings.general.appearance.label(),
            menu::choices(
                &Appearance::ALL,
                settings.general.appearance,
                Appearance::label,
                |appearance| Message::Change(Change::Appearance(appearance)),
            ),
        ),
    );
    let language = setting(
        fl!("setting-language"),
        "",
        select(
            state,
            Select::Language,
            settings.general.language.label(),
            menu::choices(
                &Language::ALL,
                settings.general.language,
                Language::label,
                |language| Message::Change(Change::Language(language)),
            ),
        ),
    );
    // Only Windows offers a choice of graphics API worth making.
    let backend = cfg!(windows).then(|| {
        setting(
            fl!("setting-graphics-backend"),
            fl!("setting-graphics-backend-description"),
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
    [
        Some(confirm_quit),
        Some(appearance),
        Some(language),
        backend,
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn display<'a>(state: &'a State, settings: &'a Settings) -> Vec<Setting<'a>> {
    vec![
        setting(
            fl!("setting-scaling"),
            fl!("setting-scaling-description"),
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
            fl!("setting-keep-aspect-ratio"),
            fl!("setting-keep-aspect-ratio-description"),
            switch(settings.display.keep_aspect_ratio, Change::KeepAspectRatio),
        ),
        setting(
            fl!("setting-display-scale"),
            fl!("setting-display-scale-description"),
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
            fl!("setting-keyboard-style"),
            fl!("setting-keyboard-style-description"),
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
            fl!("setting-keyboard-on-connect"),
            fl!("setting-keyboard-on-connect-description"),
            switch(
                settings.input.open_keyboard_on_connect,
                Change::OpenKeyboardOnConnect,
            ),
        ),
        setting(
            fl!("setting-keyboard-layout"),
            fl!("setting-keyboard-layout-description"),
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
            fl!("setting-recording-folder"),
            fl!("setting-recording-folder-description"),
            select(
                state,
                Select::RecordingFolder,
                folder,
                recording_folders(settings, Message::Change, Message::ChooseRecordingFolder),
            ),
        ),
        setting(
            fl!("setting-recording-format"),
            fl!("setting-recording-format-description"),
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
            fl!("setting-recording-quality"),
            fl!("setting-recording-quality-description"),
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

fn certificate_checking<'a>(state: &'a State, settings: &'a Settings) -> Setting<'a> {
    let checking = settings.security.certificate_checking;
    setting(
        fl!("setting-certificate-checking"),
        checking.description(),
        select(
            state,
            Select::CertificateChecking,
            checking.label(),
            menu::choices(
                &CertificateChecking::ALL,
                checking,
                CertificateChecking::label,
                |checking| Message::Change(Change::CertificateChecking(checking)),
            ),
        ),
    )
}

/// The trusted and blocked certificate lists.
fn known_certificates(settings: &Settings) -> Element<'_, Message> {
    Column::new()
        .spacing(16)
        .push(certificate_list(
            fl!("trusted-certificates"),
            fl!("trusted-certificates-description"),
            &settings.security.trusted,
            fl!("certificate-forget"),
            Change::ForgetCertificate,
        ))
        .push(certificate_list(
            fl!("blocked-certificates"),
            fl!("blocked-certificates-description"),
            &settings.security.blocked,
            fl!("certificate-unblock"),
            Change::UnblockCertificate,
        ))
        .into()
}

fn certificate_list<'a>(
    title: String,
    description: String,
    certificates: &'a [KnownCertificate],
    remove_label: String,
    remove: fn(KnownCertificate) -> Change,
) -> Element<'a, Message> {
    let rows: Vec<Element<'a, Message>> = if certificates.is_empty() {
        vec![muted(fl!("certificates-none")).into()]
    } else {
        certificates
            .iter()
            .map(|certificate| {
                certificate_row(
                    certificate,
                    remove_label.clone(),
                    Message::Change(remove(certificate.clone())),
                )
            })
            .collect()
    };
    Column::new()
        .spacing(8)
        .push(
            Column::new()
                .spacing(2)
                .push(text(title).size(14).font(style::BOLD))
                .push(muted(description)),
        )
        .push(Column::with_children(rows).spacing(8))
        .into()
}

/// A server and its certificate's fingerprint, with a button to remove it.
fn certificate_row(
    certificate: &KnownCertificate,
    remove_label: String,
    remove: Message,
) -> Element<'_, Message> {
    let words = Column::new()
        .spacing(2)
        .width(Length::Fill)
        .push(text(format!("{}:{}", certificate.host, certificate.port)))
        .push(muted(fingerprint_halves(
            &certificate.fingerprint.to_string(),
        )));
    Row::new()
        .spacing(16)
        .align_y(Alignment::Center)
        .push(words)
        .push(action_button(remove_label, Some(remove), false))
        .into()
}

/// A fingerprint on two lines: it has no spaces, so it wouldn't wrap.
fn fingerprint_halves(fingerprint: &str) -> String {
    let bytes: Vec<&str> = fingerprint.split(':').collect();
    let (first, second) = bytes.split_at(bytes.len() / 2);
    format!("{}\n{}", first.join(":"), second.join(":"))
}

/// Secondary text, as in a setting's description.
fn muted<'a>(content: impl text::IntoFragment<'a>) -> iced::widget::Text<'a> {
    text(content).size(12).style(|theme: &Theme| text::Style {
        color: Some(theme.extended_palette().background.strongest.color),
    })
}

fn about<'a>(
    settings: &'a Settings,
    update: &'a update::Status,
    side_by_side: bool,
) -> Element<'a, Message> {
    let check_on_launch = setting(
        fl!("setting-check-on-launch"),
        "",
        switch(
            settings.updates.check_on_launch,
            Change::CheckForUpdatesOnLaunch,
        ),
    );
    Column::new()
        .spacing(16)
        .push(container(about_header()).padding(iced::padding::bottom(4)))
        .push(made_by())
        .push(check_on_launch.view(side_by_side))
        .push(update_setting(update).view(side_by_side))
        .push(licence())
        .into()
}

/// The logo beside the app's name, version and links.
fn about_header<'a>() -> Element<'a, Message> {
    let logo = svg(svg::Handle::from_memory(crate::app_icon::LOGO))
        .width(LOGO_SIZE)
        .height(LOGO_SIZE);
    // Lines up the version's baseline with the name's.
    let version = container(text(concat!("v", env!("CARGO_PKG_VERSION"))).size(VERSION_SIZE))
        .padding(iced::padding::top(
            style::baseline(TITLE_SIZE) - style::baseline(VERSION_SIZE),
        ));
    let name = Row::new()
        .spacing(10)
        .push(text("ServerRemote").size(TITLE_SIZE).font(style::DISPLAY))
        .push(version);
    let links = Row::new()
        .spacing(style::SPACING)
        .push(icon_link_button(
            icons::LAUNCH,
            fl!("about-website"),
            WEBSITE_URL,
        ))
        .push(icon_link_button(
            icons::LOGO_GITHUB,
            "GitHub".to_owned(),
            update::PROJECT_URL,
        ));

    Row::new()
        .spacing(8)
        .push(logo)
        .push(
            Column::new()
                .spacing(6)
                .padding(iced::padding::top(4))
                .push(name)
                .push(links),
        )
        .into()
}

/// The contributors, each linking to their GitHub profile.
fn made_by<'a>() -> Element<'a, Message> {
    let people = Row::with_children(
        contributors::ALL
            .iter()
            .zip(AVATARS.iter())
            .map(|(contributor, avatar)| contributor_chip(contributor, avatar.clone())),
    )
    .spacing(style::SPACING)
    .width(Length::Fill)
    .wrap();
    Row::new()
        .spacing(8)
        .align_y(Alignment::Center)
        .push(text(fl!("about-made-by")).font(style::BOLD))
        .push(people)
        .into()
}

/// One image handle per contributor's avatar, made once. A handle built in
/// `view` gets a fresh ID every frame, so iced would restart decoding it each
/// frame and never get as far as drawing it.
static AVATARS: LazyLock<Vec<image::Handle>> = LazyLock::new(|| {
    contributors::ALL
        .iter()
        .map(|contributor| image::Handle::from_bytes(contributor.avatar))
        .collect()
});

fn contributor_chip(
    contributor: &contributors::Contributor,
    avatar: image::Handle,
) -> Element<'_, Message> {
    let avatar = image(avatar).width(16).height(16).border_radius(8);
    let label = Row::new()
        .spacing(6)
        .align_y(Alignment::Center)
        .push(text(format!("@{}", contributor.login)))
        .push(avatar);
    button(container(label).center_y(Length::Fill))
        .height(style::CONTROL_HEIGHT)
        .padding([0, 8])
        .style(style::filled_button)
        .on_press(Message::OpenUrl(format!(
            "https://github.com/{}",
            contributor.login
        )))
        .into()
}

fn licence<'a>() -> Element<'a, Message> {
    let links = Row::new()
        .spacing(style::SPACING)
        .push(link_button(fl!("about-mit-license"), MIT_LICENSE_URL))
        .push(link_button(fl!("about-apache-license"), APACHE_LICENSE_URL))
        .push(action_button(
            fl!("about-third-party-licences"),
            Some(Message::ShowThirdPartyLicences),
            false,
        ))
        .wrap();
    Column::new()
        .spacing(4)
        .push(text(fl!("about-licence")).size(14).font(style::BOLD))
        .push(text(fl!("about-licence-description")))
        .push(links)
        .into()
}

/// A filled button with an icon before its label, opening `url`.
fn icon_link_button<'a>(icon: Icon, label: String, url: &str) -> Element<'a, Message> {
    let content = Row::new()
        .spacing(4)
        .align_y(Alignment::Center)
        .push(themed_icon(
            svg(svg::Handle::from_memory(icon.svg())),
            |palette| palette.background.base.text,
        ))
        .push(text(label));
    button(container(content).center_y(Length::Fill))
        .height(style::CONTROL_HEIGHT)
        .padding(iced::padding::left(8).right(10))
        .style(style::filled_button)
        .on_press(Message::OpenUrl(url.to_owned()))
        .into()
}

/// The latest check's result, with the button that moves things on.
fn update_setting(status: &update::Status) -> Setting<'_> {
    let check = |label| action_button(label, Some(Message::CheckForUpdates), false);
    let (description, control): (String, Element<'_, Message>) = match status {
        update::Status::NotChecked => (fl!("update-not-checked"), check(fl!("update-check-now"))),
        update::Status::Checking => (
            fl!("update-checking"),
            action_button(fl!("update-check-now"), None, false),
        ),
        update::Status::Checked(Ok(update::Outcome::UpToDate)) => {
            (fl!("update-up-to-date"), check(fl!("update-check-again")))
        }
        update::Status::Checked(Ok(update::Outcome::NoReleases)) => {
            (fl!("update-no-releases"), check(fl!("update-check-again")))
        }
        update::Status::Checked(Ok(update::Outcome::Available { version, url })) => (
            fl!("update-available", version = version.to_string()),
            action_button(
                fl!("update-download"),
                Some(Message::OpenUrl(url.clone())),
                true,
            ),
        ),
        update::Status::Checked(Err(error)) => (
            fl!("update-failed", error = error.to_string()),
            check(fl!("update-try-again")),
        ),
    };

    setting(fl!("setting-updates"), description, control)
}

fn link_button<'a>(label: String, url: &str) -> Element<'a, Message> {
    action_button(label, Some(Message::OpenUrl(url.to_owned())), false)
}

/// A bordered button, or a filled one when `primary`; `None` disables it.
fn action_button<'a>(
    label: String,
    on_press: Option<Message>,
    primary: bool,
) -> Element<'a, Message> {
    button(container(text(label)).center_y(Length::Fill))
        .height(style::CONTROL_HEIGHT)
        .padding([0, 8])
        .style(move |theme, status| {
            if primary {
                style::dialog_button(theme, status, false)
            } else {
                style::bordered_button(theme, status, false)
            }
        })
        .on_press_maybe(on_press)
        .into()
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
            Item::entry(fl!("recording-folder-other"), Some(on_other))
                .icon(server_remote_icons::FOLDER),
        ])
        .collect()
}

fn default_recording_folder_label() -> String {
    directories::UserDirs::new()
        .and_then(|dirs| dirs.video_dir().map(folder_label))
        .unwrap_or_else(|| fl!("recording-folder-videos"))
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
    label: String,
    description: Cow<'a, str>,
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
    label: String,
    description: impl Into<Cow<'a, str>>,
    control: impl Into<Control<'a>>,
) -> Setting<'a> {
    Setting {
        label,
        description: description.into(),
        control: control.into(),
    }
}

impl<'a> Setting<'a> {
    /// The label and description with the control beside them, or with a
    /// block control underneath when `side_by_side` is false (space is
    /// tight). Switches always stay beside.
    fn view(self, side_by_side: bool) -> Element<'a, Message> {
        let description = (!self.description.is_empty()).then(|| muted(self.description));
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
    fn avatar_handles_stay_the_same_between_frames() {
        assert_eq!(AVATARS.len(), contributors::ALL.len());
        for (first, second) in AVATARS.iter().zip(AVATARS.iter()) {
            assert_eq!(first.clone().id(), second.clone().id());
        }
    }

    #[test]
    fn every_avatar_decodes() {
        for contributor in contributors::ALL {
            let avatar = ::image::load_from_memory(contributor.avatar)
                .unwrap_or_else(|error| panic!("@{}'s avatar: {error}", contributor.login));
            assert!(avatar.width() > 0, "@{}", contributor.login);
        }
    }

    #[test]
    fn fingerprints_split_into_two_lines() {
        let shown =
            fingerprint_halves(&server_remote_core::CertFingerprint([0xAB; 32]).to_string());
        let lines: Vec<&str> = shown.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().all(|line| line.split(':').count() == 16));
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
