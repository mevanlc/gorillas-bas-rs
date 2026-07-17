//! Bridge from the game's RGB framebuffer to a ratatui [`Buffer`], using the
//! sub-cell renderer. The field fills the given area (stretched to fit), which
//! matches how the original stretched its EGA screen to the display.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::subcell::{render, Framebuffer, SubcellMode};

/// Draw `fb` into `area` of `buf` using the given sub-cell `mode`.
pub fn draw_field(fb: &Framebuffer, mode: SubcellMode, area: Rect, buf: &mut Buffer) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let cells = render(fb, mode, area.width, area.height);
    for (ry, row) in cells.iter().enumerate() {
        let y = area.y + ry as u16;
        if y >= area.y + area.height {
            break;
        }
        for (cx, cell) in row.iter().enumerate() {
            let x = area.x + cx as u16;
            if x >= area.x + area.width {
                break;
            }
            let target = &mut buf[(x, y)];
            let mut tmp = [0u8; 4];
            target.set_symbol(cell.ch.encode_utf8(&mut tmp));
            target.set_fg(Color::Rgb(cell.fg.r, cell.fg.g, cell.fg.b));
            target.set_bg(Color::Rgb(cell.bg.r, cell.bg.g, cell.bg.b));
        }
    }
}
