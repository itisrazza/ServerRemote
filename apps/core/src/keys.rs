//! Turning keys into X11 keysyms, which every protocol here takes.

/// The keysym for a typed character. Latin-1 code points are their own
/// keysyms; the rest use the Unicode form.
pub fn character_keysym(character: char) -> u32 {
    let code = character as u32;
    if code <= 0xFF {
        code
    } else {
        0x0100_0000 + code
    }
}

/// The keysym for a USB HID keyboard usage (page 0x07), as iPadOS and
/// Android report hardware keys. Keys that type something map to their
/// unshifted US layout character; the server applies Shift itself.
pub fn hid_keysym(usage: u32) -> Option<u32> {
    let character = |c: char| Some(character_keysym(c));
    match usage {
        0x04..=0x1D => char::from_u32('a' as u32 + usage - 0x04).and_then(character),
        0x1E..=0x26 => char::from_u32('1' as u32 + usage - 0x1E).and_then(character),
        0x27 => character('0'),
        0x28 => Some(0xFF0D),
        0x29 => Some(0xFF1B),
        0x2A => Some(0xFF08),
        0x2B => Some(0xFF09),
        0x2C => character(' '),
        0x2D => character('-'),
        0x2E => character('='),
        0x2F => character('['),
        0x30 => character(']'),
        0x31 => character('\\'),
        0x33 => character(';'),
        0x34 => character('\''),
        0x35 => character('`'),
        0x36 => character(','),
        0x37 => character('.'),
        0x38 => character('/'),
        0x39 => Some(0xFFE5),
        // F1 to F12.
        0x3A..=0x45 => Some(0xFFBE + usage - 0x3A),
        0x46 => Some(0xFF61),
        0x47 => Some(0xFF14),
        0x48 => Some(0xFF13),
        0x49 => Some(0xFF63),
        0x4A => Some(0xFF50),
        0x4B => Some(0xFF55),
        0x4C => Some(0xFFFF),
        0x4D => Some(0xFF57),
        0x4E => Some(0xFF56),
        0x4F => Some(0xFF53),
        0x50 => Some(0xFF51),
        0x51 => Some(0xFF54),
        0x52 => Some(0xFF52),
        0x53 => Some(0xFF7F),
        0x58 => Some(0xFF8D),
        0x65 => Some(0xFF67),
        0xE0 => Some(0xFFE3),
        0xE1 => Some(0xFFE1),
        0xE2 => Some(0xFFE9),
        0xE3 => Some(0xFFEB),
        0xE4 => Some(0xFFE4),
        0xE5 => Some(0xFFE2),
        0xE6 => Some(0xFFEA),
        0xE7 => Some(0xFFEC),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn characters_map_to_latin1_keysyms() {
        assert_eq!(character_keysym('a'), 0x61);
        assert_eq!(character_keysym('A'), 0x41);
        assert_eq!(character_keysym('!'), 0x21);
        // Beyond Latin-1 uses the Unicode keysym form.
        assert_eq!(character_keysym('€'), 0x0100_0000 + '€' as u32);
    }

    #[test]
    fn hid_usages_map_to_keysyms() {
        let cases = [
            ("a", 0x04, Some(0x61)),
            ("z", 0x1D, Some(0x7A)),
            ("1", 0x1E, Some(0x31)),
            ("9", 0x26, Some(0x39)),
            ("0", 0x27, Some(0x30)),
            ("enter", 0x28, Some(0xFF0D)),
            ("escape", 0x29, Some(0xFF1B)),
            ("space", 0x2C, Some(0x20)),
            ("slash", 0x38, Some(0x2F)),
            ("F1", 0x3A, Some(0xFFBE)),
            ("F12", 0x45, Some(0xFFC9)),
            ("delete", 0x4C, Some(0xFFFF)),
            ("up", 0x52, Some(0xFF52)),
            ("left control", 0xE0, Some(0xFFE3)),
            ("right command", 0xE7, Some(0xFFEC)),
            ("unmapped", 0x32, None),
        ];
        for (name, usage, expected) in cases {
            assert_eq!(hid_keysym(usage), expected, "{name}");
        }
    }
}
