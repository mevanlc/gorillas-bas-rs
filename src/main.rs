//! QBasic GORILLAS (Deluxe) — behaviourally-faithful terminal port.
//!
//! The gameplay simulation (cityscape generation, banana physics, scoring,
//! RNG) is ported op-for-op from the original `Gorillas.bas` (EGA / SCREEN 9
//! branch). Graphics are drawn into a 640×350 indexed framebuffer and rendered
//! to the terminal with sub-cell block glyphs or Chafa symbol rendering.

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

use chafa_syms_rs::CanvasMode;
use clap::{CommandFactory, Parser, ValueEnum};

use config::GameConfig;
use render::{
    configure_chafa_threads, parse_chafa_color, parse_chafa_color_mode, ChafaConfig, RendererConfig,
};
use subcell::SubcellMode;

#[derive(Copy, Clone, Debug, ValueEnum)]
enum RenderMode {
    /// 1×2 sub-cells (`▀▄█`) — most compatible.
    Half,
    /// 2×3 sub-cells (Unicode 13 sextants).
    Sextant,
    /// 2×4 sub-cells (Unicode 16 block octants) — highest density.
    Octant,
    /// Chafa's tunable Unicode symbol renderer.
    Chafa,
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

    /// Framebuffer renderer.
    #[arg(long, value_enum, default_value_t = RenderMode::Octant)]
    render: RenderMode,

    /// Chafa color mode: none, 2, 8, 16, 16/8, 240, 256, or full [default: full].
    #[arg(long, value_name = "MODE", value_parser = parse_chafa_color_mode)]
    chafa_colors: Option<CanvasMode>,

    /// Chafa foreground color (rrggbb, #rrggbb, or a basic name) [default: white].
    #[arg(long, value_name = "COLOR", value_parser = parse_chafa_color)]
    chafa_fg: Option<u32>,

    /// Chafa background color (rrggbb, #rrggbb, or a basic name) [default: black].
    #[arg(long, value_name = "COLOR", value_parser = parse_chafa_color)]
    chafa_bg: Option<u32>,

    /// Chafa work/quality factor, 1-9 [default: 5].
    #[arg(long, value_name = "1-9")]
    chafa_work: Option<u32>,

    /// Chafa worker threads (-1 = auto) [default: -1].
    #[arg(long, value_name = "N", allow_hyphen_values = true)]
    chafa_threads: Option<i32>,

    /// Chafa symbol selector (for example: block+border, ascii, or all).
    #[arg(long, value_name = "SPEC")]
    chafa_symbols: Option<String>,

    /// Make Chafa use foreground colors only.
    #[arg(long)]
    chafa_fg_only: bool,

    /// Swap Chafa's foreground and background colors.
    #[arg(long)]
    chafa_invert: bool,

    /// Disable sound cues (accepted for parity; the TUI is silent).
    #[arg(long)]
    no_sound: bool,
}

fn main() -> std::io::Result<()> {
    let cli = Cli::parse();

    let renderer = match renderer_config(&cli) {
        Ok(renderer) => renderer,
        Err(message) => Cli::command()
            .error(clap::error::ErrorKind::ArgumentConflict, message)
            .exit(),
    };
    if matches!(renderer, RendererConfig::Chafa(_)) {
        configure_chafa_threads(cli.chafa_threads.unwrap_or(-1));
    }

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
        renderer,
        sound: !cli.no_sound,
    };

    let mut app = app::App::new(cfg);
    app.run()
}

fn renderer_config(cli: &Cli) -> Result<RendererConfig, String> {
    let uses_chafa_flags = cli.chafa_colors.is_some()
        || cli.chafa_fg.is_some()
        || cli.chafa_bg.is_some()
        || cli.chafa_work.is_some()
        || cli.chafa_threads.is_some()
        || cli.chafa_symbols.is_some()
        || cli.chafa_fg_only
        || cli.chafa_invert;

    match cli.render {
        RenderMode::Half => {
            reject_chafa_flags(uses_chafa_flags)?;
            Ok(RendererConfig::Subcell(SubcellMode::Half))
        }
        RenderMode::Sextant => {
            reject_chafa_flags(uses_chafa_flags)?;
            Ok(RendererConfig::Subcell(SubcellMode::Sextant))
        }
        RenderMode::Octant => {
            reject_chafa_flags(uses_chafa_flags)?;
            Ok(RendererConfig::Subcell(SubcellMode::Octant))
        }
        RenderMode::Chafa => Ok(RendererConfig::Chafa(ChafaConfig::new(
            cli.chafa_colors,
            cli.chafa_fg,
            cli.chafa_bg,
            cli.chafa_work,
            cli.chafa_symbols.as_deref(),
            cli.chafa_fg_only,
            cli.chafa_invert,
        )?)),
    }
}

fn reject_chafa_flags(uses_chafa_flags: bool) -> Result<(), String> {
    if uses_chafa_flags {
        Err("the --chafa-* options require '--render chafa'".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_chafa_renderer_and_prefixed_options() {
        let cli = Cli::try_parse_from([
            "gorillas",
            "--render",
            "chafa",
            "--chafa-symbols",
            "ascii",
            "--chafa-colors",
            "256",
            "--chafa-fg",
            "cyan",
            "--chafa-bg",
            "#102030",
            "--chafa-work",
            "7",
            "--chafa-threads",
            "4",
            "--chafa-fg-only",
            "--chafa-invert",
        ])
        .unwrap();

        assert!(matches!(
            renderer_config(&cli),
            Ok(RendererConfig::Chafa(_))
        ));
        assert_eq!(cli.chafa_colors, Some(CanvasMode::Indexed256));
        assert_eq!(cli.chafa_fg, Some(0x00ffff));
        assert_eq!(cli.chafa_bg, Some(0x102030));
        assert_eq!(cli.chafa_work, Some(7));
        assert_eq!(cli.chafa_threads, Some(4));
        assert!(cli.chafa_fg_only);
        assert!(cli.chafa_invert);
    }

    #[test]
    fn rejects_chafa_options_with_the_builtin_renderer() {
        let cli = Cli::try_parse_from(["gorillas", "--chafa-symbols", "ascii"]).unwrap();
        assert_eq!(
            renderer_config(&cli).unwrap_err(),
            "the --chafa-* options require '--render chafa'"
        );
    }
}
