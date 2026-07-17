//! Rasterised sprites and scene elements, ported op-for-op from the BASIC
//! `DrawGorilla` / `DoSun` routines (EGA / SCREEN 9 branch).

use crate::ega::{IndexBuffer, Sprite, GORILLA, SCR_W, SKY, SUN};
use std::f64::consts::PI;

/// QBasic `CINT`: round to nearest, ties to even.
fn cint(n: f64) -> i32 {
    let r = n.round();
    if (n - n.floor() - 0.5).abs() < 1e-9 {
        // exact .5 -> round to even
        let f = n.floor() as i32;
        if f % 2 == 0 {
            f
        } else {
            f + 1
        }
    } else {
        r as i32
    }
}

/// `Scl` in EGA mode is just `CINT`.
fn scl(n: f64) -> i32 {
    cint(n)
}

/// Gorilla arm pose.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Arms {
    RightUp,
    LeftUp,
    Down,
}

/// Sprite dimensions of a gorilla, per the BASIC `GET` extents:
/// `GET (x-Scl(15), y-Scl(1))-(x+Scl(14), y+Scl(28))` → 30×30.
pub const GOR_W: i32 = 30;
pub const GOR_H: i32 = 30;
/// Offset from the gorilla's draw origin `(x, y)` to the sprite's top-left.
pub const GOR_OX: i32 = 15;
pub const GOR_OY: i32 = 1;

/// Draw a gorilla directly into `buf` at draw-origin `(x, y)`.
pub fn draw_gorilla(buf: &mut IndexBuffer, x: i32, y: i32, arms: Arms) {
    let x = x as f64;
    let y = y as f64;
    let xi = |dx: f64| (x + scl(dx) as f64) as i32;
    let yi = |dy: f64| (y + scl(dy) as f64) as i32;
    let asp = 1.0;

    // head
    buf.rect_fill(xi(-4.0), yi(0.0), xi(2.9), yi(6.0), GORILLA);
    buf.rect_fill(xi(-5.0), yi(2.0), xi(4.0), yi(4.0), GORILLA);
    // eyes/brow
    buf.line(xi(-3.0), yi(2.0), xi(2.0), yi(2.0), 0);
    // nose (EGA only)
    for i in -2..=-1 {
        buf.pset(x as i32 + i, y as i32 + 4, 0);
        buf.pset(x as i32 + i + 3, y as i32 + 4, 0);
    }
    // neck
    buf.line(xi(-3.0), yi(7.0), xi(2.0), yi(7.0), GORILLA);
    // body
    buf.rect_fill(xi(-8.0), yi(8.0), xi(6.9), yi(14.0), GORILLA);
    buf.rect_fill(xi(-6.0), yi(15.0), xi(4.9), yi(20.0), GORILLA);
    // legs
    for i in 0..=4 {
        let ii = i as f64;
        buf.circle(
            x + scl(ii) as f64,
            y + scl(25.0) as f64,
            10.0,
            GORILLA,
            Some(3.0 * PI / 4.0),
            Some(9.0 * PI / 8.0),
            asp,
        );
        buf.circle(
            x + scl(-6.0) as f64 + scl(ii - 0.1) as f64,
            y + scl(25.0) as f64,
            10.0,
            GORILLA,
            Some(15.0 * PI / 8.0),
            Some(PI / 4.0),
            asp,
        );
    }
    // chest cut-outs (background arcs)
    buf.circle(
        xi(-4.9) as f64,
        yi(10.0) as f64,
        5.0,
        0,
        Some(3.0 * PI / 2.0),
        Some(2.0 * PI),
        asp,
    );
    buf.circle(
        xi(4.9) as f64,
        yi(10.0) as f64,
        5.0,
        0,
        Some(PI),
        Some(3.0 * PI / 2.0),
        asp,
    );
    // arms
    for i in -5..=-1 {
        let ii = i as f64;
        let (left_dy, right_dy) = match arms {
            Arms::RightUp => (14.0, 4.0),
            Arms::LeftUp => (4.0, 14.0),
            Arms::Down => (14.0, 14.0),
        };
        buf.circle(
            x + scl(ii - 0.1) as f64,
            y + scl(left_dy) as f64,
            9.0,
            GORILLA,
            Some(3.0 * PI / 4.0),
            Some(5.0 * PI / 4.0),
            asp,
        );
        buf.circle(
            x + scl(4.9) as f64 + scl(ii) as f64,
            y + scl(right_dy) as f64,
            9.0,
            GORILLA,
            Some(7.0 * PI / 4.0),
            Some(PI / 4.0),
            asp,
        );
    }
}

/// Build a 30×30 gorilla sprite for the given pose by drawing onto a scratch
/// canvas and extracting the region (equivalent to the BASIC `GET`).
pub fn gorilla_sprite(arms: Arms) -> Sprite {
    let mut canvas = IndexBuffer::new();
    let (cx, cy) = (200, 200);
    draw_gorilla(&mut canvas, cx, cy, arms);
    let mut s = Sprite::new(GOR_W, GOR_H, SKY);
    for sy in 0..GOR_H {
        for sx in 0..GOR_W {
            let px = cx - GOR_OX + sx;
            let py = cy - GOR_OY + sy;
            s.px[(sy * GOR_W + sx) as usize] = canvas.point(px, py) as u8;
        }
    }
    s
}

/// Draw the sun at the top-centre. `shock` selects the open-mouthed face.
pub fn draw_sun(buf: &mut IndexBuffer, shock: bool) {
    let x = (SCR_W / 2) as f64;
    let y = scl(25.0) as f64;
    let s = |n: f64| scl(n) as f64;

    // clear old sun area to background
    buf.rect_fill(
        (x - s(22.0)) as i32,
        (y - s(18.0)) as i32,
        (x + s(22.0)) as i32,
        (y + s(18.0)) as i32,
        SKY,
    );
    // body
    buf.circle(x, y, s(12.0), SUN, None, None, 1.0);
    buf.paint(x as i32, y as i32, SUN, SUN);
    buf.disc(x, y, s(12.0), SUN, 1.0);
    // rays
    let rays: [(f64, f64); 5] = [
        (s(20.0), 0.0),
        (0.0, s(15.0)),
        (s(15.0), s(10.0)),
        (s(8.0), s(13.0)),
        (s(18.0), s(5.0)),
    ];
    buf.line(
        (x - rays[0].0) as i32,
        y as i32,
        (x + rays[0].0) as i32,
        y as i32,
        SUN,
    );
    buf.line(
        x as i32,
        (y - rays[1].1) as i32,
        x as i32,
        (y + rays[1].1) as i32,
        SUN,
    );
    for &(dx, dy) in &rays[2..] {
        buf.line(
            (x - dx) as i32,
            (y - dy) as i32,
            (x + dx) as i32,
            (y + dy) as i32,
            SUN,
        );
        buf.line(
            (x - dx) as i32,
            (y + dy) as i32,
            (x + dx) as i32,
            (y - dy) as i32,
            SUN,
        );
    }
    // mouth
    if shock {
        buf.disc(x, y + s(5.0), s(2.9), 0, 1.0);
    } else {
        buf.circle(
            x,
            y,
            s(8.0),
            0,
            Some(210.0 * PI / 180.0),
            Some(330.0 * PI / 180.0),
            1.0,
        );
    }
    // eyes
    buf.disc(x - 3.0, y - 2.0, 1.5, 0, 1.0);
    buf.disc(x + 3.0, y - 2.0, 1.5, 0, 1.0);
}

/// A small hand-drawn banana sprite for each rotation (0=left,1=up,2=down,3=right).
/// Purely cosmetic overlay — collision uses the scene buffer, not this sprite.
pub fn banana_sprite(rot: i32, color: u8) -> Sprite {
    // 8x7 crescent; rotate the base pattern.
    let base: [&str; 7] = [
        "  XXXX  ", " XX  XX ", "XX    XX", "XX    XX", "XX    XX", " XX  XX ", "  XXXX  ",
    ];
    // Base is a ring; make it a filled banana crescent per rotation.
    let pat: [&str; 7] = match rot & 3 {
        0 => [
            "   XXX  ", "  XXXX  ", " XXX    ", " XX     ", " XXX    ", "  XXXX  ", "   XXX  ",
        ],
        1 => [
            " X    X ", " XX  XX ", "  XXXX  ", "  XXXX  ", "        ", "        ", "        ",
        ],
        2 => [
            "        ", "        ", "        ", "  XXXX  ", "  XXXX  ", " XX  XX ", " X    X ",
        ],
        _ => [
            "  XXX   ", "  XXXX  ", "    XXX ", "     XX ", "    XXX ", "  XXXX  ", "  XXX   ",
        ],
    };
    let _ = base;
    let mut s = Sprite::new(8, 7, 0);
    for (yy, row) in pat.iter().enumerate() {
        for (xx, c) in row.chars().enumerate() {
            if c == 'X' {
                s.px[yy * 8 + xx] = color;
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cint_ties_to_even() {
        assert_eq!(cint(2.9), 3);
        assert_eq!(cint(2.5), 2);
        assert_eq!(cint(3.5), 4);
        assert_eq!(cint(-0.1), 0);
        assert_eq!(cint(4.9), 5);
    }

    #[test]
    fn gorilla_sprite_has_body() {
        let s = gorilla_sprite(Arms::Down);
        assert_eq!(s.w, GOR_W);
        let lit = s.px.iter().filter(|&&p| p == GORILLA).count();
        assert!(
            lit > 100,
            "gorilla should have a substantial body, got {lit}"
        );
    }

    #[test]
    fn sun_draws_disc() {
        let mut b = IndexBuffer::new();
        draw_sun(&mut b, false);
        assert_eq!(b.point(SCR_W / 2, 25), SUN as i32);
    }
}
