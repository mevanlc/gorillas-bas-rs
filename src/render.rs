//! Bridges from the game's RGB framebuffer to a ratatui [`Buffer`]. Both the
//! built-in sub-cell renderer and `chafa-syms-rs` fill the given area (stretched
//! to fit), matching how the original stretched its EGA screen to the display.

use chafa_syms_rs::palette::{INDEX_FG, INDEX_TRANSPARENT};
use chafa_syms_rs::{Canvas, CanvasConfig, CanvasMode, CellOut, PixelType, SymbolMap};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use crate::subcell::{render, Framebuffer, SubcellMode};

const DEFAULT_CHAFA_FG: u32 = 0x00ff_ffff;
const DEFAULT_CHAFA_BG: u32 = 0x0000_0000;
const DEFAULT_CHAFA_WORK: u32 = 5;

/// Configuration for one of the available framebuffer renderers.
#[derive(Clone, Debug)]
pub enum RendererConfig {
    Subcell(SubcellMode),
    Chafa(ChafaConfig),
}

impl RendererConfig {
    pub const fn name(&self) -> &'static str {
        match self {
            RendererConfig::Subcell(SubcellMode::Half) => "half",
            RendererConfig::Subcell(SubcellMode::Sextant) => "sextant",
            RendererConfig::Subcell(SubcellMode::Octant) => "octant",
            RendererConfig::Chafa(_) => "chafa",
        }
    }
}

/// Options shared with the applicable parts of `chafa-syms-cli`.
#[derive(Clone, Debug)]
pub struct ChafaConfig {
    mode: CanvasMode,
    fg: u32,
    bg: u32,
    work_factor: f32,
    fg_only: bool,
    symbol_map: SymbolMap,
}

impl ChafaConfig {
    pub fn new(
        mode: Option<CanvasMode>,
        fg: Option<u32>,
        bg: Option<u32>,
        work: Option<u32>,
        symbols: Option<&str>,
        fg_only: bool,
        invert: bool,
    ) -> Result<Self, String> {
        let mode = mode.unwrap_or(CanvasMode::Truecolor);
        let (mut fg, mut bg) = (
            fg.unwrap_or(DEFAULT_CHAFA_FG),
            bg.unwrap_or(DEFAULT_CHAFA_BG),
        );
        if invert {
            std::mem::swap(&mut fg, &mut bg);
        }

        // Match chafa-syms-cli's default, which removes inverted symbols from
        // color modes but retains them in the two default-color modes.
        let mut symbol_map = SymbolMap::chafa_default();
        if mode != CanvasMode::Fgbg && mode != CanvasMode::FgbgBgfg {
            symbol_map.apply_selectors("-inverted").unwrap();
        }
        if let Some(selectors) = symbols {
            symbol_map
                .apply_selectors(selectors)
                .map_err(|error| error.0)?;
        }

        let work = work.unwrap_or(DEFAULT_CHAFA_WORK).clamp(1, 9);
        Ok(Self {
            mode,
            fg,
            bg,
            work_factor: (work as f32 - 1.0) / 8.0,
            fg_only,
            symbol_map,
        })
    }

    fn canvas_config(&self, width: usize, height: usize) -> CanvasConfig {
        CanvasConfig::new(width, height)
            .mode(self.mode)
            .fg_color(self.fg)
            .bg_color(self.bg)
            .work_factor(self.work_factor)
            .fg_only(self.fg_only)
            .symbol_map(self.symbol_map.clone())
    }
}

/// Parse the values accepted by `chafa-syms-cli --colors`.
pub fn parse_chafa_color_mode(value: &str) -> Result<CanvasMode, String> {
    match value {
        "none" => Ok(CanvasMode::Fgbg),
        "2" => Ok(CanvasMode::FgbgBgfg),
        "8" => Ok(CanvasMode::Indexed8),
        "16" => Ok(CanvasMode::Indexed16),
        "16/8" | "16-8" => Ok(CanvasMode::Indexed16_8),
        "240" => Ok(CanvasMode::Indexed240),
        "256" => Ok(CanvasMode::Indexed256),
        "full" | "truecolor" | "tc" => Ok(CanvasMode::Truecolor),
        _ => Err(format!("unknown color mode '{value}'")),
    }
}

/// Parse the hex and named colors accepted by `chafa-syms-cli`.
pub fn parse_chafa_color(value: &str) -> Result<u32, String> {
    let value = value.trim();
    let named = match value.to_ascii_lowercase().as_str() {
        "black" => Some(0x000000),
        "red" => Some(0xff0000),
        "green" => Some(0x00ff00),
        "yellow" => Some(0xffff00),
        "blue" => Some(0x0000ff),
        "magenta" => Some(0xff00ff),
        "cyan" => Some(0x00ffff),
        "white" => Some(0xffffff),
        "gray" | "grey" => Some(0x808080),
        _ => None,
    };
    if let Some(color) = named {
        return Ok(color);
    }

    let hex = value.strip_prefix('#').unwrap_or(value);
    if hex.len() == 6 {
        if let Ok(color) = u32::from_str_radix(hex, 16) {
            return Ok(color & 0x00ff_ffff);
        }
    }
    Err(format!("invalid color '{value}'"))
}

/// Configure the global worker pool using `chafa-syms-cli --threads` semantics.
pub fn configure_chafa_threads(threads: i32) {
    let threads = if threads >= 0 {
        threads as usize
    } else {
        std::thread::available_parallelism()
            .map(|parallelism| parallelism.get())
            .unwrap_or(1)
            .min(24)
    };
    let _ = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build_global();
}

/// Stateful renderer. Chafa canvases and RGB input storage are reused between
/// frames when the terminal dimensions remain unchanged.
pub struct FieldRenderer {
    backend: Backend,
}

enum Backend {
    Subcell(SubcellMode),
    Chafa(Box<ChafaRenderer>),
}

struct ChafaRenderer {
    config: ChafaConfig,
    canvas: Option<SizedCanvas>,
    rgb: Vec<u8>,
}

struct SizedCanvas {
    width: u16,
    height: u16,
    canvas: Canvas,
}

impl FieldRenderer {
    pub fn new(config: RendererConfig) -> Self {
        let backend = match config {
            RendererConfig::Subcell(mode) => Backend::Subcell(mode),
            RendererConfig::Chafa(config) => Backend::Chafa(Box::new(ChafaRenderer {
                config,
                canvas: None,
                rgb: Vec::new(),
            })),
        };
        Self { backend }
    }

    /// Draw `fb` into `area` of `buf` using the selected renderer.
    pub fn draw_field(&mut self, fb: &Framebuffer, area: Rect, buf: &mut Buffer) {
        match &mut self.backend {
            Backend::Subcell(mode) => draw_subcells(fb, *mode, area, buf),
            Backend::Chafa(renderer) => renderer.draw(fb, area, buf),
        }
    }
}

fn draw_subcells(fb: &Framebuffer, mode: SubcellMode, area: Rect, buf: &mut Buffer) {
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

impl ChafaRenderer {
    fn draw(&mut self, fb: &Framebuffer, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let needs_canvas = self
            .canvas
            .as_ref()
            .is_none_or(|canvas| canvas.width != area.width || canvas.height != area.height);
        if needs_canvas {
            self.canvas = Some(SizedCanvas {
                width: area.width,
                height: area.height,
                canvas: Canvas::new(
                    self.config
                        .canvas_config(area.width as usize, area.height as usize),
                ),
            });
        }

        self.rgb.clear();
        self.rgb.reserve(fb.px.len() * 3);
        for pixel in &fb.px {
            self.rgb.extend_from_slice(&[pixel.r, pixel.g, pixel.b]);
        }

        let canvas = &mut self.canvas.as_mut().unwrap().canvas;
        canvas.draw_all_pixels(
            PixelType::Rgb8,
            &self.rgb,
            fb.width,
            fb.height,
            fb.width * 3,
        );
        draw_chafa_cells(canvas.cells(), self.config.mode, area, buf);
    }
}

fn draw_chafa_cells(cells: &[CellOut], mode: CanvasMode, area: Rect, buf: &mut Buffer) {
    debug_assert_eq!(cells.len(), area.width as usize * area.height as usize);
    for (index, cell) in cells.iter().enumerate() {
        let x = area.x + (index % area.width as usize) as u16;
        let y = area.y + (index / area.width as usize) as u16;
        let target = &mut buf[(x, y)];
        target.reset();

        // A zero codepoint is the continuation of a two-cell-wide glyph. The
        // preceding ratatui cell's symbol width makes its backend skip this cell.
        let Some(symbol) = char::from_u32(cell.c).filter(|symbol| *symbol != '\0') else {
            continue;
        };

        match mode {
            CanvasMode::Truecolor => {
                let fg = direct_color(cell.fg);
                let bg = direct_color(cell.bg);
                if fg == Color::Reset && bg == Color::Reset {
                    continue;
                }
                target.set_char(symbol).set_fg(fg).set_bg(bg);
            }
            CanvasMode::Indexed256
            | CanvasMode::Indexed240
            | CanvasMode::Indexed16
            | CanvasMode::Indexed8
            | CanvasMode::Indexed16_8 => {
                if cell.fg == INDEX_TRANSPARENT as u32 && cell.bg == INDEX_TRANSPARENT as u32 {
                    continue;
                }
                target
                    .set_char(symbol)
                    .set_fg(indexed_color(cell.fg))
                    .set_bg(indexed_color(cell.bg));
            }
            CanvasMode::FgbgBgfg => {
                // This mode uses terminal-default colors plus reversal. Chafa's
                // printer normalizes flat cells to a blank before applying it.
                let symbol = if cell.fg == cell.bg { ' ' } else { symbol };
                target.set_char(symbol);
                if cell.bg == INDEX_FG as u32 {
                    target.set_style(Style::default().add_modifier(Modifier::REVERSED));
                }
            }
            CanvasMode::Fgbg => {
                // `--colors none` emits symbols without color escape sequences.
                target.set_char(symbol);
            }
        }
    }
}

fn direct_color(packed: u32) -> Color {
    let alpha = (packed >> 24) as u8;
    if alpha < 128 {
        Color::Reset
    } else {
        Color::Rgb(
            ((packed >> 16) & 0xff) as u8,
            ((packed >> 8) & 0xff) as u8,
            (packed & 0xff) as u8,
        )
    }
}

fn indexed_color(index: u32) -> Color {
    if index <= u8::MAX as u32 {
        Color::Indexed(index as u8)
    } else {
        Color::Reset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cli_color_modes_and_colors() {
        assert_eq!(parse_chafa_color_mode("16/8"), Ok(CanvasMode::Indexed16_8));
        assert_eq!(parse_chafa_color_mode("tc"), Ok(CanvasMode::Truecolor));
        assert_eq!(parse_chafa_color("#12abEF"), Ok(0x12abef));
        assert_eq!(parse_chafa_color("cyan"), Ok(0x00ffff));
    }

    #[test]
    fn rejects_invalid_symbol_selectors() {
        let result = ChafaConfig::new(
            None,
            None,
            None,
            None,
            Some("definitely-not-a-symbol-set"),
            false,
            false,
        );
        assert!(result.is_err());
    }

    #[test]
    fn chafa_draws_into_a_ratatui_buffer() {
        let mut fb = Framebuffer::new(2, 2);
        fb.px = vec![
            crate::subcell::Rgb::new(255, 0, 0),
            crate::subcell::Rgb::new(0, 255, 0),
            crate::subcell::Rgb::new(0, 0, 255),
            crate::subcell::Rgb::new(255, 255, 255),
        ];
        let config = ChafaConfig::new(None, None, None, None, None, false, false).unwrap();
        let mut renderer = FieldRenderer::new(RendererConfig::Chafa(config));
        let area = Rect::new(0, 0, 2, 1);
        let mut buf = Buffer::empty(area);

        renderer.draw_field(&fb, area, &mut buf);

        assert!(buf.content().iter().any(|cell| {
            cell.symbol() != " " || cell.fg != Color::Reset || cell.bg != Color::Reset
        }));
    }

    #[test]
    fn default_color_foreground_fill_uses_reversal() {
        let area = Rect::new(0, 0, 1, 1);
        let mut buf = Buffer::empty(area);
        let cells = [CellOut {
            c: 'x' as u32,
            fg: INDEX_FG as u32,
            bg: INDEX_FG as u32,
        }];

        draw_chafa_cells(&cells, CanvasMode::FgbgBgfg, area, &mut buf);

        let cell = &buf[(0, 0)];
        assert_eq!(cell.symbol(), " ");
        assert!(cell.modifier.contains(Modifier::REVERSED));
    }
}
