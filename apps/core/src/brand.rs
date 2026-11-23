//! The app's colours that come from its logo, shared by every front end.

/// A colour as sRGB bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Rgb {
    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }

    /// Mixed `quarters`/4 of the way towards white.
    const fn towards_white(self, quarters: u16) -> Self {
        const fn mix(channel: u8, quarters: u16) -> u8 {
            (channel as u16 + (255 - channel as u16) * quarters / 4) as u8
        }
        Self::new(
            mix(self.red, quarters),
            mix(self.green, quarters),
            mix(self.blue, quarters),
        )
    }

    /// The `#rrggbb` form, as CSS writes it.
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.red, self.green, self.blue)
    }
}

/// The logo's blue (`apps/desktop/assets/icon.icon/Assets/logo.svg`).
pub const LOGO: Rgb = Rgb::new(0x44, 0x3D, 0xFF);

/// The accent on light backgrounds: the logo's own blue, under white text.
pub const ACCENT_LIGHT: Rgb = LOGO;
/// The accent on dark backgrounds: the logo's blue a quarter of the way to
/// white, which stands out on them. No shade of it can carry white text and
/// still stand out, so it carries dark text instead ([`ON_ACCENT_DARK`]).
pub const ACCENT_DARK: Rgb = LOGO.towards_white(1);
/// Text on [`ACCENT_LIGHT`].
pub const ON_ACCENT_LIGHT: Rgb = Rgb::new(0xFF, 0xFF, 0xFF);
/// Text on [`ACCENT_DARK`]: the dark theme's background.
pub const ON_ACCENT_DARK: Rgb = Rgb::new(0x12, 0x12, 0x12);

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG relative luminance.
    fn luminance(colour: Rgb) -> f64 {
        let linear = |channel: u8| {
            let c = f64::from(channel) / 255.0;
            if c <= 0.039_28 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(colour.red) + 0.7152 * linear(colour.green) + 0.0722 * linear(colour.blue)
    }

    fn contrast(a: Rgb, b: Rgb) -> f64 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn the_dark_accent_is_the_logo_lightened() {
        assert_eq!(ACCENT_DARK.hex(), "#726dff");
    }

    #[test]
    fn accents_are_readable() {
        let light_background = Rgb::new(0xF0, 0xF0, 0xF0);
        let dark_background = Rgb::new(0x12, 0x12, 0x12);
        let cases = [
            (
                "light accent on its background",
                ACCENT_LIGHT,
                light_background,
            ),
            ("text on the light accent", ACCENT_LIGHT, ON_ACCENT_LIGHT),
            (
                "dark accent on its background",
                ACCENT_DARK,
                dark_background,
            ),
            ("text on the dark accent", ACCENT_DARK, ON_ACCENT_DARK),
        ];
        for (name, a, b) in cases {
            let ratio = contrast(a, b);
            assert!(ratio >= 4.5, "{name}: {ratio:.2}");
        }
    }
}
