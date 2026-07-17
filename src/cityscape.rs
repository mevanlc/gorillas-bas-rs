//! Faithful port of `MakeCityScape` and `PlaceGorillas` (EGA / SCREEN 9 branch),
//! plus the wind calculation. Draws directly into the scene [`IndexBuffer`].

use crate::ega::{IndexBuffer, SCR_H, SCR_W, SKY};
use crate::rng::QbRng;
use crate::sprites::{gorilla_sprite, Arms};

// EGA constants from the BASIC source.
const BOTTOM_LINE: i32 = 335;
const HT_INC: i32 = 10;
const DEF_BWIDTH: i32 = 37;
const RANDOM_HEIGHT: i32 = 120;
const WWIDTH: i32 = 3;
const WHEIGHT: i32 = 6;
const WDIF_V: i32 = 15;
const WDIF_H: i32 = 10;
const GHEIGHT: i32 = 25;
const WINDOW_COLOR: u8 = 14;
const MAX_HEIGHT: i32 = 0; // undeclared in BASIC → default 0

#[derive(Clone, Copy, Debug)]
pub struct Building {
    pub x: i32,
    pub y: i32, // top-left y
}

pub struct Cityscape {
    /// The generated skyline (retained for inspection / future features).
    #[allow(dead_code)]
    pub buildings: Vec<Building>,
    /// Gorilla sprite top-left positions, indexed 0 (left) and 1 (right).
    pub gorillas: [(i32, i32); 2],
    pub wind: i32,
}

/// `MakeCityScape`: generate the skyline, draw it, and compute the wind.
pub fn make_cityscape(buf: &mut IndexBuffer, rng: &mut QbRng) -> Cityscape {
    buf.clear(SKY);
    let mut buildings: Vec<Building> = Vec::new();

    let mut x = 2;
    let slope = rng.ran(6);
    let mut new_ht = match slope {
        1 => 15,
        2 => 130,
        3..=5 => 15,
        _ => 130, // 6
    };

    loop {
        // Slope trend update.
        match slope {
            1 => new_ht += HT_INC,
            2 => new_ht -= HT_INC,
            // CASE 4 in the source is shadowed by CASE 3 TO 5 and never runs.
            3..=5 => {
                if x > SCR_W / 2 {
                    new_ht -= 2 * HT_INC;
                } else {
                    new_ht += 2 * HT_INC;
                }
            }
            _ => {}
        }

        let mut bwidth = rng.ran(DEF_BWIDTH) + DEF_BWIDTH;
        if x + bwidth > SCR_W {
            bwidth = SCR_W - x - 2;
        }

        let mut bheight = rng.ran(RANDOM_HEIGHT) + new_ht;
        if bheight < HT_INC {
            bheight = HT_INC;
        }
        if BOTTOM_LINE - bheight <= MAX_HEIGHT + GHEIGHT {
            bheight = MAX_HEIGHT + GHEIGHT - 5;
        }

        buildings.push(Building {
            x,
            y: BOTTOM_LINE - bheight,
        });

        let building_color = (rng.ran(3) + 4) as u8; // 5,6,7

        // Outline (background) then filled body.
        buf.rect(
            x - 1,
            BOTTOM_LINE + 1,
            x + bwidth + 1,
            BOTTOM_LINE - bheight - 1,
            SKY,
        );
        buf.rect_fill(
            x,
            BOTTOM_LINE,
            x + bwidth,
            BOTTOM_LINE - bheight,
            building_color,
        );

        // Windows.
        let mut c = x + 3;
        loop {
            let mut i = bheight - 3;
            while i >= 7 {
                let wcolr: u8 = if rng.ran(4) == 1 { 8 } else { WINDOW_COLOR };
                buf.rect_fill(
                    c,
                    BOTTOM_LINE - i,
                    c + WWIDTH,
                    BOTTOM_LINE - i + WHEIGHT,
                    wcolr,
                );
                i -= WDIF_V;
            }
            c += WDIF_H;
            if c >= x + bwidth - 3 {
                break;
            }
        }

        x += bwidth + 2;
        if x > SCR_W - HT_INC {
            break;
        }
    }

    let last_building = buildings.len(); // 1-based LastBuilding == count
    let wind = compute_wind(rng);
    draw_wind_arrow(buf, wind);

    let gorillas = place_gorillas(buf, rng, &buildings, last_building);

    Cityscape {
        buildings,
        gorillas,
        wind,
    }
}

fn compute_wind(rng: &mut QbRng) -> i32 {
    let mut wind = rng.ran(10) - 5;
    if rng.ran(3) == 1 {
        if wind > 0 {
            wind += rng.ran(10);
        } else {
            wind -= rng.ran(10);
        }
    }
    wind
}

/// Draw the wind arrow at the bottom of the field (color = ExplosionColor=2).
fn draw_wind_arrow(buf: &mut IndexBuffer, wind: i32) {
    if wind == 0 {
        return;
    }
    let scale = SCR_W / 320; // = 2
    let wind_line = wind * 3 * scale;
    let cx = SCR_W / 2;
    let y = SCR_H - 5;
    buf.line(cx, y, cx + wind_line, y, crate::ega::EXPLOSION);
    let arrow_dir = if wind > 0 { -2 } else { 2 };
    buf.line(
        cx + wind_line,
        y,
        cx + wind_line + arrow_dir,
        y - 2,
        crate::ega::EXPLOSION,
    );
    buf.line(
        cx + wind_line,
        y,
        cx + wind_line + arrow_dir,
        y + 2,
        crate::ega::EXPLOSION,
    );
}

/// `PlaceGorillas`: choose buildings 2nd/3rd from each edge and stamp gorillas.
fn place_gorillas(
    buf: &mut IndexBuffer,
    rng: &mut QbRng,
    buildings: &[Building],
    last_building: usize,
) -> [(i32, i32); 2] {
    const XADJ: i32 = 14;
    const YADJ: i32 = 30;
    let down = gorilla_sprite(Arms::Down);
    let mut pos = [(0, 0); 2];
    for (i, slot) in pos.iter_mut().enumerate() {
        let bnum = if i == 0 {
            rng.ran(2) + 1 // 2 or 3 (1-based)
        } else {
            last_building as i32 - rng.ran(2) // last-1 or last-2
        };
        // Convert to 0-based; guard bounds.
        let idx = (bnum - 1).clamp(0, buildings.len() as i32 - 2) as usize;
        let bwidth = buildings[idx + 1].x - buildings[idx].x;
        let gx = buildings[idx].x + div_round(bwidth, 2) - XADJ;
        let gy = buildings[idx].y - YADJ;
        *slot = (gx, gy);
        buf.put(gx, gy, &down, None);
    }
    pos
}

/// Integer division with QBasic-style rounding (used where a float `/` result
/// is assigned to an integer variable under `DEFINT`).
fn div_round(a: i32, b: i32) -> i32 {
    (a as f64 / b as f64).round() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cityscape_fills_width() {
        let mut buf = IndexBuffer::new();
        let mut rng = QbRng::new();
        rng.randomize(1234);
        let city = make_cityscape(&mut buf, &mut rng);
        assert!(city.buildings.len() >= 5, "expected several buildings");
        // Buildings should progress left-to-right and reach near the right edge.
        let last = city.buildings.last().unwrap();
        assert!(last.x > SCR_W / 2);
        // Gorillas placed within the field, above the bottom.
        for (gx, gy) in city.gorillas {
            assert!(gx > 0 && gx < SCR_W);
            assert!(gy > 0 && gy < BOTTOM_LINE);
        }
    }

    #[test]
    fn wind_in_expected_range() {
        let mut rng = QbRng::new();
        for s in 0..200 {
            rng.randomize(s);
            let w = compute_wind(&mut rng);
            // base -4..5, optionally +/- up to 10
            assert!((-15..=15).contains(&w), "wind {w} out of range");
        }
    }

    #[test]
    fn gorillas_on_opposite_sides() {
        let mut buf = IndexBuffer::new();
        let mut rng = QbRng::new();
        rng.randomize(999);
        let city = make_cityscape(&mut buf, &mut rng);
        assert!(city.gorillas[0].0 < SCR_W / 2);
        assert!(city.gorillas[1].0 > SCR_W / 2);
    }
}
