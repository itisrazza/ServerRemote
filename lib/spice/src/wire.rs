//! Wire-level parsing and framing `shakenfist-spice-protocol` doesn't
//! provide: mini-header message framing, the `DRAW_COPY` opcode body
//! (`SpiceCopy`) and the `SpiceBitmap` sub-header that follows a Pixmap
//! `ImageDescriptor`, a pixel-format lookup, and ACK flow control.

use std::io::{self, Cursor};

use byteorder::{LittleEndian, ReadBytesExt};
use shakenfist_spice_protocol::messages::{MessageHeader, SetAck, SpiceQMask, make_message};

use crate::framebuffer::PixelFormat;

/// `SPICE_MSGC_ACK_SYNC` / `SPICE_MSGC_ACK` — common client opcodes shared by
/// every channel (numerically identical across `main_client`/`display_client`
/// /etc, though not all of those modules expose them).
pub const ACK_SYNC: u16 = 1;
pub const ACK: u16 = 2;

/// `SPICE_MSGC_DISPLAY_PREFERRED_COMPRESSION` and
/// `SPICE_IMAGE_COMPRESSION_OFF` — not yet exposed by
/// `shakenfist-spice-protocol` 0.1.5's `display_client`/`image_compression`
/// modules, so defined here from the SPICE protocol spec directly.
pub const DISPLAY_PREFERRED_COMPRESSION: u16 = 103;
pub const IMAGE_COMPRESSION_OFF: u8 = 1;

/// Read one mini-header-framed message: a 6-byte [`MessageHeader`] followed
/// by `message_size` payload bytes. Safe because `perform_link`'s hardcoded
/// common capabilities always include `MINI_HEADER`.
pub async fn read_message<S: tokio::io::AsyncRead + Unpin>(
    stream: &mut S,
) -> io::Result<(MessageHeader, Vec<u8>)> {
    use tokio::io::AsyncReadExt;

    let mut header_buf = [0u8; MessageHeader::SIZE];
    stream.read_exact(&mut header_buf).await?;
    let header = MessageHeader::read(&header_buf)?;

    let mut payload = vec![0u8; header.message_size as usize];
    stream.read_exact(&mut payload).await?;

    Ok((header, payload))
}

/// Write one mini-header-framed message.
pub async fn write_message<S: tokio::io::AsyncWrite + Unpin>(
    stream: &mut S,
    message_type: u16,
    payload: &[u8],
) -> io::Result<()> {
    use tokio::io::AsyncWriteExt;

    let buf = make_message(message_type, payload);
    stream.write_all(&buf).await?;
    stream.flush().await
}

/// `DRAW_COPY`'s opcode-specific body (`SpiceCopy` in draw.h), read at
/// `DrawBase.end_offset`. Same shape as `shakenfist_spice_protocol`'s own
/// `SpiceOpaque`, minus the `brush` field — always exactly [`SpiceCopy::SIZE`]
/// bytes, no variable-length content.
#[derive(Debug, Clone)]
pub struct SpiceCopy {
    /// Byte offset of the source `SpiceImage` within the surrounding message
    /// payload (SPICE's wire marshaller substitutes payload-relative offsets
    /// for what would be shared-memory pointers), not a real pointer.
    pub src_bitmap: u32,
    pub src_top: u32,
    pub src_left: u32,
    pub src_bottom: u32,
    pub src_right: u32,
    /// Raster-op combination flags; unused — this crate only implements a
    /// plain copy (`SPICE_ROPD_OP_PUT`), not the full raster-op vocabulary.
    #[allow(dead_code)]
    pub rop_descriptor: u16,
    /// Unused — no scaling support; a mismatched source/destination
    /// rectangle size is just clipped to the smaller of the two.
    #[allow(dead_code)]
    pub scale_mode: u8,
    /// Unused — masked draws aren't implemented.
    #[allow(dead_code)]
    pub mask: SpiceQMask,
}

impl SpiceCopy {
    pub const SIZE: usize = 36;

    pub fn read(data: &[u8]) -> io::Result<Self> {
        if data.len() < Self::SIZE {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "not enough data for SpiceCopy",
            ));
        }

        let mut cursor = Cursor::new(&data[..23]);
        let src_bitmap = cursor.read_u32::<LittleEndian>()?;
        let src_top = cursor.read_u32::<LittleEndian>()?;
        let src_left = cursor.read_u32::<LittleEndian>()?;
        let src_bottom = cursor.read_u32::<LittleEndian>()?;
        let src_right = cursor.read_u32::<LittleEndian>()?;
        let rop_descriptor = cursor.read_u16::<LittleEndian>()?;
        let scale_mode = cursor.read_u8()?;

        let mask = SpiceQMask::read(&data[23..Self::SIZE])?;

        Ok(SpiceCopy {
            src_bitmap,
            src_top,
            src_left,
            src_bottom,
            src_right,
            rop_descriptor,
            scale_mode,
            mask,
        })
    }
}

/// The `SpiceBitmap` sub-header that follows a Pixmap-typed
/// `ImageDescriptor`. Pixel data follows immediately after, `stride *
/// height` bytes — `width`/`height` are named `x`/`y` in the upstream C
/// struct, a well-known naming quirk in the SPICE headers.
#[derive(Debug, Clone)]
pub struct SpiceBitmap {
    pub format: u8,
    #[allow(dead_code)]
    pub flags: u8,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    #[allow(dead_code)]
    pub palette: u32,
}

impl SpiceBitmap {
    pub const SIZE: usize = 18;

    pub fn read(data: &[u8]) -> io::Result<Self> {
        if data.len() < Self::SIZE {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "not enough data for SpiceBitmap",
            ));
        }

        let mut cursor = Cursor::new(data);
        Ok(SpiceBitmap {
            format: cursor.read_u8()?,
            flags: cursor.read_u8()?,
            width: cursor.read_u32::<LittleEndian>()?,
            height: cursor.read_u32::<LittleEndian>()?,
            stride: cursor.read_u32::<LittleEndian>()?,
            palette: cursor.read_u32::<LittleEndian>()?,
        })
    }
}

/// Map a `SpiceBitmap.format` byte (`PIXMAP_FORMAT_*`) to a
/// [`PixelFormat`] this crate can convert. `None` for indexed/palette
/// formats (1/4/8-bit — legacy, essentially never emitted by a modern
/// guest).
pub fn bitmap_pixel_format(format: u8) -> Option<PixelFormat> {
    match format {
        6 => Some(PixelFormat::Rgb555),
        7 => Some(PixelFormat::Rgb888),
        8 => Some(PixelFormat::Xrgb8888),
        9 => Some(PixelFormat::Argb8888),
        _ => None,
    }
}

/// Tracks a channel's `SPICE_MSG_SET_ACK` flow-control state. Required, not
/// optional: skipping the reply lets the server's ack window fill, after
/// which it silently stops sending further updates.
#[derive(Debug, Default)]
pub struct AckState {
    window: u32,
    since_ack: u32,
}

impl AckState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Call on receiving `SET_ACK{generation, window}`. Returns the
    /// `ACK_SYNC` payload to send immediately in reply.
    pub fn set_ack(&mut self, generation: u32, window: u32) -> Vec<u8> {
        self.window = window;
        self.since_ack = 0;
        let mut buf = Vec::new();
        // SetAck::write_ack_sync cannot fail writing into a Vec.
        SetAck::write_ack_sync(generation, &mut buf).expect("write to Vec is infallible");
        buf
    }

    /// Call once for every message received on this channel after the
    /// initial `SET_ACK`. Returns `true` when a bare `ACK` should be sent
    /// now (every `window` messages).
    pub fn record_message(&mut self) -> bool {
        if self.window == 0 {
            return false;
        }
        self.since_ack += 1;
        if self.since_ack >= self.window {
            self.since_ack = 0;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_header_round_trips() {
        let msg = make_message(42, &[1, 2, 3]);
        let header = MessageHeader::read(&msg[..MessageHeader::SIZE]).unwrap();
        assert_eq!(header.message_type, 42);
        assert_eq!(header.message_size, 3);
        assert_eq!(&msg[MessageHeader::SIZE..], &[1, 2, 3]);
    }

    #[tokio::test]
    async fn read_message_reads_exact_declared_size() {
        let (mut client, mut server) = tokio::io::duplex(256);
        let sent = make_message(7, &[9, 8, 7, 6]);
        tokio::io::AsyncWriteExt::write_all(&mut client, &sent)
            .await
            .unwrap();

        let (header, payload) = read_message(&mut server).await.unwrap();
        assert_eq!(header.message_type, 7);
        assert_eq!(payload, vec![9, 8, 7, 6]);
    }

    #[tokio::test]
    async fn write_message_frames_type_and_payload() {
        let (mut client, mut server) = tokio::io::duplex(256);
        write_message(&mut client, 3, &[1, 2]).await.unwrap();

        let (header, payload) = read_message(&mut server).await.unwrap();
        assert_eq!(header.message_type, 3);
        assert_eq!(payload, vec![1, 2]);
    }

    fn spice_copy_bytes() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&0x100u32.to_le_bytes()); // src_bitmap
        data.extend_from_slice(&10u32.to_le_bytes()); // src_top
        data.extend_from_slice(&20u32.to_le_bytes()); // src_left
        data.extend_from_slice(&30u32.to_le_bytes()); // src_bottom
        data.extend_from_slice(&40u32.to_le_bytes()); // src_right
        data.extend_from_slice(&0x000Cu16.to_le_bytes()); // rop_descriptor
        data.push(0u8); // scale_mode
        // mask: flags=0, pos=(0,0), bitmap_offset=0 (13 bytes)
        data.push(0u8);
        data.extend_from_slice(&0i32.to_le_bytes());
        data.extend_from_slice(&0i32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data
    }

    #[test]
    fn spice_copy_parses_36_byte_body() {
        let data = spice_copy_bytes();
        assert_eq!(data.len(), SpiceCopy::SIZE);

        let copy = SpiceCopy::read(&data).unwrap();
        assert_eq!(copy.src_bitmap, 0x100);
        assert_eq!(
            (copy.src_top, copy.src_left, copy.src_bottom, copy.src_right),
            (10, 20, 30, 40)
        );
        assert_eq!(copy.rop_descriptor, 0x000C);
        assert_eq!(copy.scale_mode, 0);
        assert_eq!(copy.mask.bitmap_offset, 0);
    }

    #[test]
    fn spice_copy_too_short_is_an_error() {
        let mut data = spice_copy_bytes();
        data.pop();
        assert!(SpiceCopy::read(&data).is_err());
    }

    #[test]
    fn spice_bitmap_header_parses_format_and_stride() {
        let mut data = Vec::new();
        data.push(8u8); // format = 32BIT
        data.push(0u8); // flags
        data.extend_from_slice(&1920u32.to_le_bytes()); // width
        data.extend_from_slice(&1080u32.to_le_bytes()); // height
        data.extend_from_slice(&7680u32.to_le_bytes()); // stride
        data.extend_from_slice(&0u32.to_le_bytes()); // palette

        let bitmap = SpiceBitmap::read(&data).unwrap();
        assert_eq!(bitmap.format, 8);
        assert_eq!(bitmap.width, 1920);
        assert_eq!(bitmap.height, 1080);
        assert_eq!(bitmap.stride, 7680);
    }

    #[test]
    fn bitmap_pixel_format_maps_known_formats() {
        assert_eq!(bitmap_pixel_format(6), Some(PixelFormat::Rgb555));
        assert_eq!(bitmap_pixel_format(7), Some(PixelFormat::Rgb888));
        assert_eq!(bitmap_pixel_format(8), Some(PixelFormat::Xrgb8888));
        assert_eq!(bitmap_pixel_format(9), Some(PixelFormat::Argb8888));
    }

    #[test]
    fn bitmap_pixel_format_rejects_indexed_formats() {
        for format in [1u8, 2, 3, 4, 5] {
            assert_eq!(bitmap_pixel_format(format), None, "format {format}");
        }
    }

    #[test]
    fn ack_flow_control_replies_ack_sync_then_bare_ack_after_window() {
        let mut ack = AckState::new();
        let sync_payload = ack.set_ack(7, 2);
        assert_eq!(sync_payload, 7u32.to_le_bytes());

        assert!(!ack.record_message());
        assert!(ack.record_message());
        // Counter resets; the next window takes another 2 messages.
        assert!(!ack.record_message());
        assert!(ack.record_message());
    }

    #[test]
    fn ack_flow_control_does_nothing_before_set_ack() {
        let mut ack = AckState::new();
        assert!(!ack.record_message());
    }
}
