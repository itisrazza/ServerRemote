//! The settings the iPadOS app offers, and its About page. Settings for
//! things it doesn't have yet (recording, the virtual keyboard, the update
//! check) stay out until it does.

use server_remote_app::about;
use server_remote_app::persistence::settings::{
    Appearance, CertificateChecking, Change, KnownCertificate, Language, Scaling, Settings,
};
use server_remote_core::Icon;

use crate::look::{self, IconError, IconImage};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum AppearanceSetting {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum LanguageSetting {
    System,
    English,
    Romanian,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum ScalingSetting {
    Fit,
    Actual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum CertificateCheckingSetting {
    Strict,
    Ask,
    Off,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SettingsState {
    pub appearance: AppearanceSetting,
    pub language: LanguageSetting,
    pub scaling: ScalingSetting,
    pub keep_aspect_ratio: bool,
    pub certificate_checking: CertificateCheckingSetting,
    pub trusted: Vec<Certificate>,
    pub blocked: Vec<Certificate>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Certificate {
    pub host: String,
    pub port: u16,
    /// "AB:CD:…", 32 bytes.
    pub fingerprint: String,
}

/// A change the settings page makes. Certificates are picked by their
/// position in [`SettingsState`]'s lists.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum SettingChange {
    Appearance { value: AppearanceSetting },
    Language { value: LanguageSetting },
    Scaling { value: ScalingSetting },
    KeepAspectRatio { on: bool },
    CertificateChecking { value: CertificateCheckingSetting },
    ForgetTrusted { index: u32 },
    UnblockBlocked { index: u32 },
}

/// A choice's name, in the app's language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum SettingChoice {
    Appearance { value: AppearanceSetting },
    Language { value: LanguageSetting },
    Scaling { value: ScalingSetting },
    CertificateChecking { value: CertificateCheckingSetting },
}

#[uniffi::export]
pub fn choice_label(choice: SettingChoice) -> String {
    match choice {
        SettingChoice::Appearance { value } => Appearance::from(value).label(),
        SettingChoice::Language { value } => Language::from(value).label(),
        SettingChoice::Scaling { value } => Scaling::from(value).label(),
        SettingChoice::CertificateChecking { value } => CertificateChecking::from(value).label(),
    }
}

/// What a certificate checking choice does, for under the picker.
#[uniffi::export]
pub fn certificate_checking_description(value: CertificateCheckingSetting) -> String {
    CertificateChecking::from(value).description()
}

impl From<&Settings> for SettingsState {
    fn from(settings: &Settings) -> Self {
        let certificates = |list: &[KnownCertificate]| {
            list.iter()
                .map(|certificate| Certificate {
                    host: certificate.host.clone(),
                    port: certificate.port,
                    fingerprint: certificate.fingerprint.to_string(),
                })
                .collect()
        };
        SettingsState {
            appearance: settings.general.appearance.into(),
            language: settings.general.language.into(),
            scaling: settings.display.scaling.into(),
            keep_aspect_ratio: settings.display.keep_aspect_ratio,
            certificate_checking: settings.security.certificate_checking.into(),
            trusted: certificates(&settings.security.trusted),
            blocked: certificates(&settings.security.blocked),
        }
    }
}

impl SettingChange {
    /// The app's own change, or `None` for a certificate that's no longer
    /// listed.
    pub(crate) fn resolve(self, settings: &Settings) -> Option<Change> {
        let at = |list: &[KnownCertificate], index: u32| {
            usize::try_from(index)
                .ok()
                .and_then(|index| list.get(index))
                .cloned()
        };
        Some(match self {
            SettingChange::Appearance { value } => Change::Appearance(value.into()),
            SettingChange::Language { value } => Change::Language(value.into()),
            SettingChange::Scaling { value } => Change::Scaling(value.into()),
            SettingChange::KeepAspectRatio { on } => Change::KeepAspectRatio(on),
            SettingChange::CertificateChecking { value } => {
                Change::CertificateChecking(value.into())
            }
            SettingChange::ForgetTrusted { index } => {
                Change::ForgetCertificate(at(&settings.security.trusted, index)?)
            }
            SettingChange::UnblockBlocked { index } => {
                Change::UnblockCertificate(at(&settings.security.blocked, index)?)
            }
        })
    }
}

/// Converts between an FFI enum and the app's, both ways.
macro_rules! mirror {
    ($ffi:ident <=> $app:ident { $($variant:ident),* }) => {
        impl From<$app> for $ffi {
            fn from(value: $app) -> Self {
                match value { $($app::$variant => $ffi::$variant),* }
            }
        }
        impl From<$ffi> for $app {
            fn from(value: $ffi) -> Self {
                match value { $($ffi::$variant => $app::$variant),* }
            }
        }
    };
}

mirror!(AppearanceSetting <=> Appearance { System, Light, Dark });
mirror!(LanguageSetting <=> Language { System, English, Romanian });
mirror!(ScalingSetting <=> Scaling { Fit, Actual });
mirror!(CertificateCheckingSetting <=> CertificateChecking { Strict, Ask, Off });

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AboutInfo {
    pub version: String,
    pub website_url: String,
    pub project_url: String,
    pub mit_license_url: String,
    pub apache_license_url: String,
    /// The icon set, credited among the third-party licences.
    pub carbon_url: String,
    pub contributors: Vec<Contributor>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Contributor {
    pub login: String,
    /// A small square photo (PNG or JPEG).
    pub avatar: Vec<u8>,
}

#[uniffi::export]
pub fn about_info() -> AboutInfo {
    AboutInfo {
        version: about::VERSION.to_owned(),
        website_url: about::WEBSITE_URL.to_owned(),
        project_url: about::PROJECT_URL.to_owned(),
        mit_license_url: about::MIT_LICENSE_URL.to_owned(),
        apache_license_url: about::APACHE_LICENSE_URL.to_owned(),
        carbon_url: about::CARBON_URL.to_owned(),
        contributors: about::CONTRIBUTORS
            .iter()
            .map(|contributor| Contributor {
                login: contributor.login.to_owned(),
                avatar: contributor.avatar.to_vec(),
            })
            .collect(),
    }
}

/// The app's logo, in colour, as the desktop app and its icon draw it.
const LOGO: &[u8] = include_bytes!("../../desktop/assets/icon.icon/Assets/logo.svg");

/// The logo, `pixels` across.
#[uniffi::export]
pub fn logo(pixels: u32) -> Result<IconImage, IconError> {
    let image = look::render(&Icon::Svg(LOGO.into()), pixels)?;
    Ok(IconImage {
        template: false,
        ..image
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use server_remote_core::{CertFingerprint, Target};

    #[test]
    fn certificates_are_picked_by_position() {
        let mut settings = Settings::default();
        let first = KnownCertificate::new(&Target::new("ilo", 443), CertFingerprint([1; 32]));
        let second = KnownCertificate::new(&Target::new("ilo2", 443), CertFingerprint([2; 32]));
        settings.security.trusted = vec![first, second.clone()];

        let change = SettingChange::ForgetTrusted { index: 1 }.resolve(&settings);
        assert_eq!(change, Some(Change::ForgetCertificate(second)));
        assert_eq!(
            SettingChange::UnblockBlocked { index: 0 }.resolve(&settings),
            None
        );
    }

    #[test]
    fn the_state_mirrors_the_settings() {
        let state = SettingsState::from(&Settings::default());
        assert_eq!(state.certificate_checking, CertificateCheckingSetting::Ask);
        assert!(state.trusted.is_empty());
    }

    #[test]
    fn the_logo_renders_in_colour() {
        let image = logo(64).expect("the logo renders");
        assert!(!image.template);
        assert!(image.png.starts_with(b"\x89PNG"));
    }
}
