//! A minimal EGA-style indexed framebuffer plus the QBasic graphics primitives
//! the game relies on (`LINE`, `CIRCLE`, `PAINT`, `PSET`, `PUT`, `POINT`).
//!
//! The game logic runs on the original 640×350 EGA coordinate system and reads
//! collisions back with [`IndexBuffer::point`], exactly like the BASIC source
//! which used `POINT` against the drawn screen. Colour indices carry gameplay
//! meaning (sky / gorilla / building / sun), so they are kept as `u8` and only
//! converted to RGB for display.

use crate::subcell::{Framebuffer, Rgb};

pub const SCR_W: i32 = 640;
pub const SCR_H: i32 = 350;

// Palette indices with gameplay meaning (see the BASIC CONSTs).
pub const SKY: u8 = 0; // BACKATTR / background
pub const GORILLA: u8 = 1; // OBJECTCOLOR
#[allow(dead_code)] // ExplosionColor: part of the EGA palette map, kept for fidelity
pub const EXPLOSION: u8 = 2; // ExplosionColor
pub const SUN: u8 = 3; // SUNATTR

/// EGA 16-entry palette (each entry an RGB triple), after the game's `SetScreen`
/// remaps for SCREEN 9.
pub struct Palette {
    pub rgb: [Rgb; 16],
}

/// Convert a 6-bit EGA palette value (0..63) to RGB, matching DOSBox.
fn ega_rgb(val: u8) -> Rgb {
    let r = (((val & 0x04) >> 1) | ((val & 0x20) >> 5)) * 85;
    let g = ((val & 0x02) | ((val & 0x10) >> 4)) * 85;
    let b = (((val & 0x01) << 1) | ((val & 0x08) >> 3)) * 85;
    Rgb::new(r, g, b)
}

impl Palette {
    /// Build the palette the game uses in EGA mode (`SUB SetScreen`, SCREEN 9),
    /// including the `PALETTE OBJECTCOLOR, 46` applied after gorillas are drawn.
    pub fn game() -> Self {
        // Default EGA text-palette values for indices 0..15.
        let defaults: [u8; 16] = [0, 1, 2, 3, 4, 5, 20, 7, 56, 57, 58, 59, 60, 61, 62, 63];
        let mut rgb = [Rgb::default(); 16];
        for i in 0..16 {
            rgb[i] = ega_rgb(defaults[i]);
        }
        // SetScreen remaps.
        let remap: [(usize, u8); 11] = [
            (0, 1),
            (1, 46),
            (2, 44),
            (3, 54),
            (5, 7),
            (6, 4),
            (7, 3),
            (9, 63),
            (10, 24),
            (14, 55),
            (8, 56),
        ];
        for (idx, val) in remap {
            rgb[idx] = ega_rgb(val);
        }
        Palette { rgb }
    }

    pub fn rgb(&self, index: u8) -> Rgb {
        self.rgb[(index & 0x0f) as usize]
    }
}

/// A 640×350 indexed image plus drawing operations.
pub struct IndexBuffer {
    pub w: i32,
    pub h: i32,
    pub px: Vec<u8>,
}

impl IndexBuffer {
    pub fn new() -> Self {
        IndexBuffer {
            w: SCR_W,
            h: SCR_H,
            px: vec![SKY; (SCR_W * SCR_H) as usize],
        }
    }

    pub fn clear(&mut self, index: u8) {
        self.px.iter_mut().for_each(|p| *p = index);
    }

    #[inline]
    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.w && y < self.h
    }

    #[inline]
    pub fn pset(&mut self, x: i32, y: i32, index: u8) {
        if self.in_bounds(x, y) {
            self.px[(y * self.w + x) as usize] = index;
        }
    }

    /// `POINT(x, y)`: read a pixel, returning -1 off-screen like QBasic.
    #[inline]
    pub fn point(&self, x: i32, y: i32) -> i32 {
        if self.in_bounds(x, y) {
            self.px[(y * self.w + x) as usize] as i32
        } else {
            -1
        }
    }

    /// `LINE (x1,y1)-(x2,y2), index` — Bresenham.
    pub fn line(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, index: u8) {
        let (mut x, mut y) = (x1, y1);
        let dx = (x2 - x1).abs();
        let dy = -(y2 - y1).abs();
        let sx = if x1 < x2 { 1 } else { -1 };
        let sy = if y1 < y2 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            self.pset(x, y, index);
            if x == x2 && y == y2 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// `LINE (x1,y1)-(x2,y2), index, B` — rectangle outline.
    pub fn rect(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, index: u8) {
        self.line(x1, y1, x2, y1, index);
        self.line(x2, y1, x2, y2, index);
        self.line(x2, y2, x1, y2, index);
        self.line(x1, y2, x1, y1, index);
    }

    /// `LINE (x1,y1)-(x2,y2), index, BF` — filled rectangle.
    pub fn rect_fill(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, index: u8) {
        let (lo_x, hi_x) = (x1.min(x2), x1.max(x2));
        let (lo_y, hi_y) = (y1.min(y2), y1.max(y2));
        for y in lo_y..=hi_y {
            for x in lo_x..=hi_x {
                self.pset(x, y, index);
            }
        }
    }

    /// `CIRCLE (cx,cy), r, index [, start, end] [, aspect]`.
    ///
    /// `aspect` follows QBasic semantics: `<1` means `r` is the x-radius and the
    /// y-radius is `r*aspect`; `>1` means `r` is the y-radius. `start`/`end` (in
    /// radians) draw an arc; when the arc spans the full circle both are `None`.
    #[allow(clippy::too_many_arguments)]
    pub fn circle(
        &mut self,
        cx: f64,
        cy: f64,
        r: f64,
        index: u8,
        start: Option<f64>,
        end: Option<f64>,
        aspect: f64,
    ) {
        let asp = aspect.abs();
        let (rx, ry) = if asp <= 1.0 {
            (r, r * asp)
        } else {
            (r / asp, r)
        };
        let (mut a0, a1) = match (start, end) {
            (Some(s), Some(e)) => (s, e),
            _ => (0.0, std::f64::consts::TAU),
        };
        // QBasic sweeps counter-clockwise; if end < start, wrap around.
        let mut a1 = a1;
        if a1 < a0 {
            a1 += std::f64::consts::TAU;
        }
        // Step fine enough that adjacent samples are within a pixel.
        let steps = (((rx.abs() + ry.abs()) * 2.0).ceil() as i32).max(16);
        let da = (a1 - a0) / steps as f64;
        let mut prev: Option<(i32, i32)> = None;
        for _ in 0..=steps {
            let px = (cx + rx * a0.cos()).round() as i32;
            let py = (cy - ry * a0.sin()).round() as i32;
            match prev {
                Some((qx, qy)) => self.line(qx, qy, px, py, index),
                None => self.pset(px, py, index),
            }
            prev = Some((px, py));
            a0 += da;
        }
    }

    /// Filled disc (used for craters and the sun body).
    pub fn disc(&mut self, cx: f64, cy: f64, r: f64, index: u8, aspect: f64) {
        let asp = aspect.abs();
        let (rx, ry) = if asp <= 1.0 {
            (r, r * asp)
        } else {
            (r / asp, r)
        };
        let (rx, ry) = (rx.max(0.0), ry.max(0.0));
        let y0 = (cy - ry).floor() as i32;
        let y1 = (cy + ry).ceil() as i32;
        for y in y0..=y1 {
            let dy = (y as f64 - cy) / ry.max(0.0001);
            let s = 1.0 - dy * dy;
            if s < 0.0 {
                continue;
            }
            let half = rx * s.sqrt();
            let xa = (cx - half).round() as i32;
            let xb = (cx + half).round() as i32;
            for x in xa..=xb {
                self.pset(x, y, index);
            }
        }
    }

    /// `PAINT (x,y), fill, border` — flood fill up to a border colour.
    pub fn paint(&mut self, x: i32, y: i32, fill: u8, border: u8) {
        if !self.in_bounds(x, y) {
            return;
        }
        let start = self.px[(y * self.w + x) as usize];
        if start == fill || start == border {
            return;
        }
        let mut stack = vec![(x, y)];
        while let Some((cx, cy)) = stack.pop() {
            if !self.in_bounds(cx, cy) {
                continue;
            }
            let idx = (cy * self.w + cx) as usize;
            let cur = self.px[idx];
            if cur == border || cur == fill {
                continue;
            }
            // Scan left/right on this row.
            let mut xl = cx;
            while xl > 0 {
                let c = self.px[(cy * self.w + (xl - 1)) as usize];
                if c == border || c == fill {
                    break;
                }
                xl -= 1;
            }
            let mut xr = cx;
            while xr < self.w - 1 {
                let c = self.px[(cy * self.w + (xr + 1)) as usize];
                if c == border || c == fill {
                    break;
                }
                xr += 1;
            }
            for fx in xl..=xr {
                self.px[(cy * self.w + fx) as usize] = fill;
                if cy > 0 {
                    let up = self.px[((cy - 1) * self.w + fx) as usize];
                    if up != border && up != fill {
                        stack.push((fx, cy - 1));
                    }
                }
                if cy < self.h - 1 {
                    let dn = self.px[((cy + 1) * self.w + fx) as usize];
                    if dn != border && dn != fill {
                        stack.push((fx, cy + 1));
                    }
                }
            }
        }
    }

    /// `PUT (x,y), sprite, PSET` — blit a sprite's opaque pixels.
    /// The sprite's transparent index is `transparent` (skipped).
    pub fn put(&mut self, x: i32, y: i32, sprite: &Sprite, transparent: Option<u8>) {
        for sy in 0..sprite.h {
            for sx in 0..sprite.w {
                let idx = sprite.px[(sy * sprite.w + sx) as usize];
                if Some(idx) == transparent {
                    continue;
                }
                self.pset(x + sx, y + sy, idx);
            }
        }
    }

    /// Produce an RGB framebuffer for the sub-cell renderer.
    pub fn to_rgb(&self, pal: &Palette) -> Framebuffer {
        let mut fb = Framebuffer::new(self.w as usize, self.h as usize);
        for (i, &idx) in self.px.iter().enumerate() {
            fb.px[i] = pal.rgb(idx);
        }
        fb
    }
}

impl Default for IndexBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// A small indexed sprite bitmap.
#[derive(Clone)]
pub struct Sprite {
    pub w: i32,
    pub h: i32,
    pub px: Vec<u8>,
}

impl Sprite {
    pub fn new(w: i32, h: i32, transparent: u8) -> Self {
        Sprite {
            w,
            h,
            px: vec![transparent; (w * h) as usize],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ega_known_colors() {
        assert_eq!(ega_rgb(0), Rgb::new(0, 0, 0));
        assert_eq!(ega_rgb(63), Rgb::new(255, 255, 255));
        assert_eq!(ega_rgb(46), Rgb::new(255, 170, 85)); // gorilla tan
        assert_eq!(ega_rgb(2), Rgb::new(0, 170, 0)); // green
    }

    #[test]
    fn rect_fill_and_point() {
        let mut b = IndexBuffer::new();
        b.rect_fill(10, 10, 20, 20, GORILLA);
        assert_eq!(b.point(15, 15), GORILLA as i32);
        assert_eq!(b.point(5, 5), SKY as i32);
        assert_eq!(b.point(-1, 0), -1);
    }

    #[test]
    fn paint_fills_bounded_region() {
        let mut b = IndexBuffer::new();
        b.rect(5, 5, 15, 15, GORILLA); // border box
        b.paint(10, 10, EXPLOSION, GORILLA);
        assert_eq!(b.point(10, 10), EXPLOSION as i32);
        // Outside the border stays sky.
        assert_eq!(b.point(0, 0), SKY as i32);
    }

    #[test]
    fn disc_center_filled() {
        let mut b = IndexBuffer::new();
        b.disc(100.0, 100.0, 20.0, SUN, 1.0);
        assert_eq!(b.point(100, 100), SUN as i32);
        assert_eq!(b.point(100, 118), SUN as i32);
        assert_eq!(b.point(100, 130), SKY as i32);
    }
}
