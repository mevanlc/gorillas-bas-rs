//! QBasic GORILLAS (Deluxe) — behaviourally-faithful terminal port.
//!
//! The gameplay simulation (cityscape generation, banana physics, scoring,
//! RNG) is ported op-for-op from the original `Gorillas.bas` (EGA / SCREEN 9
//! branch). Graphics are drawn into a 640×350 indexed framebuffer and rendered
//! to the terminal with sub-cell block glyphs (half-block / sextant / octant).

mod ai;
mod app;
mod cityscape;
mod config;
mod ega;
mod physics;
mod render;
mod rng;
mod sprites;
mod subcell;
mod subcell_tables;

use clap::{Parser, ValueEnum};

use config::GameConfig;
use subcell::SubcellMode;

#[derive(Copy, Clone, Debug, ValueEnum)]
enum RenderMode {
    /// 1×2 sub-cells (`▀▄█`) — most compatible.
    Half,
    /// 2×3 sub-cells (Unicode 13 sextants).
    Sextant,
    /// 2×4 sub-cells (Unicode 16 block octants) — highest density.
    Octant,
}

impl From<RenderMode> for SubcellMode {
    fn from(m: RenderMode) -> Self {
        match m {
            RenderMode::Half => SubcellMode::Half,
            RenderMode::Sextant => SubcellMode::Sextant,
            RenderMode::Octant => SubcellMode::Octant,
        }
    }
}

/// Behaviourally-faithful terminal port of QBasic GORILLAS.
#[derive(Parser, Debug)]
#[command(name = "gorillas", version, about)]
struct Cli {
    /// Name of player 1 (left gorilla).
    #[arg(long, default_value = "Player 1")]
    player1: String,

    /// Name of player 2 (right gorilla).
    #[arg(long)]
    player2: Option<String>,

    /// Give player 2 to the computer (default unless --two-player).
    #[arg(long)]
    ai: bool,

    /// Make player 2 a second human (disables the AI).
    #[arg(long, conflicts_with = "ai")]
    two_player: bool,

    /// Computer difficulty, 1 (easiest) to 5 (hardest).
    #[arg(long, short, default_value_t = 3, value_parser = clap::value_parser!(u8).range(1..=5))]
    difficulty: u8,

    /// Maximum number of rounds in a match (1-99).
    #[arg(long, short, default_value_t = 4, value_parser = clap::value_parser!(i32).range(1..=99))]
    rounds: i32,

    /// Gravity in m/s² (1-99; competition = 17, Earth ≈ 10).
    #[arg(long, short, default_value_t = 17, value_parser = clap::value_parser!(i32).range(1..=99))]
    gravity: i32,

    /// Fix the wind speed instead of randomising it each round (-15..=15).
    #[arg(long, value_parser = clap::value_parser!(i32).range(-15..=15))]
    wind: Option<i32>,

    /// Seed the RNG for reproducible battlefields.
    #[arg(long)]
    seed: Option<u32>,

    /// Sub-cell rendering density.
    #[arg(long, value_enum, default_value_t = RenderMode::Octant)]
    render: RenderMode,

    /// Disable sound cues (accepted for parity; the TUI is silent).
    #[arg(long)]
    no_sound: bool,
}

fn main() -> std::io::Result<()> {
    let cli = Cli::parse();

    // Player 2 is the computer unless the user asked for two humans.
    let ai = !cli.two_player;
    let p2_default = if ai { "Computer" } else { "Player 2" };

    let cfg = GameConfig {
        p1_name: cli.player1,
        p2_name: cli.player2.unwrap_or_else(|| p2_default.to_string()),
        ai,
        difficulty: cli.difficulty,
        rounds: cli.rounds,
        gravity: cli.gravity,
        wind_fixed: cli.wind,
        seed: cli.seed,
        mode: cli.render.into(),
        sound: !cli.no_sound,
    };

    let mut app = app::App::new(cfg);
    app.run()
}
