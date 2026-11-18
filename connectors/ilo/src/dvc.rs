//! The KVM video decoder ("DVC"). See section 5 of `docs/protocols/ilo4.md`.
//!
//! The server→client KVM stream is a bit-oriented codec driven by a state
//! machine. The decoder owns the stream's receive cipher too, because the
//! cipher is chosen from inside the stream: firmware commands 12 and 13
//! restart it from the byte after the command.
//!
//! [`Decoder::feed`] takes bytes as they arrive and runs the machine until it
//! runs out of input. A step that needs more bytes than are buffered is
//! rolled back and retried on the next feed, so chunk boundaries never
//! matter.

use crate::cipher::{CipherKind, Keystream};

/// Something the session has to act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The video mode changed. `0×0` means no video.
    Mode {
        width: u32,
        height: u32,
    },
    /// A frame is complete: present the framebuffer.
    Frame,
    /// The stream switched cipher; the client's sending keystream restarts
    /// with it.
    Cipher(CipherKind),
    /// The session header arrived. The client may only send from now on.
    Header {
        license: u8,
        flags: u8,
    },
    /// A message for the user. Kinds 1 to 3 are status text, 4 a message box.
    Print {
        kind: u8,
        text: String,
    },
    PowerOn,
    PowerOff,
    /// The server asks for an acknowledgement (`0C 00`).
    AckRequested,
    /// The decoder lost sync for a long time; ask for a full refresh.
    RefreshNeeded,
    /// The server ended the session.
    Exit,
}

/// The largest mode the client draws, as in the vendor client.
const MAX_WIDTH: u32 = 2560;
const MAX_HEIGHT: u32 = 1600;
/// Latched bits after which the decoder asks for a refresh.
const LATCH_REFRESH: u32 = 32768;
const CACHE_SIZE: usize = 17;

// State numbers, as the vendor client numbers them.
const RESET: u8 = 0;
const START: u8 = 1;
const PIXELS: u8 = 2;
const PIXLRU1: u8 = 3;
const PIXLRU0: u8 = 4;
const PIXCODE1: u8 = 5;
const PIXCODE2: u8 = 6;
const PIXCODE3: u8 = 7;
const PIXGREY: u8 = 8;
const PIXRGBR: u8 = 9;
const PIXRPT: u8 = 10;
const PIXRPT1: u8 = 11;
const PIXRPTSTD1: u8 = 12;
const PIXRPTSTD2: u8 = 13;
const PIXRPTNSTD: u8 = 14;
const CMD: u8 = 15;
const CMD0: u8 = 16;
const MOVEXY0: u8 = 17;
const EXTCMD: u8 = 18;
const CMDX: u8 = 19;
const MOVESHORTX: u8 = 20;
const MOVELONGX: u8 = 21;
const BLKRPT: u8 = 22;
const EXTCMD1: u8 = 23;
const FIRMWARE: u8 = 24;
const EXTCMD2: u8 = 25;
const MODE0: u8 = 26;
const TIMEOUT: u8 = 27;
const BLKRPT1: u8 = 28;
const BLKRPTSTD: u8 = 29;
const BLKRPTNSTD: u8 = 30;
const PIXFAN: u8 = 31;
const PIXCODE4: u8 = 32;
const PIXDUP: u8 = 33;
const BLKDUP: u8 = 34;
const PIXCODE: u8 = 35;
const PIXSPEC: u8 = 36;
const EXIT: u8 = 37;
const LATCHED: u8 = 38;
const MOVEXY1: u8 = 39;
const MODE1: u8 = 40;
const PIXRGBG: u8 = 41;
const PIXRGBB: u8 = 42;
const HUNT: u8 = 43;
const PRINT0: u8 = 44;
const PRINT1: u8 = 45;
const CORP: u8 = 46;
const MODE2: u8 = 47;

/// `(bits, next if 0, next if not 0)` for each state.
const TABLE: [(u32, u8, u8); 48] = {
    let mut table = [(0, START, START); 48];
    table[RESET as usize] = (0, START, START);
    table[START as usize] = (1, PIXELS, CMD);
    table[PIXELS as usize] = (1, PIXFAN, PIXLRU1);
    table[PIXLRU1 as usize] = (1, PIXELS, PIXRPT1);
    table[PIXLRU0 as usize] = (1, PIXELS, PIXRPT1);
    table[PIXCODE1 as usize] = (1, PIXRPT, PIXRPT);
    table[PIXCODE2 as usize] = (2, PIXRPT, PIXRPT);
    table[PIXCODE3 as usize] = (3, PIXRPT, PIXRPT);
    table[PIXGREY as usize] = (4, PIXRPT, PIXRPT);
    table[PIXRGBR as usize] = (4, PIXRGBG, PIXRGBG);
    table[PIXRPT as usize] = (1, PIXELS, PIXRPT1);
    table[PIXRPT1 as usize] = (1, PIXDUP, PIXRPTSTD1);
    table[PIXRPTSTD1 as usize] = (3, PIXELS, PIXELS);
    table[PIXRPTSTD2 as usize] = (3, PIXELS, PIXELS);
    table[PIXRPTNSTD as usize] = (8, PIXELS, PIXELS);
    table[CMD as usize] = (1, CMD0, MOVEXY0);
    table[CMD0 as usize] = (1, CMDX, EXTCMD);
    table[MOVEXY0 as usize] = (7, MOVEXY1, MOVEXY1);
    table[EXTCMD as usize] = (1, BLKRPT, EXTCMD1);
    table[CMDX as usize] = (1, MOVESHORTX, MOVELONGX);
    table[MOVESHORTX as usize] = (3, START, START);
    table[MOVELONGX as usize] = (7, START, START);
    table[BLKRPT as usize] = (1, BLKDUP, BLKRPT1);
    table[EXTCMD1 as usize] = (1, EXTCMD2, FIRMWARE);
    table[FIRMWARE as usize] = (8, CORP, CORP);
    table[EXTCMD2 as usize] = (1, MODE0, TIMEOUT);
    table[MODE0 as usize] = (7, MODE1, MODE1);
    table[TIMEOUT as usize] = (0, START, START);
    table[BLKRPT1 as usize] = (1, BLKRPTSTD, BLKRPTNSTD);
    table[BLKRPTSTD as usize] = (3, START, START);
    table[BLKRPTNSTD as usize] = (7, START, START);
    table[PIXFAN as usize] = (1, PIXSPEC, PIXCODE);
    table[PIXCODE4 as usize] = (4, PIXRPT, PIXRPT);
    table[PIXDUP as usize] = (0, PIXELS, PIXELS);
    table[BLKDUP as usize] = (0, START, START);
    table[PIXCODE as usize] = (0, PIXCODE, PIXCODE);
    table[PIXSPEC as usize] = (1, PIXGREY, PIXRGBR);
    table[EXIT as usize] = (0, EXIT, EXIT);
    table[LATCHED as usize] = (1, LATCHED, LATCHED);
    table[MOVEXY1 as usize] = (7, START, START);
    table[MODE1 as usize] = (7, MODE2, MODE2);
    table[PIXRGBG as usize] = (4, PIXRGBB, PIXRGBB);
    table[PIXRGBB as usize] = (4, PIXRPT, PIXRPT);
    table[HUNT as usize] = (1, HUNT, RESET);
    table[PRINT0 as usize] = (8, PRINT1, PRINT1);
    table[PRINT1 as usize] = (8, PRINT1, PRINT1);
    table[CORP as usize] = (1, START, FIRMWARE);
    table[MODE2 as usize] = (4, START, START);
    table
};

/// The recently used colours, each with an LRU rank (0 = most recent) and a
/// block counter that forgets colours unused for a whole block.
#[derive(Default)]
struct ColourCache {
    entries: Vec<CacheEntry>,
}

struct CacheEntry {
    colour: u32,
    rank: u32,
    counter: u8,
}

impl ColourCache {
    fn reset(&mut self) {
        self.entries.clear();
    }

    /// The state the ≠0 branch of `PIXFAN` goes to for this cache size.
    fn pixcode_state(&self) -> u8 {
        match self.entries.len() {
            0 | 1 => LATCHED,
            2 => PIXLRU0,
            3 => PIXCODE1,
            4 | 5 => PIXCODE2,
            6..=9 => PIXCODE3,
            _ => PIXCODE4,
        }
    }

    /// Adds an explicit colour at rank 0. A colour already in the cache is a
    /// protocol error; it is still promoted, and `false` is returned.
    fn insert(&mut self, colour: u32) -> bool {
        if let Some(index) = self.entries.iter().position(|e| e.colour == colour) {
            self.promote(index);
            return false;
        }
        if self.entries.len() < CACHE_SIZE {
            for entry in &mut self.entries {
                entry.rank += 1;
            }
            self.entries.push(CacheEntry {
                colour,
                rank: 0,
                counter: 1,
            });
        } else {
            let victim = (0..self.entries.len())
                .max_by_key(|&i| self.entries[i].rank)
                .unwrap_or_default();
            self.promote(victim);
            self.entries[victim].colour = colour;
        }
        true
    }

    /// The colour with `rank`, moved to rank 0.
    fn find(&mut self, rank: u32) -> Option<u32> {
        let index = self.entries.iter().position(|e| e.rank == rank)?;
        self.promote(index);
        Some(self.entries[index].colour)
    }

    fn promote(&mut self, index: usize) {
        let old = self.entries[index].rank;
        for entry in &mut self.entries {
            if entry.rank < old {
                entry.rank += 1;
            }
        }
        self.entries[index].rank = 0;
        self.entries[index].counter = 1;
    }

    fn prune(&mut self) {
        self.entries.retain_mut(|entry| {
            if entry.counter == 0 {
                false
            } else {
                entry.counter -= 1;
                true
            }
        });
    }
}

/// What reading bits from the stream gave.
enum Read {
    Bits(u32),
    /// More than 30 zero bits: the stream resynchronised.
    Resync,
    /// Not enough input buffered yet.
    Starved,
}

pub struct Decoder {
    key: [u8; 16],
    cipher: Keystream,
    /// Received bytes not yet discarded, as received and as decrypted.
    raw: Vec<u8>,
    plain: Vec<u8>,
    /// The next byte of `plain` to read.
    pos: usize,
    acc: u32,
    bit_count: u32,
    zero_run: u32,

    state: u8,
    steps: u64,
    last_timeout: Option<u64>,
    latched_bits: u32,
    exited: bool,

    bpc: u32,
    half_height_allowed: bool,
    block_height: u32,
    cache: ColourCache,
    block: [u32; 256],
    pixels: usize,
    last_colour: u32,
    pending_red: u32,
    pending_green: u32,
    x: u32,
    y: u32,
    new_x: u32,
    blocks_wide: u32,
    blocks_high: u32,

    command: Vec<u8>,
    print_kind: u8,
    print_text: Vec<u8>,

    width: u32,
    height: u32,
    /// RGBA8, `width * height * 4` bytes.
    framebuffer: Vec<u8>,
}

impl Decoder {
    /// A decoder for a fresh KVM stream, which starts unencrypted.
    pub fn new(key: [u8; 16]) -> Self {
        Self {
            key,
            cipher: Keystream::None,
            raw: Vec::new(),
            plain: Vec::new(),
            pos: 0,
            acc: 0,
            bit_count: 0,
            zero_run: 0,
            state: RESET,
            steps: 0,
            last_timeout: None,
            latched_bits: 0,
            exited: false,
            bpc: 5,
            half_height_allowed: false,
            block_height: 16,
            cache: ColourCache::default(),
            block: [0; 256],
            pixels: 0,
            last_colour: 0,
            pending_red: 0,
            pending_green: 0,
            x: 0,
            y: 0,
            new_x: 0,
            blocks_wide: 0,
            blocks_high: 0,
            command: Vec::new(),
            print_kind: 0,
            print_text: Vec::new(),
            width: 0,
            height: 0,
            framebuffer: Vec::new(),
        }
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// The decoded screen: RGBA8, row-major.
    pub fn framebuffer(&self) -> &[u8] {
        &self.framebuffer
    }

    /// Takes newly received bytes and decodes as far as they go.
    pub fn feed(&mut self, data: &[u8], events: &mut Vec<Event>) {
        let start = self.raw.len();
        self.raw.extend_from_slice(data);
        self.plain.extend_from_slice(data);
        self.cipher.apply(&mut self.plain[start..]);

        while !self.exited && self.step(events) {}

        // Drop what has been read, keeping the buffers small.
        if self.pos > 0 {
            self.raw.drain(..self.pos);
            self.plain.drain(..self.pos);
            self.pos = 0;
        }
    }

    /// Restarts the receive cipher from the next unread byte.
    fn set_cipher(&mut self, kind: CipherKind) {
        self.cipher = Keystream::new(kind, &self.key);
        let pos = self.pos;
        self.plain.truncate(pos);
        self.plain.extend_from_slice(&self.raw[pos..]);
        self.cipher.apply(&mut self.plain[pos..]);
    }

    fn read(&mut self, n: u32) -> Read {
        if n == 0 {
            return Read::Bits(0);
        }
        if self.bit_count < n {
            let Some(&byte) = self.plain.get(self.pos) else {
                return Read::Starved;
            };
            self.pos += 1;
            self.zero_run += if byte == 0 { 8 } else { byte.trailing_zeros() };
            if self.zero_run > 30 {
                // Skip the zero bytes and the non-zero byte that ends them.
                let mut byte = byte;
                while byte == 0 {
                    let Some(&next) = self.plain.get(self.pos) else {
                        return Read::Starved;
                    };
                    self.pos += 1;
                    byte = next;
                }
                self.acc = 0;
                self.bit_count = 0;
                self.zero_run = byte.trailing_zeros();
                return Read::Resync;
            }
            if byte != 0 {
                self.zero_run = byte.leading_zeros();
            }
            self.acc |= u32::from(byte) << self.bit_count;
            self.bit_count += 8;
        }
        let value = self.acc & ((1 << n) - 1);
        self.acc >>= n;
        self.bit_count -= n;
        // The first bit taken is the field's most significant.
        Read::Bits(value.reverse_bits() >> (32 - n))
    }

    fn align_to_byte(&mut self) {
        let extra = self.bit_count & 7;
        self.acc >>= extra;
        self.bit_count -= extra;
    }

    fn bits_for(&self, state: u8) -> u32 {
        match state {
            PIXGREY | PIXRGBR | PIXRGBG | PIXRGBB => self.bpc,
            MOVELONGX | MOVEXY0 | MOVEXY1 | BLKRPTNSTD if self.block_height == 8 => 8,
            _ => TABLE[state as usize].0,
        }
    }

    /// Runs one step. Returns `false` when it needs more input.
    fn step(&mut self, events: &mut Vec<Event>) -> bool {
        let saved = (self.pos, self.acc, self.bit_count, self.zero_run);
        let state = self.state;
        let code = match self.read(self.bits_for(state)) {
            Read::Bits(code) => code,
            Read::Resync => {
                self.state = RESET;
                return true;
            }
            Read::Starved => {
                (self.pos, self.acc, self.bit_count, self.zero_run) = saved;
                return false;
            }
        };

        let (_, if_zero, if_set) = TABLE[state as usize];
        let mut next = if code == 0 { if_zero } else { if_set };
        if state == PIXFAN && code != 0 {
            next = self.cache.pixcode_state();
        }

        match state {
            RESET => {
                self.cache.reset();
                self.pixels = 0;
                self.x = 0;
                self.y = 0;
            }
            MOVESHORTX => self.x = (self.x + code + 1) & 0x7F,
            MOVELONGX => self.x = code,
            MOVEXY0 | MODE0 => self.new_x = code,
            MOVEXY1 => {
                self.x = self.new_x;
                self.y = code;
            }
            PIXLRU1 | PIXLRU0 | PIXCODE1 | PIXCODE2 | PIXCODE3 | PIXCODE4 => {
                let rank = if self.cache.entries.len() == 1 {
                    self.cache.entries[0].rank
                } else {
                    match state {
                        PIXLRU0 => 0,
                        PIXLRU1 => 1,
                        _ if code == 0 => 0,
                        _ => code + 1,
                    }
                };
                match self.cache.find(rank) {
                    Some(colour) => self.store(colour, 1),
                    None => next = LATCHED,
                }
            }
            PIXGREY => {
                let colour = (code << (2 * self.bpc)) | (code << self.bpc) | code;
                if !self.explicit(colour) {
                    next = LATCHED;
                }
            }
            PIXRGBR => self.pending_red = code,
            PIXRGBG => self.pending_green = code,
            PIXRGBB => {
                let colour =
                    (self.pending_red << (2 * self.bpc)) | (self.pending_green << self.bpc) | code;
                if !self.explicit(colour) {
                    next = LATCHED;
                }
            }
            PIXRPTSTD1 => match code {
                7 => next = PIXRPTNSTD,
                6 => next = PIXRPTSTD2,
                _ => self.store(self.last_colour, code as usize + 2),
            },
            PIXRPTSTD2 => self.store(self.last_colour, code as usize + 8),
            PIXRPTNSTD => self.store(self.last_colour, code as usize),
            PIXDUP => self.store(self.last_colour, 1),
            TIMEOUT => {
                if self.last_timeout == Some(self.steps.wrapping_sub(1)) {
                    next = LATCHED;
                }
                self.align_to_byte();
                self.last_timeout = Some(self.steps);
            }
            FIRMWARE => self.command.push(code as u8),
            CORP if code == 0 => {
                let command = std::mem::take(&mut self.command);
                if let Some(state) = self.firmware(&command, events) {
                    next = state;
                }
            }
            PRINT0 => {
                self.print_kind = code as u8;
                self.print_text.clear();
            }
            PRINT1 => {
                if code == 0 {
                    events.push(Event::Print {
                        kind: self.print_kind,
                        text: String::from_utf8_lossy(&self.print_text).into_owned(),
                    });
                    next = START;
                } else {
                    self.print_text.push(code as u8);
                }
            }
            LATCHED => {
                self.latched_bits += 1;
                if self.latched_bits >= LATCH_REFRESH {
                    self.latched_bits = 0;
                    events.push(Event::RefreshNeeded);
                }
            }
            BLKDUP => self.next_blocks(1),
            BLKRPTSTD => self.next_blocks(code + 2),
            BLKRPTNSTD => self.next_blocks(code),
            MODE1 => {
                self.blocks_wide = self.new_x;
                self.blocks_high = code;
            }
            MODE2 => {
                self.x = 0;
                self.y = 0;
                self.pixels = 0;
                self.cache.reset();
                let width = self.blocks_wide * 16;
                let height = self.blocks_high * 16 + code;
                self.block_height = if width > 1616 && self.half_height_allowed {
                    8
                } else {
                    16
                };
                self.set_mode(width, height, events);
            }
            EXIT => {
                self.exited = true;
                events.push(Event::Exit);
            }
            _ => {}
        }
        if state != LATCHED {
            self.latched_bits = 0;
        }

        let block_pixels = 16 * self.block_height as usize;
        if next == PIXELS && self.pixels >= block_pixels {
            self.next_blocks(1);
            self.cache.prune();
            next = START;
        }
        self.state = next;
        self.steps += 1;
        true
    }

    /// Runs a firmware command (`bytes` ends with its opcode). Returns a
    /// state that overrides the next one.
    fn firmware(&mut self, bytes: &[u8], events: &mut Vec<Event>) -> Option<u8> {
        let (&opcode, params) = bytes.split_last()?;
        let param = |i: usize| params.get(i).copied().unwrap_or_default();
        match opcode {
            1 => {
                self.exited = true;
                events.push(Event::Exit);
                return Some(EXIT);
            }
            2 => return Some(PRINT0),
            4 => events.push(Event::PowerOn),
            5 => {
                self.blank();
                events.push(Event::PowerOff);
                events.push(Event::Frame);
            }
            6 => self.blank(),
            9 => self.align_to_byte(),
            11 => self.bpc = 5 - u32::from(param(0) & 3),
            12 => {
                let kind = CipherKind::from_selector(param(0));
                self.set_cipher(kind);
                events.push(Event::Cipher(kind));
            }
            13 => {
                self.bpc = 5 - u32::from(param(0) & 3);
                self.half_height_allowed = param(3) & 8 != 0;
                let kind = CipherKind::from_selector(param(1));
                self.set_cipher(kind);
                events.push(Event::Cipher(kind));
                events.push(Event::Header {
                    license: param(2),
                    flags: param(3),
                });
            }
            16 => events.push(Event::AckRequested),
            128 => events.push(Event::Frame),
            _ => {}
        }
        None
    }

    /// Adds an explicit colour to the cache and emits it. `false` if the
    /// colour was already cached, which is a protocol error.
    fn explicit(&mut self, colour: u32) -> bool {
        if !self.cache.insert(colour) {
            return false;
        }
        self.store(colour, 1);
        true
    }

    fn store(&mut self, colour: u32, count: usize) {
        self.last_colour = colour;
        for _ in 0..count {
            if let Some(slot) = self.block.get_mut(self.pixels) {
                *slot = colour;
            }
            self.pixels += 1;
        }
    }

    /// Writes the block buffer at the current position `count` times.
    fn next_blocks(&mut self, count: u32) {
        let block_height = self.block_height;
        for _ in 0..count {
            if self.x < self.blocks_wide && self.y * block_height < self.height {
                self.draw_block(self.x * 16, self.y * block_height);
                self.x += 1;
            }
        }
        self.pixels = 0;
    }

    fn draw_block(&mut self, left: u32, top: u32) {
        let max = (1u32 << self.bpc) - 1;
        let scale = |value: u32| ((value & max) * 255 / max) as u8;
        for row in 0..self.block_height {
            let y = top + row;
            if y >= self.height {
                break;
            }
            for column in 0..16 {
                let x = left + column;
                if x >= self.width {
                    break;
                }
                let colour = self.block[(row * 16 + column) as usize];
                let offset = ((y * self.width + x) * 4) as usize;
                self.framebuffer[offset..offset + 4].copy_from_slice(&[
                    scale(colour >> (2 * self.bpc)),
                    scale(colour >> self.bpc),
                    scale(colour),
                    0xFF,
                ]);
            }
        }
    }

    fn set_mode(&mut self, width: u32, height: u32, events: &mut Vec<Event>) {
        let width = width.min(MAX_WIDTH);
        let height = height.min(MAX_HEIGHT);
        if (width, height) != (self.width, self.height) {
            self.width = width;
            self.height = height;
            self.framebuffer = vec![0; (width * height * 4) as usize];
            self.blank();
            events.push(Event::Mode { width, height });
        }
    }

    fn blank(&mut self) {
        for pixel in self.framebuffer.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[0, 0, 0, 0xFF]);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Writes DVC bit fields the way the decoder reads them: each field's
    /// most significant bit first, packed into bytes least significant bit
    /// first.
    #[derive(Default)]
    pub struct BitWriter {
        bytes: Vec<u8>,
        bits: u32,
    }

    impl BitWriter {
        pub fn put(&mut self, value: u32, n: u32) -> &mut Self {
            for i in (0..n).rev() {
                if self.bits % 8 == 0 {
                    self.bytes.push(0);
                }
                if value >> i & 1 != 0 {
                    *self.bytes.last_mut().unwrap() |= 1 << (self.bits % 8);
                }
                self.bits += 1;
            }
            self
        }

        /// A firmware command: each byte followed by its "more" bit.
        pub fn firmware(&mut self, bytes: &[u8]) -> &mut Self {
            // START→CMD→CMD0→EXTCMD→EXTCMD1→FIRMWARE: 1,0,1,1,1.
            self.put(0b10111, 5);
            for (i, &byte) in bytes.iter().enumerate() {
                self.put(u32::from(byte), 8);
                self.put(u32::from(i + 1 < bytes.len()), 1);
            }
            self
        }

        /// Sets the mode to `blocks_wide`×`blocks_high` 16-pixel blocks.
        pub fn mode(&mut self, blocks_wide: u32, blocks_high: u32) -> &mut Self {
            // START→CMD→CMD0→EXTCMD→EXTCMD1→EXTCMD2→MODE0: 1,0,1,1,0,0.
            self.put(0b101100, 6)
                .put(blocks_wide, 7)
                .put(blocks_high, 7)
                .put(0, 4)
        }

        /// One block of a single explicit 15-bit colour, then the block
        /// written at (`x`, `y`).
        pub fn solid_block(&mut self, x: u32, y: u32, rgb: (u32, u32, u32)) -> &mut Self {
            // START→CMD→MOVEXY0 (1,1), x, y.
            self.put(0b11, 2).put(x, 7).put(y, 7);
            // START→PIXELS→PIXFAN→PIXSPEC→PIXRGBR (0,0,0,1), r, g, b.
            self.put(0b0001, 4)
                .put(rgb.0, 5)
                .put(rgb.1, 5)
                .put(rgb.2, 5);
            // PIXRPT→PIXRPT1→PIXRPTSTD1 (1,1) code 7 → PIXRPTNSTD: 255 more.
            self.put(0b11, 2).put(7, 3).put(255, 8);
            // PIXELS with a full block writes it and returns to START.
            self
        }

        /// End of frame.
        pub fn frame(&mut self) -> &mut Self {
            self.firmware(&[128])
        }

        pub fn bytes(&self) -> Vec<u8> {
            self.bytes.clone()
        }
    }

    /// The 7-byte session header firmware 2.82 sent: 5-bit colour, RC4.
    pub const HEADER: [u8; 7] = [0x1d, 0x20, 0x60, 0xf0, 0x90, 0x61, 0xb5];

    /// A resync marker: more than 30 zero bits, then a non-zero byte.
    pub const RESYNC: [u8; 6] = [0, 0, 0, 0, 0, 0x80];

    #[test]
    fn decodes_the_live_session_header() {
        let mut decoder = Decoder::new([0; 16]);
        let mut events = Vec::new();
        decoder.feed(&HEADER, &mut events);
        assert_eq!(
            events,
            [
                Event::Cipher(CipherKind::Rc4),
                Event::Header {
                    license: 7,
                    flags: 9
                }
            ]
        );
    }

    fn encrypted_video(key: [u8; 16], kind: CipherKind) -> Vec<u8> {
        let mut video = BitWriter::default();
        video
            .mode(2, 1)
            .solid_block(0, 0, (31, 0, 0))
            .solid_block(1, 0, (0, 0, 31))
            .frame();
        let mut body = RESYNC.to_vec();
        body.extend(video.bytes());
        Keystream::new(kind, &key).apply(&mut body);
        let mut stream = HEADER.to_vec();
        stream.extend(body);
        stream
    }

    fn pixel(decoder: &Decoder, x: u32, y: u32) -> [u8; 4] {
        let offset = ((y * decoder.size().0 + x) * 4) as usize;
        decoder.framebuffer()[offset..offset + 4]
            .try_into()
            .unwrap()
    }

    #[test]
    fn decodes_blocks_after_the_cipher_switch() {
        let key = [3; 16];
        let stream = encrypted_video(key, CipherKind::Rc4);
        let mut decoder = Decoder::new(key);
        let mut events = Vec::new();
        decoder.feed(&stream, &mut events);

        assert!(events.contains(&Event::Mode {
            width: 32,
            height: 16
        }));
        assert_eq!(events.last(), Some(&Event::Frame));
        assert_eq!(pixel(&decoder, 0, 0), [255, 0, 0, 255]);
        assert_eq!(pixel(&decoder, 15, 15), [255, 0, 0, 255]);
        assert_eq!(pixel(&decoder, 16, 0), [0, 0, 255, 255]);
        assert_eq!(pixel(&decoder, 31, 15), [0, 0, 255, 255]);
    }

    #[test]
    fn chunk_boundaries_do_not_matter() {
        let key = [9; 16];
        let stream = encrypted_video(key, CipherKind::Rc4);
        let mut whole = Decoder::new(key);
        let mut expected = Vec::new();
        whole.feed(&stream, &mut expected);

        let mut split = Decoder::new(key);
        let mut events = Vec::new();
        for byte in &stream {
            split.feed(std::slice::from_ref(byte), &mut events);
        }
        assert_eq!(events, expected);
        assert_eq!(split.framebuffer(), whole.framebuffer());
    }

    #[test]
    fn a_repeated_explicit_colour_latches_until_resync() {
        let mut video = BitWriter::default();
        video.mode(1, 1);
        // Two explicit greys of the same value: the second is an error.
        video.put(0b11, 2).put(0, 7).put(0, 7);
        video.put(0b0000, 4).put(5, 5); // PIXGREY 5
        video.put(0b0, 1); // PIXRPT: no repeat → PIXELS
        video.put(0b000, 3).put(5, 5); // PIXFAN→PIXSPEC→PIXGREY 5 again
        let mut stream = RESYNC.to_vec();
        stream.extend(video.bytes());
        let mut decoder = Decoder::new([0; 16]);
        decoder.feed(&stream, &mut Vec::new());
        assert_eq!(decoder.state, LATCHED);

        // A resync recovers.
        let mut events = Vec::new();
        let mut more = RESYNC.to_vec();
        let mut frame = BitWriter::default();
        frame.frame();
        more.extend(frame.bytes());
        decoder.feed(&more, &mut events);
        assert_eq!(events, [Event::Frame]);
    }

    #[test]
    fn print_and_power_commands_become_events() {
        let mut stream = BitWriter::default();
        stream.firmware(&[2]).put(1, 8);
        for byte in b"hi\0" {
            stream.put(u32::from(*byte), 8);
        }
        stream.firmware(&[4]).firmware(&[16]);
        let mut decoder = Decoder::new([0; 16]);
        let mut events = Vec::new();
        decoder.feed(&stream.bytes(), &mut events);
        assert_eq!(
            events,
            [
                Event::Print {
                    kind: 1,
                    text: "hi".into()
                },
                Event::PowerOn,
                Event::AckRequested,
            ]
        );
    }

    #[test]
    fn colour_cache_ranks() {
        let mut cache = ColourCache::default();
        assert!(cache.insert(10));
        assert!(cache.insert(20));
        assert!(cache.insert(30));
        assert!(!cache.insert(20));
        // 20 is rank 0 now, 30 rank 1, 10 rank 2.
        assert_eq!(cache.find(2), Some(10));
        assert_eq!(cache.find(1), Some(20));
        assert_eq!(cache.find(7), None);
        assert_eq!(cache.pixcode_state(), PIXCODE1);
    }

    /// Decodes a capture of a real iLO, kept outside the repo because it
    /// shows a real console. Run with
    /// `ILO_CAPTURE=…/kvm_refresh.json cargo test -p server-remote-ilo -- --ignored`.
    #[test]
    #[ignore = "needs a local capture file"]
    fn decodes_a_local_capture() {
        let path = std::env::var("ILO_CAPTURE").expect("set ILO_CAPTURE");
        let capture: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let key_text = capture["keys"]["rc_info"][0]["enc_key"].as_str().unwrap();
        let key: [u8; 16] =
            core::array::from_fn(|i| u8::from_str_radix(&key_text[i * 2..i * 2 + 2], 16).unwrap());
        let mut decoder = Decoder::new(key);
        let mut events = Vec::new();
        let mut greeted = 0;
        for event in capture["conns"]["kvm"]["events"].as_array().unwrap() {
            if event["dir"] != "rx" {
                continue;
            }
            let hex = event["hex"].as_str().unwrap();
            let bytes: Vec<u8> = (0..hex.len() / 2)
                .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
                .collect();
            // The greeting and the status byte come before the video.
            if greeted < 2 {
                greeted += 1;
                continue;
            }
            decoder.feed(&bytes, &mut events);
        }
        let frames = events.iter().filter(|e| **e == Event::Frame).count();
        eprintln!("size {:?}, {frames} frames", decoder.size());
        // ILO_CAPTURE_PPM=<path> saves the final screen to look at.
        if let Ok(out) = std::env::var("ILO_CAPTURE_PPM") {
            let (width, height) = decoder.size();
            let mut ppm = format!("P6\n{width} {height}\n255\n").into_bytes();
            for pixel in decoder.framebuffer().chunks_exact(4) {
                ppm.extend_from_slice(&pixel[..3]);
            }
            std::fs::write(out, ppm).unwrap();
        }
        assert!(frames > 0);
        assert!(decoder.size().0 > 0);
    }
}
