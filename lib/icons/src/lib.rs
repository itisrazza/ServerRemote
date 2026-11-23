//! The app's icon set: Carbon icons (<https://carbondesignsystem.com/elements/icons/library/>)
//! plus a few custom ones, exported from the Figma design and embedded at
//! compile time.
//!
//! The SVGs are drawn in white; tint them when rendering.

/// An embedded SVG icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Icon {
    svg: &'static [u8],
    name: &'static str,
}

impl Icon {
    pub const fn svg(self) -> &'static [u8] {
        self.svg
    }

    /// The icon's file name without `.svg`, e.g. `carbon_power`, so a front
    /// end can swap in its platform's own symbol for it.
    pub const fn name(self) -> &'static str {
        self.name
    }
}

macro_rules! icons {
    ($($name:ident = $file:literal;)*) => {
        $(pub const $name: Icon = Icon {
            svg: include_bytes!(concat!("../assets/", $file, ".svg")),
            name: $file,
        };)*

        /// Every icon in the set.
        pub const ALL: &[Icon] = &[$($name),*];
    };
}

icons! {
    ADD = "carbon_add";
    ADD_LARGE = "carbon_add-large";
    ARROW_DOWN = "carbon_arrow-down";
    ARROW_LEFT = "carbon_arrow-left";
    ARROW_RIGHT = "carbon_arrow-right";
    ARROW_UP = "carbon_arrow-up";
    ARROWS_HORIZONTAL = "carbon_arrows-horizontal";
    CAMERA = "carbon_camera";
    CHECKMARK = "carbon_checkmark";
    CHEVRON_DOWN = "carbon_chevron-down";
    CHEVRON_LEFT = "carbon_chevron-left";
    CHEVRON_RIGHT = "carbon_chevron-right";
    CHEVRON_UP = "carbon_chevron-up";
    CLOSE = "carbon_close";
    CLOSE_LARGE = "carbon_close-large";
    COPY_TO_CLIPBOARD = "carbon_copy-to-clipboard";
    CURSOR_1 = "carbon_cursor-1";
    DIAMOND_OUTLINE = "carbon_diamond-outline";
    DOWN_TO_BOTTOM = "carbon_down-to-bottom";
    DOWNLOAD = "carbon_download";
    FOLDER = "carbon_folder";
    INFORMATION = "carbon_information";
    KEYBOARD = "carbon_keyboard";
    LAUNCH = "carbon_launch";
    LOCKED = "carbon_locked";
    LOGO_GITHUB = "carbon_logo--github";
    POWER = "carbon_power";
    PROGRESS_BAR_ROUND = "carbon_progress-bar-round";
    RESTART = "carbon_restart";
    SAVE = "carbon_save";
    SCREEN = "carbon_screen";
    SETTINGS = "carbon_settings";
    UP_TO_TOP = "carbon_up-to-top";
    UPLOAD = "carbon_upload";
    VIRTUAL_DESKTOP = "carbon_virtual-desktop";
    VMDK_DISK = "carbon_vmdk-disk";
    KEY_BACKSPACE = "key_backspace";
    KEY_DELETE = "key_delete";
    KEY_ENTER = "key_enter";
    KEY_WINDOWS = "key_windows";
    PROTOCOL_ILO = "protocol_ilo";
    PROTOCOL_MSRDP = "protocol_msrdp";
    PROTOCOL_SPICE = "protocol_spice";
    PROTOCOL_VNC = "protocol_vnc";
}

/// `iced` integration, enabled by the `iced` feature.
#[cfg(feature = "iced")]
impl Icon {
    /// Builds an [`iced::widget::svg::Svg`] element for this icon.
    pub fn element<'a>(self) -> ::iced::widget::svg::Svg<'a, ::iced::Theme> {
        ::iced::widget::svg::Svg::new(::iced::widget::svg::Handle::from_memory(self.svg))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_is_an_svg_document() {
        for icon in ALL {
            let source = std::str::from_utf8(icon.svg()).expect("icon is UTF-8");
            assert!(source.trim_start().starts_with("<svg"), "{source}");
        }
    }
}
