//! App-wide settings, persisted as `settings.toml` in the platform config
//! directory and saved on every change.
//!
//! Some settings describe features that aren't built yet (recording, display
//! scaling); they're stored so the choices survive until those features land.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ui::connection::KeyboardSize;

use super::store;
use crate::i18n::fl;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub general: General,
    pub display: Display,
    pub input: Input,
    pub recording: Recording,
    pub updates: Updates,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct General {
    /// Ask before closing the window while connections are open.
    pub confirm_quit: bool,
    pub appearance: Appearance,
    pub language: Language,
    /// The GPU API to draw with, on Windows. Takes effect on next launch.
    pub graphics_backend: GraphicsBackend,
}

impl Default for General {
    fn default() -> Self {
        General {
            confirm_quit: true,
            appearance: Appearance::default(),
            language: Language::default(),
            graphics_backend: GraphicsBackend::default(),
        }
    }
}

/// Light or dark colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Appearance {
    /// Whichever the operating system is set to.
    #[default]
    System,
    Light,
    Dark,
}

impl Appearance {
    pub const ALL: [Appearance; 3] = [Appearance::System, Appearance::Light, Appearance::Dark];

    pub fn label(self) -> String {
        match self {
            Appearance::System => fl!("appearance-system"),
            Appearance::Light => fl!("appearance-light"),
            Appearance::Dark => fl!("appearance-dark"),
        }
    }
}

/// The language the UI is shown in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Language {
    /// The operating system's preferred language, if translated.
    #[default]
    System,
    English,
    Romanian,
}

impl Language {
    pub const ALL: [Language; 3] = [Language::System, Language::English, Language::Romanian];

    /// Each language is named in itself, so it can be found whatever the
    /// current language.
    pub fn label(self) -> String {
        match self {
            Language::System => fl!("language-system"),
            Language::English => "English".to_owned(),
            Language::Romanian => "Română".to_owned(),
        }
    }
}

/// Which GPU API wgpu draws with on Windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GraphicsBackend {
    /// Keeps up with live window resizing better than Vulkan.
    #[default]
    DirectX12,
    Vulkan,
}

impl GraphicsBackend {
    pub const ALL: [GraphicsBackend; 2] = [GraphicsBackend::DirectX12, GraphicsBackend::Vulkan];

    pub fn label(self) -> String {
        match self {
            GraphicsBackend::DirectX12 => "DirectX 12".to_owned(),
            GraphicsBackend::Vulkan => "Vulkan".to_owned(),
        }
    }

    /// The value wgpu expects in `WGPU_BACKEND`.
    pub fn wgpu_name(self) -> &'static str {
        match self {
            GraphicsBackend::DirectX12 => "dx12",
            GraphicsBackend::Vulkan => "vulkan",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Updates {
    /// Ask GitHub for a newer release each time the app opens.
    pub check_on_launch: bool,
}

impl Default for Updates {
    fn default() -> Self {
        Updates {
            check_on_launch: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Display {
    pub scaling: Scaling,
    pub keep_aspect_ratio: bool,
    pub scale: DisplayScale,
}

impl Default for Display {
    fn default() -> Self {
        Display {
            scaling: Scaling::Fit,
            keep_aspect_ratio: true,
            scale: DisplayScale::Automatic,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Scaling {
    /// Scale the remote display to fit the window.
    #[default]
    Fit,
    /// Show the remote display at its actual size.
    Actual,
}

impl Scaling {
    pub const ALL: [Scaling; 2] = [Scaling::Fit, Scaling::Actual];

    pub fn label(self) -> String {
        match self {
            Scaling::Fit => fl!("scaling-fit"),
            Scaling::Actual => fl!("scaling-actual"),
        }
    }
}

/// How the remote display's pixels map to the local screen's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DisplayScale {
    /// Follow the local screen's scale factor.
    #[default]
    Automatic,
    #[serde(rename = "100%")]
    Percent100,
    #[serde(rename = "125%")]
    Percent125,
    #[serde(rename = "150%")]
    Percent150,
    #[serde(rename = "175%")]
    Percent175,
    #[serde(rename = "200%")]
    Percent200,
}

impl DisplayScale {
    pub const ALL: [DisplayScale; 6] = [
        DisplayScale::Automatic,
        DisplayScale::Percent100,
        DisplayScale::Percent125,
        DisplayScale::Percent150,
        DisplayScale::Percent175,
        DisplayScale::Percent200,
    ];

    pub fn label(self) -> String {
        match self {
            DisplayScale::Automatic => fl!("display-scale-automatic"),
            DisplayScale::Percent100 => "100%".to_owned(),
            DisplayScale::Percent125 => "125%".to_owned(),
            DisplayScale::Percent150 => "150%".to_owned(),
            DisplayScale::Percent175 => "175%".to_owned(),
            DisplayScale::Percent200 => "200%".to_owned(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Input {
    /// The virtual keyboard's layout when a connection opens.
    pub keyboard_style: KeyboardSize,
    pub open_keyboard_on_connect: bool,
    pub keyboard_layout: KeyboardLayout,
}

/// The remote machine's keyboard layout, for mapping keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KeyboardLayout {
    #[default]
    UsQwerty,
    UkQwerty,
    GermanQwertz,
    FrenchAzerty,
}

impl KeyboardLayout {
    pub const ALL: [KeyboardLayout; 4] = [
        KeyboardLayout::UsQwerty,
        KeyboardLayout::UkQwerty,
        KeyboardLayout::GermanQwertz,
        KeyboardLayout::FrenchAzerty,
    ];

    pub fn label(self) -> String {
        match self {
            KeyboardLayout::UsQwerty => fl!("keyboard-layout-us"),
            KeyboardLayout::UkQwerty => fl!("keyboard-layout-uk"),
            KeyboardLayout::GermanQwertz => fl!("keyboard-layout-german"),
            KeyboardLayout::FrenchAzerty => fl!("keyboard-layout-french"),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Recording {
    /// Where recordings are saved; `None` means the system's videos folder.
    pub folder: Option<PathBuf>,
    /// Folders chosen before, most recent first, offered in the Save to menu.
    pub recent_folders: Vec<PathBuf>,
    pub format: RecordingFormat,
    pub quality: RecordingQuality,
}

/// How many folders [`Recording::recent_folders`] remembers.
pub const RECENT_FOLDERS: usize = 5;

impl Recording {
    /// Saves to `folder` from now on, remembering it among the recent ones.
    pub fn choose_folder(&mut self, folder: PathBuf) {
        self.recent_folders.retain(|recent| *recent != folder);
        self.recent_folders.insert(0, folder.clone());
        self.recent_folders.truncate(RECENT_FOLDERS);
        self.folder = Some(folder);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RecordingFormat {
    /// Our own format: frames and input events, timestamped.
    #[default]
    ServerRemote,
    QuickTime,
    WebP,
    Mkv,
}

impl RecordingFormat {
    pub const ALL: [RecordingFormat; 4] = [
        RecordingFormat::ServerRemote,
        RecordingFormat::QuickTime,
        RecordingFormat::WebP,
        RecordingFormat::Mkv,
    ];

    pub fn label(self) -> String {
        match self {
            RecordingFormat::ServerRemote => "ServerRemote".to_owned(),
            RecordingFormat::QuickTime => "QuickTime".to_owned(),
            RecordingFormat::WebP => "WebP".to_owned(),
            RecordingFormat::Mkv => "MKV".to_owned(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RecordingQuality {
    Low,
    Medium,
    #[default]
    High,
    Lossless,
}

impl RecordingQuality {
    pub const ALL: [RecordingQuality; 4] = [
        RecordingQuality::Low,
        RecordingQuality::Medium,
        RecordingQuality::High,
        RecordingQuality::Lossless,
    ];

    pub fn label(self) -> String {
        match self {
            RecordingQuality::Low => fl!("quality-low"),
            RecordingQuality::Medium => fl!("quality-medium"),
            RecordingQuality::High => fl!("quality-high"),
            RecordingQuality::Lossless => fl!("quality-lossless"),
        }
    }
}

/// The directory settings live in, or `None` if the platform has none.
pub fn config_dir() -> Option<PathBuf> {
    store::config_dir()
}

impl Settings {
    fn path(dir: &Path) -> PathBuf {
        dir.join("settings.toml")
    }

    /// Loads settings from `dir`, falling back to defaults if there's no
    /// directory, or the file is missing, unreadable, or fails to parse.
    pub fn load(dir: Option<&Path>) -> Self {
        let Some(dir) = dir else {
            return Self::default();
        };
        store::load(&Self::path(dir)).unwrap_or_else(|error| {
            eprintln!("failed to load settings: {error}");
            Self::default()
        })
    }

    /// Persists settings to `dir`; does nothing without one. Failures are
    /// logged to stderr rather than surfaced to the UI, as with saved
    /// sessions.
    pub fn save(&self, dir: Option<&Path>) {
        let Some(dir) = dir else {
            return;
        };
        if let Err(error) = store::save(&Self::path(dir), self) {
            eprintln!("failed to save settings: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_through_toml() {
        let mut settings = Settings::default();
        settings.general.confirm_quit = false;
        settings.display.scale = DisplayScale::Percent150;
        settings.input.keyboard_style = KeyboardSize::Large;
        settings
            .recording
            .choose_folder(PathBuf::from("/recordings"));
        settings.recording.format = RecordingFormat::Mkv;

        let text = toml::to_string_pretty(&settings).unwrap();
        let parsed: Settings = toml::from_str(&text).unwrap();

        assert_eq!(parsed, settings);
    }

    #[test]
    fn save_then_load_through_a_directory() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = Settings::default();
        settings.general.graphics_backend = GraphicsBackend::Vulkan;

        settings.save(Some(dir.path()));

        assert_eq!(Settings::load(Some(dir.path())), settings);
    }

    #[test]
    fn without_a_directory_nothing_is_stored() {
        Settings::default().save(None);
        assert_eq!(Settings::load(None), Settings::default());
    }

    #[test]
    fn missing_fields_take_their_defaults() {
        let parsed: Settings = toml::from_str(
            r#"
            [display]
            scale = "125%"
            "#,
        )
        .unwrap();

        assert_eq!(
            parsed,
            Settings {
                display: Display {
                    scale: DisplayScale::Percent125,
                    ..Display::default()
                },
                ..Settings::default()
            }
        );
    }

    #[test]
    fn choosing_folders_keeps_a_short_recent_list() {
        let folder = |name: &str| PathBuf::from(format!("/{name}"));
        let cases = [
            ("first choice", vec!["a"], vec!["a"]),
            ("most recent first", vec!["a", "b"], vec!["b", "a"]),
            (
                "re-choosing moves it to the front",
                vec!["a", "b", "a"],
                vec!["a", "b"],
            ),
            (
                "only the last few are kept",
                vec!["a", "b", "c", "d", "e", "f"],
                vec!["f", "e", "d", "c", "b"],
            ),
        ];

        for (name, chosen, expected) in cases {
            let mut recording = Recording::default();
            for choice in &chosen {
                recording.choose_folder(folder(choice));
            }
            let expected: Vec<PathBuf> = expected.into_iter().map(folder).collect();
            assert_eq!(recording.recent_folders, expected, "{name}");
            assert_eq!(recording.folder.as_ref(), expected.first(), "{name}");
        }
    }
}
