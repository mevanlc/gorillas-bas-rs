//! Terminal application: title screen, match/round loop, human input, shot
//! animation and the game-over summary. Ties the faithful simulation modules to
//! a ratatui/crossterm front-end.

use std::io::{self, Stdout};
use std::time::{Duration, Instant};

use ratatui::backend::CrosstermBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::crossterm::{execute, terminal};
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::Terminal;
use unicode_width::UnicodeWidthStr;

use crate::ai::{choose_shot, AiConfig, AiContext};
use crate::cityscape::{make_cityscape, Cityscape};
use crate::config::GameConfig;
use crate::ega::{IndexBuffer, Palette, SKY};
use crate::physics::{plot_shot, ShotFrame, ShotOutcome, ShotParams};
use crate::render::FieldRenderer;
use crate::rng::QbRng;
use crate::sprites::{banana_sprite, draw_sun, gorilla_sprite, Arms};

type Term = Terminal<CrosstermBackend<Stdout>>;

const BANANA_COLOR: u8 = 3; // bright yellow (sun palette entry); overlay-only

/// RAII guard: restore the terminal on drop / panic.
struct TermGuard;
impl TermGuard {
    fn enter() -> io::Result<Term> {
        terminal::enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(
            stdout,
            terminal::EnterAlternateScreen,
            ratatui::crossterm::cursor::Hide
        )?;
        Terminal::new(CrosstermBackend::new(stdout))
    }
}
impl Drop for TermGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            terminal::LeaveAlternateScreen,
            ratatui::crossterm::cursor::Show
        );
    }
}

pub struct App {
    cfg: GameConfig,
    rng: QbRng,
    pal: Palette,
    scores: [i32; 2],
    last_shot: [(Option<f64>, Option<i32>); 2],
    renderer: FieldRenderer,
}

enum RoundResult {
    Winner(u8),
    Quit,
}

impl App {
    pub fn new(cfg: GameConfig) -> Self {
        let renderer = FieldRenderer::new(cfg.renderer.clone());
        let mut rng = QbRng::new();
        match cfg.seed {
            Some(s) => rng.randomize(s),
            None => rng.randomize(seed_from_time()),
        }
        App {
            cfg,
            rng,
            pal: Palette::game(),
            scores: [0, 0],
            last_shot: [(None, None), (None, None)],
            renderer,
        }
    }

    pub fn run(&mut self) -> io::Result<()> {
        let _guard = TermGuard;
        let mut term = TermGuard::enter()?;

        if !self.title_screen(&mut term)? {
            return Ok(());
        }

        loop {
            self.scores = [0, 0];
            self.last_shot = [(None, None), (None, None)];
            let quit = self.play_match(&mut term)?;
            if quit {
                break;
            }
            if !self.game_over_screen(&mut term)? {
                break;
            }
        }
        Ok(())
    }

    // -- Screens ------------------------------------------------------------

    fn title_screen(&mut self, term: &mut Term) -> io::Result<bool> {
        loop {
            term.draw(|f| {
                let area = f.area();
                let buf = f.buffer_mut();
                fill_bg(buf, area, Color::Rgb(0, 0, 60));
                let lines = [
                    "Q B a s i c   G O R I L L A S",
                    "DELUXE  -  Rust / ratatui port",
                    "",
                    "Your mission is to hit your opponent with an exploding banana",
                    "by varying the angle and power of your throw, taking into",
                    "account wind speed, gravity, and the city skyline.",
                    "",
                    &format!(
                        "{}  vs  {}{}",
                        self.cfg.p1_name,
                        self.cfg.p2_name,
                        if self.cfg.ai {
                            format!("  (CPU difficulty {})", self.cfg.difficulty)
                        } else {
                            String::new()
                        }
                    ),
                    &format!(
                        "Rounds: {}    Gravity: {}    Render: {}",
                        self.cfg.rounds,
                        self.cfg.gravity,
                        self.cfg.renderer.name()
                    ),
                    "",
                    "[P] Play      [Q] Quit",
                ];
                let start = area.height.saturating_sub(lines.len() as u16) / 2;
                for (i, l) in lines.iter().enumerate() {
                    center(buf, area, start + i as u16, l, Color::White);
                }
            })?;
            if let Some(k) = read_key(Duration::from_millis(200))? {
                match k.code {
                    KeyCode::Char('p') | KeyCode::Char('P') | KeyCode::Enter => return Ok(true),
                    KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => return Ok(false),
                    _ => {}
                }
            }
        }
    }

    fn game_over_screen(&mut self, term: &mut Term) -> io::Result<bool> {
        loop {
            term.draw(|f| {
                let area = f.area();
                let buf = f.buffer_mut();
                fill_bg(buf, area, Color::Rgb(0, 0, 60));
                let winner = if self.scores[0] > self.scores[1] {
                    format!("{} wins the match!", self.cfg.p1_name)
                } else if self.scores[1] > self.scores[0] {
                    format!("{} wins the match!", self.cfg.p2_name)
                } else {
                    "The match was a draw!".to_string()
                };
                let score = format!(
                    "{}: {}     {}: {}",
                    self.cfg.p1_name, self.scores[0], self.cfg.p2_name, self.scores[1]
                );
                let lines = [
                    "Game Over!",
                    "",
                    &winner,
                    &score,
                    "",
                    "[Y] Play again    [N] Quit",
                ];
                let start = area.height.saturating_sub(lines.len() as u16) / 2;
                for (i, l) in lines.iter().enumerate() {
                    center(buf, area, start + i as u16, l, Color::White);
                }
            })?;
            if let Some(k) = read_key(Duration::from_millis(200))? {
                match k.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => return Ok(true),
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Char('q') | KeyCode::Esc => {
                        return Ok(false)
                    }
                    _ => {}
                }
            }
        }
    }

    // -- Match / round ------------------------------------------------------

    fn play_match(&mut self, term: &mut Term) -> io::Result<bool> {
        let min_rounds = self.cfg.rounds / 2 + 1;
        let mut game = 0;
        loop {
            match self.play_round(term)? {
                RoundResult::Quit => return Ok(true),
                RoundResult::Winner(w) => {
                    self.scores[(w - 1) as usize] += 1;
                }
            }
            game += 1;
            if game >= self.cfg.rounds
                || self.scores[0] >= min_rounds
                || self.scores[1] >= min_rounds
            {
                break;
            }
        }
        Ok(false)
    }

    fn play_round(&mut self, term: &mut Term) -> io::Result<RoundResult> {
        // Build the battlefield.
        let mut scene = IndexBuffer::new();
        self.rng.randomize(seed_from_time());
        let mut city = make_cityscape(&mut scene, &mut self.rng);
        if let Some(w) = self.cfg.wind_fixed {
            city.wind = w;
        }
        draw_sun(&mut scene, false);

        // Turn order: player 1 throws first, then alternate.
        let mut tosser: u8 = 1;
        loop {
            let sx = city.gorillas[(tosser - 1) as usize].0;
            let sy = city.gorillas[(tosser - 1) as usize].1;
            let oth = city.gorillas[(2 - tosser) as usize].0;

            // Obtain the shot (human or AI).
            let is_ai = self.cfg.ai && tosser == 2;
            let (angle, velocity) = if is_ai {
                let ctx = AiContext {
                    scene: &scene,
                    start_x: sx,
                    start_y: sy,
                    oth_x: oth,
                    player: tosser,
                    gravity: self.cfg.gravity,
                    wind: city.wind,
                };
                let mut aim_rng = QbRng::new();
                aim_rng.randomize(seed_from_time());
                // Briefly show the CPU "thinking".
                self.show_status(term, &scene, &city, tosser, "CPU is aiming...")?;
                std::thread::sleep(Duration::from_millis(350));
                choose_shot(AiConfig::new(self.cfg.difficulty), &ctx, &mut aim_rng)
            } else {
                match self.human_input(term, &scene, &city, tosser)? {
                    Some(v) => v,
                    None => return Ok(RoundResult::Quit),
                }
            };
            self.last_shot[(tosser - 1) as usize] = (Some(angle), Some(velocity));

            // Simulate.
            let params = ShotParams {
                start_x: sx,
                start_y: sy,
                angle_deg: angle,
                velocity,
                player: tosser,
                oth_x: oth,
                gravity: self.cfg.gravity,
                wind: city.wind,
            };
            let shot = plot_shot(&scene, &params, &mut self.rng);

            // Animate and resolve.
            if self.animate_shot(term, &scene, &city, tosser, &shot.frames)? {
                return Ok(RoundResult::Quit);
            }

            match shot.outcome {
                ShotOutcome::Gorilla { hit } => {
                    // Apply the explosion to the scene and celebrate.
                    if let Some((ex, ey)) = shot.explosion {
                        explode_gorilla(&mut scene, ex, ey);
                        let _ = (ex, ey);
                    }
                    // Winner: if the thrower blasted their own gorilla, the
                    // opponent wins (BASIC: PlayerNum = 3 - PlayerNum).
                    let winner = if hit == tosser { 3 - tosser } else { tosser };
                    // Redraw remaining gorillas & victory dance.
                    self.victory_dance(term, &mut scene, &city, winner, hit)?;
                    if let Some(msg) = shot.message {
                        self.flash_message(term, &scene, &city, tosser, &msg)?;
                    }
                    return Ok(RoundResult::Winner(winner));
                }
                ShotOutcome::Building => {
                    if let Some((ex, ey)) = shot.explosion {
                        explode_building(&mut scene, ex, ey);
                    }
                    if let Some(msg) = shot.message {
                        self.flash_message(term, &scene, &city, tosser, &msg)?;
                    }
                }
                ShotOutcome::NoHit => {
                    if let Some(msg) = shot.message {
                        self.flash_message(term, &scene, &city, tosser, &msg)?;
                    }
                }
            }

            tosser = 3 - tosser;
        }
    }

    // -- Rendering helpers --------------------------------------------------

    /// Compose the display buffer (scene clone + sun face + optional banana),
    /// convert to RGB and draw it, then overlay HUD text.
    #[allow(clippy::too_many_arguments)]
    fn draw_scene(
        &mut self,
        term: &mut Term,
        scene: &IndexBuffer,
        city: &Cityscape,
        tosser: u8,
        sun_shock: bool,
        banana: Option<(i32, i32, u8)>,
        prompt: Option<&str>,
    ) -> io::Result<()> {
        let mut display = IndexBuffer {
            w: scene.w,
            h: scene.h,
            px: scene.px.clone(),
        };
        if sun_shock {
            draw_sun(&mut display, true);
        }
        if let Some((bx, by, rot)) = banana {
            let spr = banana_sprite(rot as i32, BANANA_COLOR);
            display.put(bx, by, &spr, Some(0));
        }
        let fb = display.to_rgb(&self.pal);
        let scores = self.scores;
        let p1 = self.cfg.p1_name.clone();
        let p2 = self.cfg.p2_name.clone();
        let wind = city.wind;
        let prompt = prompt.map(|s| s.to_string());
        term.draw(|f| {
            let area = f.area();
            let buf = f.buffer_mut();
            self.renderer.draw_field(&fb, area, buf);
            // HUD: names top corners.
            paint(buf, area, 0, 0, &p1, Color::Rgb(255, 80, 80));
            let p2x = area.width.saturating_sub(p2.len() as u16);
            paint(buf, area, p2x, 0, &p2, Color::Rgb(255, 80, 80));
            // Score bottom-centre.
            let score = format!("{} > Score < {}", scores[0], scores[1]);
            center(
                buf,
                area,
                area.height.saturating_sub(1),
                &score,
                Color::Rgb(120, 200, 255),
            );
            // Wind readout: an ASCII arrow whose shaft length encodes speed and
            // whose head shows direction (e.g. `--=>` blowing right, `<=--` left).
            let wtxt = format!("Wind: {} {}", wind_arrow(wind), wind);
            center(
                buf,
                area,
                area.height.saturating_sub(2),
                &wtxt,
                Color::Rgb(120, 200, 255),
            );
            // Prompt (current tosser side).
            if let Some(p) = &prompt {
                let y = 1;
                if tosser == 1 {
                    paint(buf, area, 0, y, p, Color::White);
                } else {
                    let x = area.width.saturating_sub(p.len() as u16);
                    paint(buf, area, x, y, p, Color::White);
                }
            }
        })?;
        Ok(())
    }

    fn show_status(
        &mut self,
        term: &mut Term,
        scene: &IndexBuffer,
        city: &Cityscape,
        tosser: u8,
        msg: &str,
    ) -> io::Result<()> {
        self.draw_scene(term, scene, city, tosser, false, None, Some(msg))
    }

    fn flash_message(
        &mut self,
        term: &mut Term,
        scene: &IndexBuffer,
        _city: &Cityscape,
        tosser: u8,
        msg: &str,
    ) -> io::Result<()> {
        // Centre banner for ~1.2s.
        let fb = scene.to_rgb(&self.pal);
        let scores = self.scores;
        let deadline = Instant::now() + Duration::from_millis(1200);
        while Instant::now() < deadline {
            term.draw(|f| {
                let area = f.area();
                let buf = f.buffer_mut();
                self.renderer.draw_field(&fb, area, buf);
                let score = format!("{} > Score < {}", scores[0], scores[1]);
                center(
                    buf,
                    area,
                    area.height.saturating_sub(1),
                    &score,
                    Color::Rgb(120, 200, 255),
                );
                center(buf, area, 0, msg, Color::Rgb(120, 255, 120));
            })?;
            if read_key(Duration::from_millis(60))?.is_some() {
                break;
            }
        }
        let _ = tosser;
        Ok(())
    }

    /// Returns true if the user asked to quit during animation.
    fn animate_shot(
        &mut self,
        term: &mut Term,
        scene: &IndexBuffer,
        city: &Cityscape,
        tosser: u8,
        frames: &[ShotFrame],
    ) -> io::Result<bool> {
        if frames.is_empty() {
            return Ok(false);
        }
        // Target ~1.4s total, clamped to a comfortable per-frame delay.
        let per = (1400 / frames.len().max(1)).clamp(6, 24) as u64;
        for fr in frames {
            self.draw_scene(term, scene, city, tosser, fr.sun_shock, fr.banana, None)?;
            if let Some(k) = read_key(Duration::from_millis(per))? {
                match k.code {
                    KeyCode::Char('q') | KeyCode::Char('Q') => return Ok(true),
                    KeyCode::Esc => break, // skip the rest of the animation
                    _ => {}
                }
            }
        }
        Ok(false)
    }

    fn victory_dance(
        &mut self,
        term: &mut Term,
        scene: &mut IndexBuffer,
        city: &Cityscape,
        winner: u8,
        hit: u8,
    ) -> io::Result<()> {
        // Remove the destroyed gorilla from the scene already done by caller.
        let _ = hit;
        let (gx, gy) = city.gorillas[(winner - 1) as usize];
        for i in 0..4 {
            let arms = if i % 2 == 0 {
                Arms::LeftUp
            } else {
                Arms::RightUp
            };
            let spr = gorilla_sprite(arms);
            let mut display = IndexBuffer {
                w: scene.w,
                h: scene.h,
                px: scene.px.clone(),
            };
            display.put(gx, gy, &spr, None);
            let fb = display.to_rgb(&self.pal);
            let scores = self.scores;
            term.draw(|f| {
                let area = f.area();
                let buf = f.buffer_mut();
                self.renderer.draw_field(&fb, area, buf);
                let score = format!("{} > Score < {}", scores[0], scores[1]);
                center(
                    buf,
                    area,
                    area.height.saturating_sub(1),
                    &score,
                    Color::Rgb(120, 200, 255),
                );
            })?;
            if read_key(Duration::from_millis(200))?.is_some() {
                break;
            }
        }
        Ok(())
    }

    // -- Human input --------------------------------------------------------

    fn human_input(
        &mut self,
        term: &mut Term,
        scene: &IndexBuffer,
        city: &Cityscape,
        tosser: u8,
    ) -> io::Result<Option<(f64, i32)>> {
        let (prev_a, prev_v) = self.last_shot[(tosser - 1) as usize];
        let angle = match self.read_number(
            term,
            scene,
            city,
            tosser,
            "Angle:",
            prev_a.map(|a| a as i32),
            360,
        )? {
            Some(a) => a,
            None => return Ok(None),
        };
        let velocity =
            match self.read_number(term, scene, city, tosser, "Velocity:", prev_v, 200)? {
                Some(v) => v,
                None => return Ok(None),
            };
        Ok(Some((angle as f64, velocity.max(1))))
    }

    /// Read an integer in `1..=max`. Enter confirms (empty reuses `prev`),
    /// Esc aborts the round, `q` quits the game (returns None either way, with
    /// the distinction surfaced by the caller's Quit handling).
    #[allow(clippy::too_many_arguments)]
    fn read_number(
        &mut self,
        term: &mut Term,
        scene: &IndexBuffer,
        city: &Cityscape,
        tosser: u8,
        label: &str,
        prev: Option<i32>,
        max: i32,
    ) -> io::Result<Option<i32>> {
        let mut entry = String::new();
        loop {
            let shown = if entry.is_empty() {
                match prev {
                    Some(p) => format!("{label} {p}_ (Enter=reuse)"),
                    None => format!("{label} _"),
                }
            } else {
                format!("{label} {entry}_")
            };
            self.draw_scene(term, scene, city, tosser, false, None, Some(&shown))?;

            if let Some(k) = read_key(Duration::from_millis(200))? {
                match k.code {
                    KeyCode::Char(c) if c.is_ascii_digit() => {
                        if entry.len() < 3 {
                            let cand = format!("{entry}{c}");
                            if cand.parse::<i32>().unwrap_or(i32::MAX) <= max {
                                entry = cand;
                            }
                        }
                    }
                    KeyCode::Backspace => {
                        entry.pop();
                    }
                    KeyCode::Enter => {
                        if entry.is_empty() {
                            if let Some(p) = prev {
                                return Ok(Some(p));
                            }
                        } else if let Ok(v) = entry.parse::<i32>() {
                            if v >= 1 && v <= max {
                                return Ok(Some(v));
                            }
                            entry.clear();
                        }
                    }
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => return Ok(None),
                    _ => {}
                }
            }
        }
    }
}

// -- Free helpers -----------------------------------------------------------

fn seed_from_time() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u32)
        .unwrap_or(1)
}

fn read_key(timeout: Duration) -> io::Result<Option<KeyEvent>> {
    if event::poll(timeout)? {
        if let Event::Key(k) = event::read()? {
            if k.kind != KeyEventKind::Release {
                return Ok(Some(k));
            }
        }
    }
    Ok(None)
}

fn fill_bg(buf: &mut Buffer, area: Rect, color: Color) {
    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            let c = &mut buf[(x, y)];
            c.set_symbol(" ");
            c.set_bg(color);
        }
    }
}

fn paint(buf: &mut Buffer, area: Rect, x: u16, y: u16, text: &str, fg: Color) {
    if y >= area.height {
        return;
    }
    for (i, ch) in text.chars().enumerate() {
        let cx = x + i as u16;
        if cx >= area.width {
            break;
        }
        let abs_x = area.x + cx;
        let abs_y = area.y + y;
        // If this position is the continuation of a Chafa wide glyph, make its
        // leading cell narrow so ratatui does not skip the HUD character.
        if abs_x > area.x && buf[(abs_x - 1, abs_y)].symbol().width() > 1 {
            buf[(abs_x - 1, abs_y)].set_symbol(" ");
        }
        let cell = &mut buf[(abs_x, abs_y)];
        let mut tmp = [0u8; 4];
        cell.set_symbol(ch.encode_utf8(&mut tmp));
        cell.set_fg(fg);
    }
}

fn center(buf: &mut Buffer, area: Rect, y: u16, text: &str, fg: Color) {
    let len = text.chars().count() as u16;
    let x = area.width.saturating_sub(len) / 2;
    paint(buf, area, x, y, text, fg);
}

/// Render the wind as an ASCII arrow: a shaft of `-` whose length is the wind
/// speed, with a `=>`/`<=` head showing which way it blows. Calm wind (`0`) is a
/// short bar. Shaft length is capped so it never overruns a narrow HUD.
fn wind_arrow(wind: i32) -> String {
    let mag = wind.unsigned_abs().min(20) as usize;
    if mag == 0 {
        return "--".to_string();
    }
    let shaft = "-".repeat(mag);
    if wind > 0 {
        format!("{shaft}=>")
    } else {
        format!("<={shaft}")
    }
}

/// Explosion crater for a gorilla kill (clears a tall region to sky, mirroring
/// `ExplodeGorilla` which clears up to ~24*SclX≈48 pixels tall).
fn explode_gorilla(scene: &mut IndexBuffer, x: i32, y: i32) {
    scene.disc(x as f64, y as f64, 26.0, SKY, 1.7);
}

/// Building-hit crater: `DoExplosion` clears ~1.175 * newExplosionRadius (≈47).
fn explode_building(scene: &mut IndexBuffer, x: i32, y: i32) {
    scene.disc(x as f64, y as f64, 47.0, SKY, 1.0);
}
