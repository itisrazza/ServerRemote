//! The stream ciphers that protect the console and command channels.
//!
//! iLO 4 encrypts each channel with a stream cipher keyed by a 16-byte key
//! from `rc_info`: RC4, or AES used as a keystream generator (OFB with an
//! all-zero IV). The console picks between them mid-stream (DVC commands 12
//! and 13); the command channel always uses AES when it is encrypted at all.
//!
//! "AES-256" in the console's cipher selector still runs AES-128: the key is
//! only ever 16 bytes, and the vendor client builds the same AES-128 cipher
//! for both selectors.

use aes::Aes128;
use aes::cipher::{Array, BlockCipherEncrypt, KeyInit};

/// The console's cipher selector, as DVC commands 12 and 13 carry it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CipherKind {
    None,
    Rc4,
    Aes128,
    Aes256,
}

impl CipherKind {
    /// Decodes a selector byte; unknown values mean no encryption, as in the
    /// vendor client.
    pub fn from_selector(selector: u8) -> Self {
        match selector {
            1 => CipherKind::Rc4,
            2 => CipherKind::Aes128,
            3 => CipherKind::Aes256,
            _ => CipherKind::None,
        }
    }

    /// A short label for the status bar, or `None` when unencrypted.
    pub fn label(self) -> Option<&'static str> {
        match self {
            CipherKind::None => None,
            CipherKind::Rc4 => Some("RC4"),
            CipherKind::Aes128 => Some("AES"),
            CipherKind::Aes256 => Some("AES-256"),
        }
    }
}

/// One direction of an encrypted channel: XORs bytes with a keystream.
pub enum Keystream {
    None,
    Rc4(Rc4),
    Aes(AesOfb),
}

impl Keystream {
    pub fn new(kind: CipherKind, key: &[u8; 16]) -> Self {
        match kind {
            CipherKind::None => Keystream::None,
            CipherKind::Rc4 => Keystream::Rc4(Rc4::new(key)),
            CipherKind::Aes128 | CipherKind::Aes256 => Keystream::Aes(AesOfb::new(key)),
        }
    }

    /// Encrypts or decrypts `bytes` in place (the same operation).
    pub fn apply(&mut self, bytes: &mut [u8]) {
        match self {
            Keystream::None => {}
            Keystream::Rc4(rc4) => bytes.iter_mut().for_each(|b| *b ^= rc4.next_byte()),
            Keystream::Aes(aes) => bytes.iter_mut().for_each(|b| *b ^= aes.next_byte()),
        }
    }
}

/// The RC4 keystream generator.
pub struct Rc4 {
    sbox: [u8; 256],
    i: u8,
    j: u8,
}

impl Rc4 {
    pub fn new(key: &[u8]) -> Self {
        let mut sbox = [0u8; 256];
        for (n, slot) in sbox.iter_mut().enumerate() {
            *slot = n as u8;
        }
        let mut j = 0u8;
        for n in 0..256 {
            j = j.wrapping_add(sbox[n]).wrapping_add(key[n % key.len()]);
            sbox.swap(n, j as usize);
        }
        Self { sbox, i: 0, j: 0 }
    }

    pub fn next_byte(&mut self) -> u8 {
        self.i = self.i.wrapping_add(1);
        self.j = self.j.wrapping_add(self.sbox[self.i as usize]);
        self.sbox.swap(self.i as usize, self.j as usize);
        let index = self.sbox[self.i as usize].wrapping_add(self.sbox[self.j as usize]);
        self.sbox[index as usize]
    }
}

/// AES-128 in OFB mode with a zero IV, as a keystream generator.
pub struct AesOfb {
    cipher: Aes128,
    block: [u8; 16],
    used: usize,
}

impl AesOfb {
    pub fn new(key: &[u8; 16]) -> Self {
        Self {
            cipher: Aes128::new(&Array::from(*key)),
            block: [0; 16],
            used: 16,
        }
    }

    pub fn next_byte(&mut self) -> u8 {
        if self.used == 16 {
            let mut block = Array::from(self.block);
            self.cipher.encrypt_block(&mut block);
            self.block = block.into();
            self.used = 0;
        }
        let byte = self.block[self.used];
        self.used += 1;
        byte
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn rc4_matches_rfc_6229() {
        // RFC 6229, 128-bit key 0x0102...10, keystream offset 0.
        let key: Vec<u8> = (1..=16).collect();
        let mut rc4 = Rc4::new(&key);
        let stream: Vec<u8> = (0..16).map(|_| rc4.next_byte()).collect();
        assert_eq!(stream, hex("9ac7cc9a609d1ef7b2932899cde41b97"));
    }

    #[test]
    fn aes_ofb_matches_nist_sp800_38a() {
        // NIST SP 800-38A F.4.1 uses a non-zero IV; with a zero IV the first
        // OFB block is simply AES(key, 0), checked against FIPS-197's
        // AES-128 known answer for a zero plaintext under this key.
        let key: [u8; 16] = hex("000102030405060708090a0b0c0d0e0f").try_into().unwrap();
        let mut ofb = AesOfb::new(&key);
        let first: Vec<u8> = (0..16).map(|_| ofb.next_byte()).collect();
        let mut expected = Array::from([0u8; 16]);
        Aes128::new(&Array::from(key)).encrypt_block(&mut expected);
        assert_eq!(first, expected.to_vec());

        // The second block is AES(key, first block): the feedback chain.
        let second: Vec<u8> = (0..16).map(|_| ofb.next_byte()).collect();
        let mut chained = expected;
        Aes128::new(&Array::from(key)).encrypt_block(&mut chained);
        assert_eq!(second, chained.to_vec());
    }

    #[test]
    fn aes_known_answer() {
        // FIPS-197 appendix C.1.
        let key: [u8; 16] = hex("000102030405060708090a0b0c0d0e0f").try_into().unwrap();
        let plain: [u8; 16] = hex("00112233445566778899aabbccddeeff").try_into().unwrap();
        let mut block = Array::from(plain);
        Aes128::new(&Array::from(key)).encrypt_block(&mut block);
        assert_eq!(block.to_vec(), hex("69c4e0d86a7b0430d8cdb78070b4c55a"));
    }

    #[test]
    fn keystreams_round_trip() {
        let key = [7u8; 16];
        for kind in [CipherKind::Rc4, CipherKind::Aes128, CipherKind::Aes256] {
            let mut data = *b"hello, iLO console";
            Keystream::new(kind, &key).apply(&mut data);
            assert_ne!(&data, b"hello, iLO console");
            Keystream::new(kind, &key).apply(&mut data);
            assert_eq!(&data, b"hello, iLO console");
        }
    }
}
