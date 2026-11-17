//! Translates the X11 keysyms `server-remote`'s UI produces into PC/AT
//! Set-1 scan codes, which is what SPICE's inputs channel expects.
//!
//! Unlike VNC/RFB, SPICE has no notion of a keysym — it drives the guest's
//! virtual PS/2 keyboard controller directly with scan codes, so shifted
//! characters (`'A'`, `'!'`, ...) need a synthetic Shift bracketed around the
//! base key's scan code, since the keysym already bakes in which character
//! shift produced rather than naming a physical key plus a modifier.

/// A PC/AT Set-1 scan code. `Extended` codes are preceded on the wire by a
/// `0xE0` byte (see [`crate::wire`]'s key-event encoding).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanCode {
    Simple(u8),
    Extended(u8),
}

/// A scan code plus whether it must be sent with a synthetic Shift held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyTranslation {
    pub code: ScanCode,
    pub shift: bool,
}

const fn simple(code: u8) -> KeyTranslation {
    KeyTranslation {
        code: ScanCode::Simple(code),
        shift: false,
    }
}

const fn simple_shifted(code: u8) -> KeyTranslation {
    KeyTranslation {
        code: ScanCode::Simple(code),
        shift: true,
    }
}

const fn extended(code: u8) -> KeyTranslation {
    KeyTranslation {
        code: ScanCode::Extended(code),
        shift: false,
    }
}

/// Left Shift's own scan code, used to bracket shifted characters.
pub const LEFT_SHIFT: u8 = 0x2A;

/// Translate an X11 keysym (as produced by `server-remote`'s
/// `character_keysym`/`named_keysym`) into a scan code. Returns `None` for
/// keysyms outside what the UI can produce — dropped rather than sent as a
/// garbage scan code.
pub fn keysym_to_scancode(keysym: u32) -> Option<KeyTranslation> {
    match keysym {
        // Named keys (server-remote/src/ui/connection.rs named_keysym()).
        0xFF0D => Some(simple(0x1C)),       // Enter
        0xFF09 => Some(simple(0x0F)),       // Tab
        0xFF08 => Some(simple(0x0E)),       // Backspace
        0xFF1B => Some(simple(0x01)),       // Escape
        0xFFFF => Some(extended(0x53)),     // Delete
        0xFF63 => Some(extended(0x52)),     // Insert
        0xFF50 => Some(extended(0x47)),     // Home
        0xFF57 => Some(extended(0x4F)),     // End
        0xFF55 => Some(extended(0x49)),     // PageUp
        0xFF56 => Some(extended(0x51)),     // PageDown
        0xFF51 => Some(extended(0x4B)),     // ArrowLeft
        0xFF52 => Some(extended(0x48)),     // ArrowUp
        0xFF53 => Some(extended(0x4D)),     // ArrowRight
        0xFF54 => Some(extended(0x50)),     // ArrowDown
        0xFFE1 => Some(simple(LEFT_SHIFT)), // Shift
        0xFFE3 => Some(simple(0x1D)),       // Control
        0xFFE9 => Some(simple(0x38)),       // Alt
        0xFFEA => Some(extended(0x38)),     // AltGraph (right Alt)
        0xFFEB => Some(extended(0x5B)),     // Super/Meta (left GUI)
        0xFFE5 => Some(simple(0x3A)),       // CapsLock
        0xFFBE => Some(simple(0x3B)),       // F1
        0xFFBF => Some(simple(0x3C)),       // F2
        0xFFC0 => Some(simple(0x3D)),       // F3
        0xFFC1 => Some(simple(0x3E)),       // F4
        0xFFC2 => Some(simple(0x3F)),       // F5
        0xFFC3 => Some(simple(0x40)),       // F6
        0xFFC4 => Some(simple(0x41)),       // F7
        0xFFC5 => Some(simple(0x42)),       // F8
        0xFFC6 => Some(simple(0x43)),       // F9
        0xFFC7 => Some(simple(0x44)),       // F10
        0xFFC8 => Some(simple(0x57)),       // F11
        0xFFC9 => Some(simple(0x58)),       // F12

        // Latin-1 passthrough (server-remote/src/ui/connection.rs
        // character_keysym()) — keysym == char code point for 0x00..=0xFF.
        code @ 0x00..=0xFF => ascii_to_scancode(code as u8),

        _ => None,
    }
}

/// US QWERTY layout: unshifted and shifted printable ASCII to scan codes.
fn ascii_to_scancode(code: u8) -> Option<KeyTranslation> {
    let ch = code as char;
    match ch {
        ' ' => Some(simple(0x39)),
        '0'..='9' => {
            // '1'..'9' are contiguous at 0x02..0x0A; '0' wraps to 0x0B.
            let code = if ch == '0' {
                0x0B
            } else {
                0x02 + (ch as u8 - b'1')
            };
            Some(simple(code))
        }
        'a'..='z' => Some(simple(letter_scancode(ch.to_ascii_uppercase()))),
        'A'..='Z' => Some(simple_shifted(letter_scancode(ch))),
        '-' => Some(simple(0x0C)),
        '_' => Some(simple_shifted(0x0C)),
        '=' => Some(simple(0x0D)),
        '+' => Some(simple_shifted(0x0D)),
        '[' => Some(simple(0x1A)),
        '{' => Some(simple_shifted(0x1A)),
        ']' => Some(simple(0x1B)),
        '}' => Some(simple_shifted(0x1B)),
        ';' => Some(simple(0x27)),
        ':' => Some(simple_shifted(0x27)),
        '\'' => Some(simple(0x28)),
        '"' => Some(simple_shifted(0x28)),
        '`' => Some(simple(0x29)),
        '~' => Some(simple_shifted(0x29)),
        '\\' => Some(simple(0x2B)),
        '|' => Some(simple_shifted(0x2B)),
        ',' => Some(simple(0x33)),
        '<' => Some(simple_shifted(0x33)),
        '.' => Some(simple(0x34)),
        '>' => Some(simple_shifted(0x34)),
        '/' => Some(simple(0x35)),
        '?' => Some(simple_shifted(0x35)),
        '!' => Some(simple_shifted(0x02)),
        '@' => Some(simple_shifted(0x03)),
        '#' => Some(simple_shifted(0x04)),
        '$' => Some(simple_shifted(0x05)),
        '%' => Some(simple_shifted(0x06)),
        '^' => Some(simple_shifted(0x07)),
        '&' => Some(simple_shifted(0x08)),
        '*' => Some(simple_shifted(0x09)),
        '(' => Some(simple_shifted(0x0A)),
        ')' => Some(simple_shifted(0x0B)),
        _ => None,
    }
}

/// Scan code for an uppercase QWERTY letter key (`'A'..='Z'`).
fn letter_scancode(upper: char) -> u8 {
    match upper {
        'Q' => 0x10,
        'W' => 0x11,
        'E' => 0x12,
        'R' => 0x13,
        'T' => 0x14,
        'Y' => 0x15,
        'U' => 0x16,
        'I' => 0x17,
        'O' => 0x18,
        'P' => 0x19,
        'A' => 0x1E,
        'S' => 0x1F,
        'D' => 0x20,
        'F' => 0x21,
        'G' => 0x22,
        'H' => 0x23,
        'J' => 0x24,
        'K' => 0x25,
        'L' => 0x26,
        'Z' => 0x2C,
        'X' => 0x2D,
        'C' => 0x2E,
        'V' => 0x2F,
        'B' => 0x30,
        'N' => 0x31,
        'M' => 0x32,
        _ => unreachable!("letter_scancode called with a non-letter"),
    }
}

/// Applies press/release Set-1 encoding: `Simple` codes get the `0x80` break
/// bit on release; `Extended` codes are preceded by `0xE0` on the wire (both
/// press and release), per `crate::wire`'s key-event writer.
pub fn make_code_byte(code: u8, pressed: bool) -> u8 {
    if pressed { code } else { code | 0x80 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_letter_maps_to_expected_scancode() {
        assert_eq!(
            keysym_to_scancode(0x61), // 'a'
            Some(simple(0x1E))
        );
    }

    #[test]
    fn uppercase_letter_requires_shift() {
        assert_eq!(
            keysym_to_scancode(0x41), // 'A'
            Some(simple_shifted(0x1E))
        );
    }

    #[test]
    fn shifted_symbol_requires_shift_on_the_base_key() {
        // '!' (0x21) is Shift+1, base key scan code 0x02.
        assert_eq!(keysym_to_scancode(0x21), Some(simple_shifted(0x02)));
    }

    #[test]
    fn digit_and_unshifted_symbol_do_not_require_shift() {
        assert_eq!(keysym_to_scancode('5' as u32), Some(simple(0x06)));
        assert_eq!(keysym_to_scancode('-' as u32), Some(simple(0x0C)));
    }

    #[test]
    fn enter_tab_escape_backspace_map_to_known_set1_codes() {
        assert_eq!(keysym_to_scancode(0xFF0D), Some(simple(0x1C))); // Enter
        assert_eq!(keysym_to_scancode(0xFF09), Some(simple(0x0F))); // Tab
        assert_eq!(keysym_to_scancode(0xFF1B), Some(simple(0x01))); // Escape
        assert_eq!(keysym_to_scancode(0xFF08), Some(simple(0x0E))); // Backspace
    }

    #[test]
    fn arrow_keys_and_delete_are_extended() {
        for keysym in [
            0xFF51u32, 0xFF52, 0xFF53, 0xFF54, 0xFFFF, 0xFF63, 0xFF50, 0xFF57, 0xFF55, 0xFF56,
        ] {
            match keysym_to_scancode(keysym) {
                Some(KeyTranslation {
                    code: ScanCode::Extended(_),
                    ..
                }) => {}
                other => panic!("keysym {keysym:#X} expected Extended, got {other:?}"),
            }
        }
    }

    #[test]
    fn unmapped_keysym_returns_none() {
        // Beyond Latin-1 (server-remote's Unicode keysym form) and not a
        // named key this table knows about.
        assert_eq!(keysym_to_scancode(0x0100_0000 + '€' as u32), None);
        assert_eq!(keysym_to_scancode(0xFF61), None); // PrintScreen, not mapped
    }

    #[test]
    fn press_release_encoding_sets_break_bit_only_on_release() {
        assert_eq!(make_code_byte(0x1E, true), 0x1E);
        assert_eq!(make_code_byte(0x1E, false), 0x9E);
    }
}
