//! Rendering icons from either the app's own set or connector-supplied data.
//!
//! SVG handles are identified by a hash of their contents, so they can be
//! built on the fly. Raster handles get a fresh id on every construction,
//! which would re-upload the image every frame, so a [`Cache`] keeps one
//! handle per image and is refreshed whenever new icons arrive from a session.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use iced::widget::{image, svg};
use iced::{Color, Element, Theme};
use server_remote_core::Icon;

/// An icon ready to draw.
#[derive(Debug, Clone)]
pub enum Handle {
    /// Tinted to the given colour when drawn.
    Svg(svg::Handle),
    /// Drawn as-is.
    Raster(image::Handle),
}

impl From<server_remote_icons::Icon> for Handle {
    fn from(icon: server_remote_icons::Icon) -> Self {
        Handle::Svg(svg::Handle::from_memory(icon.svg()))
    }
}

pub fn view<'a, Message: 'a>(
    handle: Handle,
    size: f32,
    color: impl Fn(&Theme) -> Color + 'a,
) -> Element<'a, Message> {
    match handle {
        Handle::Svg(handle) => svg(handle)
            .width(size)
            .height(size)
            .style(move |theme: &Theme, _status| svg::Style {
                color: Some(color(theme)),
            })
            .into(),
        Handle::Raster(handle) => image(handle).width(size).height(size).into(),
    }
}

/// Keeps one image handle per connector-supplied raster icon.
#[derive(Debug, Default)]
pub struct Cache {
    raster: HashMap<u64, image::Handle>,
}

impl Cache {
    /// Replaces the cached raster images with the ones in `icons`, keeping
    /// handles for images that are still in use.
    pub fn refresh<'a>(&mut self, icons: impl IntoIterator<Item = &'a Icon>) {
        let mut previous = std::mem::take(&mut self.raster);
        self.raster = icons
            .into_iter()
            .filter_map(|icon| match icon {
                Icon::Raster(bytes) => Some(bytes),
                Icon::Builtin(_) | Icon::Svg(_) => None,
            })
            .map(|bytes| {
                let key = content_hash(bytes);
                let handle = previous
                    .remove(&key)
                    .unwrap_or_else(|| image::Handle::from_bytes(bytes.to_vec()));
                (key, handle)
            })
            .collect();
    }

    pub fn handle(&self, icon: &Icon) -> Handle {
        match icon {
            Icon::Builtin(icon) => (*icon).into(),
            Icon::Svg(bytes) => Handle::Svg(svg::Handle::from_memory(bytes.clone())),
            Icon::Raster(bytes) => Handle::Raster(
                self.raster
                    .get(&content_hash(bytes))
                    .expect("raster icons are cached when capabilities or status arrive")
                    .clone(),
            ),
        }
    }
}

fn content_hash(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::*;

    fn raster(bytes: &'static [u8]) -> Icon {
        Icon::Raster(Cow::Borrowed(bytes))
    }

    #[test]
    fn refresh_keeps_handles_for_images_still_in_use() {
        let mut cache = Cache::default();
        cache.refresh(&[raster(b"one"), raster(b"two")]);
        let Handle::Raster(before) = cache.handle(&raster(b"one")) else {
            panic!("raster icon resolves to a raster handle");
        };

        cache.refresh(&[raster(b"one")]);
        let Handle::Raster(after) = cache.handle(&raster(b"one")) else {
            panic!("raster icon resolves to a raster handle");
        };

        assert_eq!(before.id(), after.id());
        assert_eq!(cache.raster.len(), 1);
    }

    #[test]
    fn builtin_and_svg_icons_need_no_cache() {
        let cache = Cache::default();
        assert!(matches!(
            cache.handle(&Icon::Builtin(server_remote_icons::POWER)),
            Handle::Svg(_)
        ));
        assert!(matches!(
            cache.handle(&Icon::Svg(Cow::Borrowed(b"<svg/>"))),
            Handle::Svg(_)
        ));
    }
}
