//! An RGBA8 canvas the Display channel's `DRAW_COPY` handler composites raw
//! SPICE pixmaps into.
//!
//! SPICE hands over pixel data in whatever format the guest's QXL/virtio-gpu
//! driver produced (`SpiceBitmap.format`); unlike VNC's raw rectangles, it
//! isn't pre-converted to RGBA and rows may be padded beyond
//! `width * bytes_per_pixel`, so every format needs its own per-pixel
//! conversion and the destination write always respects `stride`.

use crate::Frame;

/// A SPICE `SpiceBitmap` pixel format this canvas knows how to convert.
/// Named after the wire format (`PIXMAP_FORMAT_*` in the SPICE protocol
/// spec); byte order in the source buffer is little-endian, as produced by
/// an x86/x86_64 QEMU guest — **unverified against a live capture**, flip
/// the channel order here first if colors look swapped in testing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// `PIXMAP_FORMAT_16BIT`: 16-bit RGB555, 2 bytes/pixel.
    Rgb555,
    /// `PIXMAP_FORMAT_24BIT`: 24-bit RGB888, 3 bytes/pixel, byte order B,G,R.
    Rgb888,
    /// `PIXMAP_FORMAT_32BIT`: 32-bit RGB888 (padded), 4 bytes/pixel, byte
    /// order B,G,R,X (the 4th byte is unused padding, not alpha).
    Xrgb8888,
    /// `PIXMAP_FORMAT_RGBA`: 32-bit ARGB8888, 4 bytes/pixel, byte order
    /// B,G,R,A.
    Argb8888,
}

impl PixelFormat {
    /// Bytes occupied by one pixel of this format.
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            PixelFormat::Rgb555 => 2,
            PixelFormat::Rgb888 => 3,
            PixelFormat::Xrgb8888 | PixelFormat::Argb8888 => 4,
        }
    }

    /// Convert one pixel at `src[..bytes_per_pixel()]` to opaque-unless-ARGB
    /// RGBA8.
    fn read_pixel(self, src: &[u8]) -> [u8; 4] {
        match self {
            PixelFormat::Rgb555 => {
                let word = u16::from_le_bytes([src[0], src[1]]);
                let r5 = (word >> 10) & 0x1F;
                let g5 = (word >> 5) & 0x1F;
                let b5 = word & 0x1F;
                [expand_5_to_8(r5), expand_5_to_8(g5), expand_5_to_8(b5), 255]
            }
            PixelFormat::Rgb888 => [src[2], src[1], src[0], 255],
            PixelFormat::Xrgb8888 => [src[2], src[1], src[0], 255],
            PixelFormat::Argb8888 => [src[2], src[1], src[0], src[3]],
        }
    }
}

/// Expand a 5-bit color channel to 8 bits by replicating its high bits into
/// the low bits, rather than a lossy left-shift-only scale.
fn expand_5_to_8(value: u16) -> u8 {
    ((value << 3) | (value >> 2)) as u8
}

/// The destination rectangle and source geometry for one [`Framebuffer::apply_pixmap`] blit.
#[derive(Debug, Clone, Copy)]
pub struct PixmapBlit {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub stride: u32,
    pub format: PixelFormat,
}

pub struct Framebuffer {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Framebuffer {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; (width * height * 4) as usize],
        }
    }

    /// Composite a rectangle of `format`-encoded pixels described by `blit`.
    /// `blit.stride` is the source's bytes-per-row, which may exceed
    /// `blit.w * format.bytes_per_pixel()` (row padding) — never assume
    /// tight packing. Out-of-bounds destination pixels are clipped; a `data`
    /// too short for the declared rectangle stops the blit early rather
    /// than panicking.
    pub fn apply_pixmap(&mut self, blit: PixmapBlit, data: &[u8]) {
        let PixmapBlit {
            x,
            y,
            w,
            h,
            stride,
            format,
        } = blit;
        let bpp = format.bytes_per_pixel();
        for row in 0..h {
            let dy = y + row;
            if dy >= self.height {
                break;
            }
            let row_start = (row * stride) as usize;
            if row_start + (w as usize) * bpp > data.len() {
                return;
            }
            for col in 0..w {
                let dx = x + col;
                if dx >= self.width {
                    continue;
                }
                let src = row_start + (col as usize) * bpp;
                let rgba = format.read_pixel(&data[src..src + bpp]);
                let dst = ((dy * self.width + dx) * 4) as usize;
                self.pixels[dst..dst + 4].copy_from_slice(&rgba);
            }
        }
    }

    pub fn to_frame(&self) -> Frame {
        Frame {
            width: self.width,
            height: self.height,
            pixels: self.pixels.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_framebuffer_is_zeroed() {
        let fb = Framebuffer::new(2, 2);
        assert_eq!(fb.to_frame().pixels, vec![0u8; 2 * 2 * 4]);
    }

    fn blit(x: u32, y: u32, w: u32, h: u32, stride: u32, format: PixelFormat) -> PixmapBlit {
        PixmapBlit {
            x,
            y,
            w,
            h,
            stride,
            format,
        }
    }

    #[test]
    fn apply_pixmap_32bit_writes_pixels_opaque() {
        let mut fb = Framebuffer::new(2, 2);
        // Xrgb8888, byte order B,G,R,X: pure red at (1,1).
        fb.apply_pixmap(blit(1, 1, 1, 1, 4, PixelFormat::Xrgb8888), &[0, 0, 255, 0]);

        let frame = fb.to_frame();
        assert_eq!(&frame.pixels[12..16], &[255, 0, 0, 255]);
        assert_eq!(&frame.pixels[0..4], &[0, 0, 0, 0]);
    }

    #[test]
    fn apply_pixmap_16bit_converts_rgb555() {
        let mut fb = Framebuffer::new(1, 1);
        // Pure red in RGB555: R=0x1F, G=0, B=0 -> 0b0_11111_00000_00000 = 0x7C00.
        let word = 0x7C00u16.to_le_bytes();
        fb.apply_pixmap(blit(0, 0, 1, 1, 2, PixelFormat::Rgb555), &word);

        assert_eq!(&fb.to_frame().pixels[0..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn apply_pixmap_honors_stride_padding() {
        let mut fb = Framebuffer::new(2, 2);
        // 2 px wide, Xrgb8888 (4 bytes/px) but stride=12 (4 bytes of padding
        // per row): row 0 is green at col 0, row 1 is blue at col 0.
        let mut data = vec![0u8; 12 * 2];
        data[0..4].copy_from_slice(&[0, 255, 0, 0]); // row 0, col 0: green
        data[12..16].copy_from_slice(&[255, 0, 0, 0]); // row 1, col 0: blue

        fb.apply_pixmap(blit(0, 0, 2, 2, 12, PixelFormat::Xrgb8888), &data);

        let frame = fb.to_frame();
        assert_eq!(&frame.pixels[0..4], &[0, 255, 0, 255]); // (0,0) green
        assert_eq!(&frame.pixels[8..12], &[0, 0, 255, 255]); // (0,1) blue
    }

    #[test]
    fn apply_pixmap_clips_to_bounds() {
        let mut fb = Framebuffer::new(2, 2);
        // A 2x2 rect placed at (1,1) only has one in-bounds pixel.
        let data = [
            1, 2, 3, 0, // (1,1) in bounds
            4, 5, 6, 0, // (2,1) out of bounds
            7, 8, 9, 0, // (1,2) out of bounds
            1, 1, 1, 0, // (2,2) out of bounds
        ];
        fb.apply_pixmap(blit(1, 1, 2, 2, 8, PixelFormat::Xrgb8888), &data);
        assert_eq!(&fb.to_frame().pixels[12..16], &[3, 2, 1, 255]);
    }
}
