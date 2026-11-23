//! What the front end needs to look like the desktop app: its fonts, and its
//! icons rendered to images.

use std::hash::{DefaultHasher, Hash, Hasher};

use server_remote_core::Icon;
use server_remote_icons as icons;

use crate::ConnectionProtocol;

/// Nunito Sans, as the desktop app embeds it.
const FONTS: [&[u8]; 2] = [
    include_bytes!("../../desktop/assets/fonts/NunitoSans-Medium.ttf"),
    include_bytes!("../../desktop/assets/fonts/NunitoSans-Bold.ttf"),
];

/// The font files to register before drawing any text.
#[uniffi::export]
pub fn fonts() -> Vec<Vec<u8>> {
    FONTS.iter().map(|font| font.to_vec()).collect()
}

/// The built-in icons the front end draws itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum AppIcon {
    Add,
    AddLarge,
    ChevronDown,
    ChevronRight,
    Close,
    CopyToClipboard,
    Cursor,
    Download,
    Information,
    Keyboard,
    Launch,
    Locked,
    Save,
    Settings,
    Upload,
    VirtualDesktop,
}

impl AppIcon {
    fn icon(self) -> icons::Icon {
        match self {
            AppIcon::Add => icons::ADD,
            AppIcon::AddLarge => icons::ADD_LARGE,
            AppIcon::ChevronDown => icons::CHEVRON_DOWN,
            AppIcon::ChevronRight => icons::CHEVRON_RIGHT,
            AppIcon::Close => icons::CLOSE,
            AppIcon::CopyToClipboard => icons::COPY_TO_CLIPBOARD,
            AppIcon::Cursor => icons::CURSOR_1,
            AppIcon::Download => icons::DOWNLOAD,
            AppIcon::Information => icons::INFORMATION,
            AppIcon::Keyboard => icons::KEYBOARD,
            AppIcon::Launch => icons::LAUNCH,
            AppIcon::Locked => icons::LOCKED,
            AppIcon::Save => icons::SAVE,
            AppIcon::Settings => icons::SETTINGS,
            AppIcon::Upload => icons::UPLOAD,
            AppIcon::VirtualDesktop => icons::VIRTUAL_DESKTOP,
        }
    }
}

/// An icon as a PNG.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct IconImage {
    pub png: Vec<u8>,
    /// Draw it in the text colour (only its shape matters); otherwise as is.
    pub template: bool,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum IconError {
    #[error("the icon isn't a usable SVG: {0}")]
    Svg(String),
    #[error("an icon can't be drawn {0} pixels across")]
    Size(u32),
    #[error("couldn't encode the icon: {0}")]
    Encode(String),
}

/// A built-in icon, `pixels` across.
#[uniffi::export]
pub fn app_icon(icon: AppIcon, pixels: u32) -> Result<IconImage, IconError> {
    render(&Icon::Builtin(icon.icon()), pixels)
}

/// A protocol's icon, `pixels` across.
#[uniffi::export]
pub fn protocol_icon(protocol: ConnectionProtocol, pixels: u32) -> Result<IconImage, IconError> {
    let protocol = server_remote_app::Protocol::from(protocol);
    render(&Icon::Builtin(protocol.icon()), pixels)
}

/// Identifies a connector's icon in the records, for
/// [`crate::Core::connection_icon`].
pub fn icon_id(icon: &Icon) -> u64 {
    let mut hasher = DefaultHasher::new();
    match icon {
        Icon::Builtin(icon) => (0u8, icon.svg()).hash(&mut hasher),
        Icon::Svg(svg) => (1u8, svg.as_ref()).hash(&mut hasher),
        Icon::Raster(image) => (2u8, image.as_ref()).hash(&mut hasher),
    }
    hasher.finish()
}

/// Renders an SVG icon to a PNG `pixels` across; raster icons pass through.
pub fn render(icon: &Icon, pixels: u32) -> Result<IconImage, IconError> {
    let svg = match icon {
        Icon::Builtin(icon) => icon.svg(),
        Icon::Svg(svg) => svg.as_ref(),
        Icon::Raster(image) => {
            return Ok(IconImage {
                png: image.to_vec(),
                template: false,
            });
        }
    };
    let tree = resvg::usvg::Tree::from_data(svg, &resvg::usvg::Options::default())
        .map_err(|error| IconError::Svg(error.to_string()))?;
    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(pixels, pixels).ok_or(IconError::Size(pixels))?;
    let size = tree.size();
    let scale = pixels as f32 / size.width().max(size.height());
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let png = pixmap
        .encode_png()
        .map_err(|error| IconError::Encode(error.to_string()))?;
    Ok(IconImage {
        png,
        template: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_app_icon_renders() {
        let all = [
            AppIcon::Add,
            AppIcon::AddLarge,
            AppIcon::ChevronDown,
            AppIcon::ChevronRight,
            AppIcon::Close,
            AppIcon::CopyToClipboard,
            AppIcon::Cursor,
            AppIcon::Download,
            AppIcon::Information,
            AppIcon::Keyboard,
            AppIcon::Launch,
            AppIcon::Locked,
            AppIcon::Save,
            AppIcon::Settings,
            AppIcon::Upload,
            AppIcon::VirtualDesktop,
        ];
        for icon in all {
            let image = app_icon(icon, 32).unwrap_or_else(|error| panic!("{icon:?}: {error}"));
            assert!(image.template, "{icon:?}");
            assert!(image.png.starts_with(b"\x89PNG"), "{icon:?}");
        }
    }

    #[test]
    fn raster_icons_pass_through() {
        let png = b"\x89PNG not really".to_vec();
        let image = render(&Icon::Raster(png.clone().into()), 32).expect("passes through");
        assert_eq!(image.png, png);
        assert!(!image.template);
    }

    #[test]
    fn a_zero_size_is_an_error() {
        assert!(matches!(app_icon(AppIcon::Add, 0), Err(IconError::Size(0))));
    }

    #[test]
    fn fonts_are_truetype() {
        assert!(fonts().iter().all(|font| font.starts_with(&[0, 1, 0, 0])));
    }
}
