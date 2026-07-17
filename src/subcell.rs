//! Sub-cell rendering: turn an RGB pixel framebuffer into a grid of terminal
//! cells, where each cell packs several sub-pixels using Unicode block glyphs.
//!
//! Three modes are supported, trading horizontal/vertical density for glyph
//! availability in the terminal font:
//!
//! * [`SubcellMode::Half`]  — 1×2 sub-cells (`▀▄█`, universally supported).
//! * [`SubcellMode::Sextant`] — 2×3 sub-cells (Unicode 13 legacy computing).
//! * [`SubcellMode::Octant`]  — 2×4 sub-cells (Unicode 16 block octants).
//!
//! Cell bit numbering is row-major, left column first, e.g. for octants:
//! ```text
//!   bit0 bit1      (top row)
//!   bit2 bit3
//!   bit4 bit5
//!   bit6 bit7      (bottom row)
//! ```
//! which matches the `BLOCK OCTANT-n` code-point ordering used to build the
//! lookup tables in `subcell_tables.rs`.

use crate::subcell_tables::{OCTANT_GLYPHS, SEXTANT_GLYPHS};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SubcellMode {
    Half,
    Sextant,
    Octant,
}

impl SubcellMode {
    /// Sub-pixel dimensions of a single terminal cell: (cols, rows).
    pub const fn dims(self) -> (u16, u16) {
        match self {
            SubcellMode::Half => (1, 2),
            SubcellMode::Sextant => (2, 3),
            SubcellMode::Octant => (2, 4),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Rgb { r, g, b }
    }
    fn dist2(self, o: Rgb) -> u32 {
        let dr = self.r as i32 - o.r as i32;
        let dg = self.g as i32 - o.g as i32;
        let db = self.b as i32 - o.b as i32;
        (dr * dr + dg * dg + db * db) as u32
    }
}

/// One resolved terminal cell: a glyph plus foreground/background colours.
#[derive(Copy, Clone, Debug)]
pub struct Cell {
    pub ch: char,
    pub fg: Rgb,
    pub bg: Rgb,
}

/// An RGB pixel framebuffer sampled by the renderer.
pub struct Framebuffer {
    pub width: usize,
    pub height: usize,
    pub px: Vec<Rgb>,
}

impl Framebuffer {
    pub fn new(width: usize, height: usize) -> Self {
        Framebuffer {
            width,
            height,
            px: vec![Rgb::default(); width * height],
        }
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize) -> Rgb {
        self.px[y * self.width + x]
    }
}

/// Render `fb` into a `cols`×`rows` grid of [`Cell`]s using the given mode.
///
/// The source framebuffer is scaled (nearest-neighbour) onto the sub-pixel grid
/// so that the game's internal resolution is decoupled from the terminal size.
pub fn render(fb: &Framebuffer, mode: SubcellMode, cols: u16, rows: u16) -> Vec<Vec<Cell>> {
    let (cw, ch) = mode.dims();
    let sub_w = cols as usize * cw as usize; // total sub-pixels across
    let sub_h = rows as usize * ch as usize; // total sub-pixels down
    if sub_w == 0 || sub_h == 0 {
        return Vec::new();
    }

    // Pre-sample the whole sub-pixel grid from the framebuffer.
    let mut grid = vec![Rgb::default(); sub_w * sub_h];
    for sy in 0..sub_h {
        let src_y = sy * fb.height / sub_h;
        for sx in 0..sub_w {
            let src_x = sx * fb.width / sub_w;
            grid[sy * sub_w + sx] = fb.get(src_x, src_y);
        }
    }

    let table: &[char] = match mode {
        SubcellMode::Half => &[],
        SubcellMode::Sextant => &SEXTANT_GLYPHS,
        SubcellMode::Octant => &OCTANT_GLYPHS,
    };

    let mut out = Vec::with_capacity(rows as usize);
    for cy in 0..rows as usize {
        let mut line = Vec::with_capacity(cols as usize);
        for cx in 0..cols as usize {
            // Collect this cell's sub-pixels (row-major, left column first).
            let n = (cw * ch) as usize;
            let mut sub = [Rgb::default(); 8];
            for r in 0..ch as usize {
                for c in 0..cw as usize {
                    let sx = cx * cw as usize + c;
                    let sy = cy * ch as usize + r;
                    sub[r * cw as usize + c] = grid[sy * sub_w + sx];
                }
            }
            line.push(resolve_cell(&sub[..n], table, mode));
        }
        out.push(line);
    }
    out
}

/// Reduce a cell's sub-pixels to (glyph, fg, bg) by choosing the two dominant
/// colours and packing a bitmask of which sub-pixels are closer to the fg.
fn resolve_cell(sub: &[Rgb], table: &[char], mode: SubcellMode) -> Cell {
    // Gather distinct colours with counts (cells are tiny, linear scan is fine).
    let mut palette: Vec<(Rgb, u32)> = Vec::with_capacity(8);
    for &p in sub {
        if let Some(e) = palette.iter_mut().find(|(c, _)| *c == p) {
            e.1 += 1;
        } else {
            palette.push((p, 1));
        }
    }

    if palette.len() == 1 {
        let c = palette[0].0;
        return Cell {
            ch: ' ',
            fg: c,
            bg: c,
        };
    }

    // Background = most common colour. Foreground = colour maximising
    // count * colour-distance from bg, so a small bright detail still wins.
    palette.sort_by_key(|&(_, count)| std::cmp::Reverse(count));
    let bg = palette[0].0;
    let fg = palette[1..]
        .iter()
        .max_by_key(|(c, cnt)| *cnt as u64 * (c.dist2(bg) as u64 + 1))
        .map(|(c, _)| *c)
        .unwrap_or(bg);

    // Bitmask: sub-pixel is "on" (fg) if nearer fg than bg.
    let mut mask: usize = 0;
    for (i, &p) in sub.iter().enumerate() {
        if p.dist2(fg) < p.dist2(bg) {
            mask |= 1 << i;
        }
    }

    let ch = match mode {
        SubcellMode::Half => match mask & 0b11 {
            0b00 => ' ',
            0b01 => '\u{2580}', // top only
            0b10 => '\u{2584}', // bottom only
            _ => '\u{2588}',    // both
        },
        SubcellMode::Sextant => table[mask & 0x3f],
        SubcellMode::Octant => table[mask & 0xff],
    };

    Cell { ch, fg, bg }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_are_full() {
        assert_eq!(OCTANT_GLYPHS.len(), 256);
        assert_eq!(SEXTANT_GLYPHS.len(), 64);
        // Boundary glyphs.
        assert_eq!(OCTANT_GLYPHS[0], ' ');
        assert_eq!(OCTANT_GLYPHS[255], '\u{2588}');
        assert_eq!(SEXTANT_GLYPHS[0], ' ');
        assert_eq!(SEXTANT_GLYPHS[63], '\u{2588}');
    }

    #[test]
    fn octant_known_patterns() {
        // upper half = top 4 bits set (rows 0,1) -> ▀
        assert_eq!(OCTANT_GLYPHS[0b0000_1111], '\u{2580}');
        // lower half -> ▄
        assert_eq!(OCTANT_GLYPHS[0b1111_0000], '\u{2584}');
        // left column (bits 0,2,4,6) -> ▌
        assert_eq!(OCTANT_GLYPHS[0b0101_0101], '\u{258C}');
        // right column -> ▐
        assert_eq!(OCTANT_GLYPHS[0b1010_1010], '\u{2590}');
    }

    #[test]
    fn solid_cell_collapses() {
        let sub = [Rgb::new(10, 20, 30); 8];
        let c = resolve_cell(&sub, &OCTANT_GLYPHS, SubcellMode::Octant);
        assert_eq!(c.ch, ' ');
        assert_eq!(c.fg, c.bg);
    }

    #[test]
    fn two_color_split_picks_glyph() {
        // top row one colour, rest another -> not blank
        let mut sub = [Rgb::new(0, 0, 0); 8];
        sub[0] = Rgb::new(255, 255, 255);
        sub[1] = Rgb::new(255, 255, 255);
        let c = resolve_cell(&sub, &OCTANT_GLYPHS, SubcellMode::Octant);
        assert_ne!(c.ch, ' ');
        assert_ne!(c.fg, c.bg);
    }
}
