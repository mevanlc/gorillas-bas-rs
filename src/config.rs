//! Shared runtime configuration, populated from the CLI.

use crate::render::RendererConfig;
use crate::subcell::SubcellMode;

#[derive(Clone, Debug)]
pub struct GameConfig {
    pub p1_name: String,
    pub p2_name: String,
    pub ai: bool,
    pub difficulty: u8,
    pub rounds: i32,
    pub gravity: i32,
    pub wind_fixed: Option<i32>,
    pub seed: Option<u32>,
    pub renderer: RendererConfig,
    /// Accepted for parity with the original; the TUI does not emit sound.
    #[allow(dead_code)]
    pub sound: bool,
}

impl Default for GameConfig {
    fn default() -> Self {
        // Defaults mirror Gorillas.ini (defaultGravity=17, defaultRoundQty=4).
        GameConfig {
            p1_name: "Player 1".into(),
            p2_name: "Computer".into(),
            ai: true,
            difficulty: 3,
            rounds: 4,
            gravity: 17,
            wind_fixed: None,
            seed: None,
            renderer: RendererConfig::Subcell(SubcellMode::Octant),
            sound: true,
        }
    }
}
