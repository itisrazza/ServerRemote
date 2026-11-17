//! An RGBA8 canvas the VNC connector composites incremental rectangles into.
//!
//! VNC delivers the screen as a stream of rectangle updates (raw pixels or
//! copy-from-elsewhere), but the session model hands the UI whole [`Frame`]s, so
//! we keep a full canvas here and snapshot it after applying updates.

use server_remote_core::Frame;

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

    /// Composite a raw rectangle of `[r, g, b, _]` pixels at `(x, y)`. The
    /// fourth byte is padding in VNC, so the canvas is kept fully opaque.
    pub fn apply_raw(&mut self, x: u32, y: u32, w: u32, h: u32, data: &[u8]) {
        for row in 0..h {
            let dy = y + row;
            if dy >= self.height {
                break;
            }
            for col in 0..w {
                let dx = x + col;
                if dx >= self.width {
                    continue;
                }
                let src = ((row * w + col) * 4) as usize;
                let dst = ((dy * self.width + dx) * 4) as usize;
                if src + 3 >= data.len() {
                    return;
                }
                self.pixels[dst] = data[src];
                self.pixels[dst + 1] = data[src + 1];
                self.pixels[dst + 2] = data[src + 2];
                self.pixels[dst + 3] = 255;
            }
        }
    }

    /// Copy a `w`×`h` region from `(src_x, src_y)` to `(dst_x, dst_y)`
    /// (VNC's CopyRect). Goes via a temporary to stay correct when the source
    /// and destination overlap.
    pub fn apply_copy(&mut self, dst_x: u32, dst_y: u32, src_x: u32, src_y: u32, w: u32, h: u32) {
        let mut tmp = vec![0u8; (w * h * 4) as usize];
        for row in 0..h {
            let sy = src_y + row;
            if sy >= self.height {
                continue;
            }
            for col in 0..w {
                let sx = src_x + col;
                if sx >= self.width {
                    continue;
                }
                let src = ((sy * self.width + sx) * 4) as usize;
                let dst = ((row * w + col) * 4) as usize;
                tmp[dst..dst + 4].copy_from_slice(&self.pixels[src..src + 4]);
            }
        }
        for row in 0..h {
            let dy = dst_y + row;
            if dy >= self.height {
                continue;
            }
            for col in 0..w {
                let dx = dst_x + col;
                if dx >= self.width {
                    continue;
                }
                let src = ((row * w + col) * 4) as usize;
                let dst = ((dy * self.width + dx) * 4) as usize;
                self.pixels[dst..dst + 4].copy_from_slice(&tmp[src..src + 4]);
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
    fn apply_raw_writes_pixels_opaque() {
        let mut fb = Framebuffer::new(2, 2);
        // Fill the bottom-right pixel red; note the input alpha (0) is ignored.
        fb.apply_raw(1, 1, 1, 1, &[10, 20, 30, 0]);

        let frame = fb.to_frame();
        assert_eq!(&frame.pixels[12..16], &[10, 20, 30, 255]);
        // Untouched pixels stay zero.
        assert_eq!(&frame.pixels[0..4], &[0, 0, 0, 0]);
    }

    #[test]
    fn apply_raw_clips_to_bounds() {
        let mut fb = Framebuffer::new(2, 2);
        // A 2x2 rect at (1,1) only has one in-bounds pixel; the rest is clipped.
        fb.apply_raw(
            1,
            1,
            2,
            2,
            &[1, 2, 3, 0, 4, 5, 6, 0, 7, 8, 9, 0, 1, 1, 1, 0],
        );
        assert_eq!(&fb.to_frame().pixels[12..16], &[1, 2, 3, 255]);
    }

    #[test]
    fn apply_copy_moves_region() {
        let mut fb = Framebuffer::new(2, 1);
        fb.apply_raw(0, 0, 1, 1, &[9, 9, 9, 0]);
        // Copy the left pixel onto the right one.
        fb.apply_copy(1, 0, 0, 0, 1, 1);
        assert_eq!(&fb.to_frame().pixels[4..8], &[9, 9, 9, 255]);
    }
}
