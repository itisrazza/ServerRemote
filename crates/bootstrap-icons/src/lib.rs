//! Bootstrap Icons (<https://github.com/twbs/icons>), vendored as a git
//! submodule and embedded into the binary at build time.
//!
//! ```
//! let svg: &[u8] = bootstrap_icons::bi!(gear-wide-connected);
//! assert_eq!(svg, bootstrap_icons::Icons::GearWideConnected.svg());
//! ```

include!(concat!(env!("OUT_DIR"), "/icons.rs"));

pub use bootstrap_icons_macros::bi;

/// `iced` integration, enabled by the `iced` feature.
#[cfg(feature = "iced")]
pub mod iced {
    pub use bootstrap_icons_macros::iced_bi as bi;

    /// Builds an [`iced::widget::svg::Svg`] element for the given icon.
    pub fn element(icon: crate::Icons) -> ::iced::widget::svg::Svg<'static, ::iced::Theme> {
        ::iced::widget::svg::Svg::new(::iced::widget::svg::Handle::from_memory(icon.svg()))
    }
}
