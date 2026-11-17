//! The app's logo as a window icon, rasterised from the same SVG the macOS
//! icon bundle (`assets/icon.icon`) is built from.

use iced::window;

pub const LOGO: &[u8] = include_bytes!("../assets/icon.icon/Assets/logo.svg");
/// Large enough for taskbars and Alt-Tab on high-density screens; the
/// platform scales it down where it needs less.
const SIZE: u32 = 256;

/// The logo, for the title bar, taskbar and window switcher on Windows and
/// Linux. (macOS shows the app bundle's icon instead.)
pub fn window_icon() -> window::Icon {
    let rgba = rasterise(LOGO, SIZE).expect("the embedded logo is a valid SVG");
    window::icon::from_rgba(rgba, SIZE, SIZE).expect("the rasterised logo is SIZE×SIZE RGBA")
}

/// Renders `svg` into a `size`×`size` straight-alpha RGBA buffer, scaled to
/// fill it.
fn rasterise(svg: &[u8], size: u32) -> Result<Vec<u8>, resvg::usvg::Error> {
    let tree = resvg::usvg::Tree::from_data(svg, &resvg::usvg::Options::default())?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size).expect("the icon size is non-zero");
    let scale = size as f32 / tree.size().width().max(tree.size().height());
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );

    // tiny-skia stores premultiplied alpha; window icons want it straight.
    Ok(pixmap
        .pixels()
        .iter()
        .flat_map(|pixel| {
            let color = pixel.demultiply();
            [color.red(), color.green(), color.blue(), color.alpha()]
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_logo_becomes_a_window_icon() {
        let _ = window_icon();
    }

    #[test]
    fn the_logo_fills_the_icon() {
        let rgba = rasterise(LOGO, 64).unwrap();
        assert_eq!(rgba.len(), 64 * 64 * 4);

        let alpha = |x: usize, y: usize| rgba[(y * 64 + x) * 4 + 3];
        // The tile fills the upper left; the bottom-left corner is empty.
        assert_eq!(alpha(16, 8), 255, "inside the tile");
        assert_eq!(alpha(4, 60), 0, "outside the logo");
    }

    #[test]
    fn invalid_svg_is_an_error() {
        assert!(rasterise(b"not an svg", 16).is_err());
    }
}
