//! Keyboard and mouse messages for the KVM connection. See section 4 of
//! `docs/protocols/ilo4.md`.
//!
//! The iLO takes USB HID boot-keyboard reports: the full set of keys held
//! down, not key events. The UI sends X11 keysyms, so each is mapped to the
//! key that produces it on a US keyboard; shifted symbols map to their
//! unshifted key, because the Shift key itself is reported separately.

use server_remote_core::PointerButton;

/// The keys currently held, as a HID report tracks them.
#[derive(Debug, Default)]
pub struct Keyboard {
    modifiers: u8,
    keys: Vec<u8>,
}

/// HID boot reports carry at most six keys besides the modifiers.
const MAX_KEYS: usize = 6;

/// HID usage of the Delete key, for Ctrl+Alt+Del.
pub const HID_DELETE: u8 = 0x4C;
/// Left Ctrl and left Alt in the modifier byte.
pub const MODIFIERS_CTRL_ALT: u8 = 0x05;

impl Keyboard {
    /// Applies a key event. Returns the report to send, or `None` if the key
    /// has no HID equivalent or the event changes nothing.
    pub fn key(&mut self, keysym: u32, pressed: bool) -> Option<[u8; 10]> {
        if let Some(bit) = modifier_bit(keysym) {
            let before = self.modifiers;
            if pressed {
                self.modifiers |= bit;
            } else {
                self.modifiers &= !bit;
            }
            return (before != self.modifiers).then(|| self.report());
        }
        let usage = hid_usage(keysym)?;
        let held = self.keys.contains(&usage);
        if pressed && !held && self.keys.len() < MAX_KEYS {
            self.keys.push(usage);
        } else if !pressed && held {
            self.keys.retain(|&key| key != usage);
        } else {
            return None;
        }
        Some(self.report())
    }

    /// Forgets every held key and returns the all-up report.
    pub fn release_all(&mut self) -> [u8; 10] {
        self.modifiers = 0;
        self.keys.clear();
        self.report()
    }

    pub fn report(&self) -> [u8; 10] {
        report(self.modifiers, &self.keys)
    }
}

/// A keyboard message: type 1, modifiers, then up to six HID usages.
pub fn report(modifiers: u8, keys: &[u8]) -> [u8; 10] {
    let mut message = [0u8; 10];
    message[0] = 0x01;
    message[2] = modifiers;
    for (slot, key) in message[4..].iter_mut().zip(keys) {
        *slot = *key;
    }
    message
}

fn modifier_bit(keysym: u32) -> Option<u8> {
    Some(match keysym {
        0xFFE3 => 0x01,                   // Control_L
        0xFFE1 => 0x02,                   // Shift_L
        0xFFE9 | 0xFFE7 => 0x04,          // Alt_L, Meta_L
        0xFFEB => 0x08,                   // Super_L
        0xFFE4 => 0x10,                   // Control_R
        0xFFE2 => 0x20,                   // Shift_R
        0xFFEA | 0xFE03 | 0xFFE8 => 0x40, // Alt_R, ISO_Level3_Shift, Meta_R
        0xFFEC => 0x80,                   // Super_R
        _ => return None,
    })
}

/// The HID usage of the US-layout key that types `keysym`.
fn hid_usage(keysym: u32) -> Option<u8> {
    let usage = match keysym {
        0x61..=0x7A => 0x04 + (keysym - 0x61) as u8, // a–z
        0x41..=0x5A => 0x04 + (keysym - 0x41) as u8, // A–Z
        0x31..=0x39 => 0x1E + (keysym - 0x31) as u8, // 1–9
        0x30 => 0x27,
        0x21 => 0x1E, // !
        0x40 => 0x1F, // @
        0x23 => 0x20, // #
        0x24 => 0x21, // $
        0x25 => 0x22, // %
        0x5E => 0x23, // ^
        0x26 => 0x24, // &
        0x2A => 0x25, // *
        0x28 => 0x26, // (
        0x29 => 0x27, // )
        0x20 => 0x2C,
        0x2D | 0x5F => 0x2D,                               // - _
        0x3D | 0x2B => 0x2E,                               // = +
        0x5B | 0x7B => 0x2F,                               // [ {
        0x5D | 0x7D => 0x30,                               // ] }
        0x5C | 0x7C => 0x31,                               // \ |
        0x3B | 0x3A => 0x33,                               // ; :
        0x27 | 0x22 => 0x34,                               // ' "
        0x60 | 0x7E => 0x35,                               // ` ~
        0x2C | 0x3C => 0x36,                               // , <
        0x2E | 0x3E => 0x37,                               // . >
        0x2F | 0x3F => 0x38,                               // / ?
        0xFF0D => 0x28,                                    // Return
        0xFF1B => 0x29,                                    // Escape
        0xFF08 => 0x2A,                                    // BackSpace
        0xFF09 => 0x2B,                                    // Tab
        0xFFE5 => 0x39,                                    // Caps_Lock
        0xFFBE..=0xFFC9 => 0x3A + (keysym - 0xFFBE) as u8, // F1–F12
        0xFF61 => 0x46,                                    // Print
        0xFF14 => 0x47,                                    // Scroll_Lock
        0xFF13 => 0x48,                                    // Pause
        0xFF63 => 0x49,                                    // Insert
        0xFF50 => 0x4A,                                    // Home
        0xFF55 => 0x4B,                                    // Prior
        0xFFFF => HID_DELETE,
        0xFF57 => 0x4D,                                    // End
        0xFF56 => 0x4E,                                    // Next
        0xFF53 => 0x4F,                                    // Right
        0xFF51 => 0x50,                                    // Left
        0xFF54 => 0x51,                                    // Down
        0xFF52 => 0x52,                                    // Up
        0xFF7F => 0x53,                                    // Num_Lock
        0xFFAF => 0x54,                                    // KP_Divide
        0xFFAA => 0x55,                                    // KP_Multiply
        0xFFAD => 0x56,                                    // KP_Subtract
        0xFFAB => 0x57,                                    // KP_Add
        0xFF8D => 0x58,                                    // KP_Enter
        0xFFB1..=0xFFB9 => 0x59 + (keysym - 0xFFB1) as u8, // KP_1–KP_9
        0xFFB0 => 0x62,                                    // KP_0
        0xFFAE => 0x63,                                    // KP_Decimal
        0xFF67 => 0x65,                                    // Menu
        _ => return None,
    };
    Some(usage)
}

/// The pointer as the iLO's mouse messages describe it.
#[derive(Debug, Default)]
pub struct Mouse {
    x: f32,
    y: f32,
    buttons: u8,
}

impl Mouse {
    /// Moves to (`x`, `y`) in framebuffer pixels on a `size` screen.
    pub fn moved(&mut self, x: f32, y: f32, size: (u32, u32)) -> [u8; 10] {
        let (dx, dy) = (x - self.x, self.y - y);
        self.x = x;
        self.y = y;
        self.report(dx, dy, size)
    }

    /// Presses or releases a button. Returns the report to send.
    pub fn button(&mut self, button: PointerButton, pressed: bool, size: (u32, u32)) -> [u8; 10] {
        let bit = match button {
            PointerButton::Left => 0x01,
            PointerButton::Right => 0x02,
            PointerButton::Middle => 0x04,
            PointerButton::Back => 0x08,
            PointerButton::Forward => 0x10,
        };
        if pressed {
            self.buttons |= bit;
        } else {
            self.buttons &= !bit;
        }
        self.report(0.0, 0.0, size)
    }

    /// Type 2: absolute position scaled to 0–3000, relative motion in
    /// sign-magnitude (positive `dy` is upwards), buttons, wheel.
    fn report(&self, dx: f32, dy: f32, (width, height): (u32, u32)) -> [u8; 10] {
        let scale = |value: f32, extent: u32| {
            if extent == 0 {
                0
            } else {
                (3000.0 * value.clamp(0.0, extent as f32) / extent as f32).round() as u16
            }
        };
        let x = scale(self.x, width).to_le_bytes();
        let y = scale(self.y, height).to_le_bytes();
        [
            0x02,
            0x00,
            x[0],
            x[1],
            y[0],
            y[1],
            sign_magnitude(dx),
            sign_magnitude(dy),
            self.buttons,
            0,
        ]
    }
}

fn sign_magnitude(delta: f32) -> u8 {
    let magnitude = delta.abs().round().min(127.0) as u8;
    if delta < 0.0 && magnitude > 0 {
        0x80 | magnitude
    } else {
        magnitude
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_a_capital_letter_reports_shift_and_the_key() {
        let mut keyboard = Keyboard::default();
        assert_eq!(keyboard.key(0xFFE1, true), Some(report(0x02, &[])));
        assert_eq!(keyboard.key(0x41, true), Some(report(0x02, &[0x04])));
        // Released after Shift, so the UI reports the lower-case keysym.
        assert_eq!(keyboard.key(0xFFE1, false), Some(report(0, &[0x04])));
        assert_eq!(keyboard.key(0x61, false), Some(report(0, &[])));
    }

    #[test]
    fn shifted_symbols_map_to_their_key() {
        assert_eq!(hid_usage(0x21), hid_usage(0x31)); // ! and 1
        assert_eq!(hid_usage(0x3F), hid_usage(0x2F)); // ? and /
        assert_eq!(hid_usage(0xFFC9), Some(0x45)); // F12
        assert_eq!(hid_usage(0x0100_20AC), None); // €
    }

    #[test]
    fn repeats_and_unknown_keys_send_nothing() {
        let mut keyboard = Keyboard::default();
        assert!(keyboard.key(0x61, true).is_some());
        assert_eq!(keyboard.key(0x61, true), None);
        assert_eq!(keyboard.key(0x0100_20AC, true), None);
        assert_eq!(keyboard.key(0x62, false), None);
    }

    #[test]
    fn at_most_six_keys_are_reported() {
        let mut keyboard = Keyboard::default();
        for keysym in 0x61..0x68 {
            keyboard.key(keysym, true);
        }
        assert_eq!(keyboard.report(), report(0, &[4, 5, 6, 7, 8, 9]));
        assert_eq!(keyboard.release_all(), report(0, &[]));
    }

    #[test]
    fn mouse_reports_scale_position_and_encode_motion() {
        let mut mouse = Mouse::default();
        let size = (1600, 900);
        let message = mouse.moved(800.0, 450.0, size);
        assert_eq!(&message[..2], &[0x02, 0x00]);
        assert_eq!(u16::from_le_bytes([message[2], message[3]]), 1500);
        assert_eq!(u16::from_le_bytes([message[4], message[5]]), 1500);
        assert_eq!(message[6], 127); // clamped
        assert_eq!(message[7], 0x80 | 127); // moved down: negative

        let message = mouse.moved(797.0, 455.0, size);
        assert_eq!(message[6], 0x83);
        assert_eq!(message[7], 0x85);

        let message = mouse.button(PointerButton::Right, true, size);
        assert_eq!((message[6], message[7], message[8]), (0, 0, 0x02));
    }
}
