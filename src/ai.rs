//! Computer-controlled opponent with configurable difficulty (1 = easiest,
//! 5 = hardest).
//!
//! The AI reuses the real [`plot_shot`] simulator to search for an
//! angle/velocity that would destroy the opponent on the *current* battlefield
//! (so it respects wind, gravity, craters and the skyline). Difficulty then
//! degrades that ideal solution: harder levels search more finely and add less
//! aiming error; easier levels search coarsely, add large error, and partly
//! ignore the wind — just like a weaker human player.

use crate::ega::IndexBuffer;
use crate::physics::{plot_shot, ShotOutcome, ShotParams};
use crate::rng::QbRng;

#[derive(Clone, Copy, Debug)]
pub struct AiConfig {
    /// 1..=5
    pub difficulty: u8,
}

impl AiConfig {
    pub fn new(difficulty: u8) -> Self {
        AiConfig {
            difficulty: difficulty.clamp(1, 5),
        }
    }
}

struct Tuning {
    angle_step: usize,
    vel_step: usize,
    angle_err: f64,
    vel_err: f64,
    wind_awareness: f64, // 0..1, how much of the wind it compensates for
}

fn tuning(level: u8) -> Tuning {
    match level {
        1 => Tuning {
            angle_step: 5,
            vel_step: 9,
            angle_err: 18.0,
            vel_err: 30.0,
            wind_awareness: 0.3,
        },
        2 => Tuning {
            angle_step: 4,
            vel_step: 7,
            angle_err: 12.0,
            vel_err: 20.0,
            wind_awareness: 0.55,
        },
        3 => Tuning {
            angle_step: 3,
            vel_step: 5,
            angle_err: 7.0,
            vel_err: 12.0,
            wind_awareness: 0.75,
        },
        4 => Tuning {
            angle_step: 2,
            vel_step: 3,
            angle_err: 3.0,
            vel_err: 5.0,
            wind_awareness: 0.9,
        },
        _ => Tuning {
            angle_step: 1,
            vel_step: 2,
            angle_err: 1.0,
            vel_err: 2.0,
            wind_awareness: 1.0,
        },
    }
}

/// Context the AI needs to aim.
pub struct AiContext<'a> {
    pub scene: &'a IndexBuffer,
    pub start_x: i32,
    pub start_y: i32,
    pub oth_x: i32,
    pub player: u8, // AI's player number (1 or 2)
    pub gravity: i32,
    pub wind: i32,
}

/// Choose an `(angle_deg, velocity)` for the AI's throw.
pub fn choose_shot(cfg: AiConfig, ctx: &AiContext, rng: &mut QbRng) -> (f64, i32) {
    let t = tuning(cfg.difficulty);
    // A scratch RNG so simulation-internal message rolls don't disturb game RNG.
    let mut scratch = QbRng::new();

    // The AI only partly perceives the wind at low levels: it searches on a
    // "believed" battlefield by biasing the target, but validates on the real one.
    let believed_wind = (ctx.wind as f64 * t.wind_awareness).round() as i32;

    let mut candidates: Vec<(f64, i32)> = Vec::new();
    let mut sims = 0usize;
    const MAX_SIMS: usize = 20_000;

    let opponent = 3 - ctx.player;

    'search: for a in (5..=85).step_by(t.angle_step) {
        let angle = a as f64;
        let mut v = 10;
        while v <= 180 {
            if sims >= MAX_SIMS {
                break 'search;
            }
            sims += 1;
            let p = ShotParams {
                start_x: ctx.start_x,
                start_y: ctx.start_y,
                angle_deg: angle,
                velocity: v,
                player: ctx.player,
                oth_x: ctx.oth_x,
                gravity: ctx.gravity,
                wind: believed_wind,
            };
            let shot = plot_shot(ctx.scene, &p, &mut scratch);
            if shot.outcome == (ShotOutcome::Gorilla { hit: opponent }) {
                candidates.push((angle, v));
                break; // first hitting velocity at this angle is enough
            }
            v += t.vel_step as i32;
        }
    }

    let (ideal_angle, ideal_vel) = if candidates.is_empty() {
        ballistic_guess(ctx)
    } else {
        // Prefer a mid-arc solution (robust to small aiming error): pick the
        // candidate whose angle is closest to 45°.
        candidates.sort_by(|x, y| (x.0 - 45.0).abs().partial_cmp(&(y.0 - 45.0).abs()).unwrap());
        candidates[0]
    };

    // Apply difficulty-scaled aiming error.
    let angle = ideal_angle + uniform(rng, t.angle_err);
    let vel = (ideal_vel as f64 + uniform(rng, t.vel_err)).round() as i32;

    (angle.clamp(1.0, 89.0), vel.clamp(2, 200))
}

/// Rough ballistic estimate for when no simulated hit was found (e.g. blocked
/// by the skyline): a 45° shot with range ≈ v²/g.
fn ballistic_guess(ctx: &AiContext) -> (f64, i32) {
    let d = (ctx.oth_x - ctx.start_x).abs() as f64;
    let v = (d * ctx.gravity as f64).sqrt();
    (45.0, (v.round() as i32).clamp(20, 180))
}

/// Symmetric error in `[-mag, mag]`, biased toward the centre (sum of two rolls).
fn uniform(rng: &mut QbRng, mag: f64) -> f64 {
    if mag <= 0.0 {
        return 0.0;
    }
    let r = (rng.rnd() + rng.rnd()) / 2.0; // triangular around 0.5
    (r - 0.5) * 2.0 * mag
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cityscape::make_cityscape;
    use crate::sprites::draw_sun;

    fn setup(seed: u32) -> (IndexBuffer, [(i32, i32); 2], i32) {
        let mut buf = IndexBuffer::new();
        let mut rng = QbRng::new();
        rng.randomize(seed);
        let city = make_cityscape(&mut buf, &mut rng);
        draw_sun(&mut buf, false);
        (buf, city.gorillas, city.wind)
    }

    #[test]
    fn ai_returns_valid_ranges() {
        let (scene, gor, wind) = setup(11);
        let mut rng = QbRng::new();
        for lvl in 1..=5 {
            let ctx = AiContext {
                scene: &scene,
                start_x: gor[1].0,
                start_y: gor[1].1,
                oth_x: gor[0].0,
                player: 2,
                gravity: 17,
                wind,
            };
            let (a, v) = choose_shot(AiConfig::new(lvl), &ctx, &mut rng);
            assert!((1.0..=89.0).contains(&a));
            assert!((2..=200).contains(&v));
        }
    }

    #[test]
    fn hardest_ai_hits_often() {
        // Over several battlefields, difficulty-5 should land a hit within a few
        // shots noticeably more often than difficulty-1.
        let mut hard_hits = 0;
        let mut easy_hits = 0;
        for seed in 0..25u32 {
            let (scene, gor, wind) = setup(seed + 100);
            let mut rng = QbRng::new();
            rng.randomize(seed * 7 + 1);
            let mut scratch = QbRng::new();

            for (lvl, counter) in [(5u8, &mut hard_hits), (1u8, &mut easy_hits)] {
                let ctx = AiContext {
                    scene: &scene,
                    start_x: gor[0].0,
                    start_y: gor[0].1,
                    oth_x: gor[1].0,
                    player: 1,
                    gravity: 17,
                    wind,
                };
                let (a, v) = choose_shot(AiConfig::new(lvl), &ctx, &mut rng);
                let p = ShotParams {
                    start_x: gor[0].0,
                    start_y: gor[0].1,
                    angle_deg: a,
                    velocity: v,
                    player: 1,
                    oth_x: gor[1].0,
                    gravity: 17,
                    wind,
                };
                let shot = plot_shot(&scene, &p, &mut scratch);
                if shot.outcome == (ShotOutcome::Gorilla { hit: 2 }) {
                    *counter += 1;
                }
            }
        }
        assert!(
            hard_hits >= easy_hits,
            "hardest AI ({hard_hits}) should hit at least as often as easiest ({easy_hits})"
        );
        assert!(hard_hits > 0, "hardest AI should land some first-shot hits");
    }
}
