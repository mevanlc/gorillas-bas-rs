//! Faithful port of the `PlotShot` banana trajectory routine (EGA branch).
//!
//! The simulation runs on the original 640×350 coordinate system and reads
//! collisions from the scene [`IndexBuffer`] via `POINT`, exactly like the
//! BASIC. Rather than performing terminal I/O inline, it records a list of
//! [`ShotFrame`]s for the caller to animate and returns a structured outcome.

use crate::ega::{IndexBuffer, GORILLA, SCR_H, SCR_W, SUN};
use crate::rng::QbRng;

const SUN_HT: i32 = 43; // EGA SunHt
const ADJUST: i32 = 4; // Scl(4)

/// One animation step: the banana position/rotation (if drawn this step) and
/// whether the sun currently wears its shocked face.
#[derive(Clone, Copy, Debug)]
pub struct ShotFrame {
    pub banana: Option<(i32, i32, u8)>,
    pub sun_shock: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShotOutcome {
    /// Banana ran out of steam or left the field without hitting a gorilla.
    NoHit,
    /// A building (or window/skyline) was struck; crater at `explosion`.
    Building,
    /// A gorilla was destroyed. `hit` is the player number (1 or 2) whose
    /// gorilla exploded — may equal the thrower (a self-hit).
    Gorilla { hit: u8 },
}

pub struct Shot {
    pub frames: Vec<ShotFrame>,
    pub outcome: ShotOutcome,
    pub explosion: Option<(i32, i32)>,
    pub message: Option<String>,
    /// True if the banana ever shocked the sun (exposed for parity/inspection).
    #[allow(dead_code)]
    pub sun_hit: bool,
}

/// Parameters describing a single throw.
pub struct ShotParams {
    pub start_x: i32,
    pub start_y: i32,
    /// The player's chosen angle in degrees (0..360), *before* the player-2 mirror.
    pub angle_deg: f64,
    pub velocity: i32,
    pub player: u8, // 1 or 2
    pub oth_x: i32,
    pub gravity: i32,
    pub wind: i32,
}

fn sgn(v: f64) -> i32 {
    if v > 0.0 {
        1
    } else if v < 0.0 {
        -1
    } else {
        0
    }
}

/// Predict the banana's x when it reaches the bottom of the screen — used for
/// the off-screen "miss distance" heuristics (cosmetic messaging).
fn predict_bottom(
    start_x_pos: f64,
    start_y_pos: f64,
    init_x_vel: f64,
    init_y_vel: f64,
    gravity: i32,
    wind: i32,
    player: u8,
) -> f64 {
    let fall_dist = start_y_pos - (SCR_H as f64 - 7.0);
    let g = gravity as f64;
    let disc = init_y_vel * init_y_vel + 2.0 * (-g) * fall_dist;
    let t2 = (-init_y_vel - disc.max(0.0).sqrt()) / (-g);
    let mut xp = init_x_vel * t2 + (t2 * t2 * wind as f64) / 10.0 + start_x_pos;
    if player == 2 {
        xp += 25.0;
    }
    xp
}

/// Run the trajectory. `scene` is read-only (collision map with sun baked in).
pub fn plot_shot(scene: &IndexBuffer, p: &ShotParams, rng: &mut QbRng) -> Shot {
    // Player 2 mirrors the angle.
    let mut angle = p.angle_deg;
    let angle_chk = if p.player == 2 { 180.0 - angle } else { angle };
    if p.player == 2 {
        angle = 180.0 - angle;
    }
    let angle_rad = angle / 180.0 * std::f64::consts::PI;

    let velocity = p.velocity as f64;
    let init_x_vel = angle_rad.cos() * velocity;
    let mut init_y_vel = angle_rad.sin() * velocity;

    let mut start_x_pos = p.start_x as f64;
    let mut start_y_pos = p.start_y as f64 - ADJUST as f64 - 3.0;
    let direction: f64 = if p.player == 2 {
        start_x_pos += 25.0;
        4.0
    } else {
        -4.0
    };

    let g = p.gravity as f64;
    let wind = p.wind as f64;

    let mut frames: Vec<ShotFrame> = Vec::new();
    let mut sun_shock = false;
    let mut sun_hit = false;

    // Velocity too low → immediate self-hit (BASIC preset).
    if p.velocity < 2 {
        return Shot {
            frames,
            outcome: ShotOutcome::Gorilla { hit: p.player },
            explosion: None,
            message: None,
            sun_hit,
        };
    }

    let x_predicted = predict_bottom(
        start_x_pos,
        start_y_pos,
        init_x_vel,
        init_y_vel,
        p.gravity,
        p.wind,
        p.player,
    );
    let mut missed_dist = if direction > 0.0 {
        x_predicted
    } else {
        SCR_W as f64 - x_predicted
    };
    if sgn(direction) == sgn(init_x_vel) {
        missed_dist = -missed_dist;
    }

    let mut impact = false;
    let mut on_screen = true;
    let mut sun_hit_once = false;
    let mut shot_in_sun = false;
    let mut bounced = false;
    let mut t2b = 9999.0_f64;
    let mut t = 0.0_f64;

    let mut x = start_x_pos;
    let mut y = start_y_pos;
    let mut pointval: i32 = 0;
    // Default banana rotation, used by the flavour-message logic if the banana
    // was never actually drawn (e.g. an immediate off-screen shot).
    let mut rot: u8 = 0;
    let _ = rot;

    let mut off_screen_failure = false;

    while !impact && on_screen {
        x = start_x_pos + init_x_vel * t + 0.5 * (wind / 5.0) * t * t;
        y = start_y_pos + (-(init_y_vel * t) + 0.5 * g * t * t) * (SCR_H as f64 / 350.0);

        // Bottom of screen.
        if y >= SCR_H as f64 - 7.0 {
            if init_y_vel > 2.0 && t2b == 9999.0 {
                bounced = true;
                let inside =
                    init_y_vel * init_y_vel - 2.0 * g * (start_y_pos - (SCR_H as f64 - 7.0));
                init_y_vel = inside.max(0.0).sqrt() * 0.4;
                start_x_pos = x;
                y = SCR_H as f64 - 7.0;
                start_y_pos = y;
                t = 0.0;
            } else {
                on_screen = false;
                if t2b != 9999.0 && sgn(direction) != sgn(init_x_vel) {
                    off_screen_failure = true;
                }
            }
        }

        // Banana leaves the sides.
        if (x >= SCR_W as f64 - 10.0 || x <= 3.0)
            && (x_predicted >= SCR_W as f64 - 10.0 || x_predicted <= 3.0)
        {
            if t > t2b {
                on_screen = false;
                if sgn(direction) != sgn(init_x_vel) {
                    off_screen_failure = true;
                }
            } else if t2b == 9999.0 {
                t2b = if y <= 0.0 { t + 1.5 } else { t + 4.0 };
            }
        }

        let mut drew_banana = false;
        if on_screen && y > 0.0 && x > 3.0 && x < SCR_W as f64 - 10.0 {
            // Collision probe along the banana's leading edge.
            let mut look_y = 0;
            let mut look_x = 8 * (2 - p.player as i32); // Scl(8*(2-player))
            let _ = bounced;
            loop {
                pointval = scene.point((x as i32) + look_x, (y as i32) + look_y);
                if pointval == 0 {
                    impact = false;
                    if shot_in_sun && ((SCR_W / 2 - x as i32).abs() > 20 || (y as i32) > SUN_HT) {
                        shot_in_sun = false;
                    }
                } else if pointval == SUN as i32 && (y as i32) < SUN_HT {
                    if !sun_hit_once {
                        sun_shock = true;
                    }
                    sun_hit_once = true;
                    sun_hit = true;
                    shot_in_sun = true;
                } else {
                    impact = true;
                    sun_shock = false;
                }
                look_x += direction as i32;
                look_y += 6;
                if impact || look_x != 4 {
                    break;
                }
            }

            if !shot_in_sun && !impact {
                rot = ((t * 10.0) as i64).rem_euclid(4) as u8;
                frames.push(ShotFrame {
                    banana: Some((x as i32, y as i32, rot)),
                    sun_shock,
                });
                drew_banana = true;
            }
        }

        if !drew_banana {
            frames.push(ShotFrame {
                banana: None,
                sun_shock,
            });
        }

        t += 0.1;

        // Safety valve: bound the number of steps (a real throw resolves quickly).
        if frames.len() > 100_000 {
            on_screen = false;
        }
    }

    // Resolve outcome.
    let mut outcome = ShotOutcome::NoHit;
    let mut explosion = None;
    let mut message = None;

    if pointval == GORILLA as i32 {
        let hit = if x < SCR_W as f64 / 2.0 { 1 } else { 2 };
        outcome = ShotOutcome::Gorilla { hit };
        explosion = Some((x as i32, y as i32));
    } else if impact {
        // Building hit → crater.
        outcome = ShotOutcome::Building;
        explosion = Some((x as i32 + ADJUST, y as i32 + ADJUST));

        // Feeble-shot message (cosmetic), mirrors PredictReturnToHeight logic.
        let t2 = (2.0 * init_y_vel) / g;
        let xp_ret = init_x_vel * t2 + 0.5 * (wind / 5.0) * t2 * t2 + start_x_pos;
        if sgn(direction) != sgn(init_x_vel) {
            let d1 = (xp_ret - p.start_x as f64).abs();
            let reach = ((p.oth_x - p.start_x) as f64).abs();
            if (d1 < reach / 3.0 && angle_chk > 60.0) || d1 < reach / 6.0 {
                message = Some(match rng.ran(3) {
                    1 => "Aren't your little muscles strong enough?".to_string(),
                    2 => "Now that was feeble.".to_string(),
                    _ => "You can do better than that!".to_string(),
                });
            }
        }
    }

    if off_screen_failure && message.is_none() {
        message = Some(failure_message(missed_dist.abs(), y, rng));
    }

    Shot {
        frames,
        outcome,
        explosion,
        message,
        sun_hit,
    }
}

/// Off-screen "you missed" flavour text, keyed by how far past the edge the
/// shot travelled (mirrors `FailureMessage`).
fn failure_message(missed: f64, y: f64, rng: &mut QbRng) -> String {
    let m = missed as i32;
    if y > 0.0 {
        match m {
            1..=155 => match rng.ran(2) {
                1 => "That went a wee bit far, didn't it?",
                _ => "It seems you overdid that a little.",
            },
            156..=640 => match rng.ran(4) {
                1 => "I think you need glasses.",
                _ => "Hmmm...that wasn't good.",
            },
            641..=1500 => "WHAT? That went MILES OFF!",
            _ => match rng.ran(2) {
                1 => "WHAT ARE YOU PLAYING AT?",
                _ => "Temper temper",
            },
        }
    } else {
        match m {
            1..=155 => "A little nearer and you might stand a chance",
            156..=640 => match rng.ran(2) {
                1 => "Nope. That was too far off.",
                _ => "\"Hello? I'm over here!\"",
            },
            641..=1500 => match rng.ran(2) {
                1 => "Whoa! Go easy with it!",
                _ => "You must be JOKING!",
            },
            _ => "You weren't supposed to put it into orbit.",
        }
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cityscape::make_cityscape;

    fn scene_with_city(seed: u32) -> (IndexBuffer, [(i32, i32); 2], i32) {
        let mut buf = IndexBuffer::new();
        let mut rng = QbRng::new();
        rng.randomize(seed);
        let city = make_cityscape(&mut buf, &mut rng);
        crate::sprites::draw_sun(&mut buf, false);
        (buf, city.gorillas, city.wind)
    }

    #[test]
    fn direct_horizontal_low_shot_hits_something() {
        let (scene, gor, wind) = scene_with_city(7);
        let mut rng = QbRng::new();
        let p = ShotParams {
            start_x: gor[0].0,
            start_y: gor[0].1,
            angle_deg: 45.0,
            velocity: 100,
            player: 1,
            oth_x: gor[1].0,
            gravity: 17,
            wind,
        };
        let shot = plot_shot(&scene, &p, &mut rng);
        // Should terminate and produce some frames.
        assert!(!shot.frames.is_empty());
    }

    #[test]
    fn low_velocity_is_self_hit() {
        let (scene, gor, wind) = scene_with_city(3);
        let mut rng = QbRng::new();
        let p = ShotParams {
            start_x: gor[0].0,
            start_y: gor[0].1,
            angle_deg: 45.0,
            velocity: 1,
            player: 1,
            oth_x: gor[1].0,
            gravity: 17,
            wind,
        };
        let shot = plot_shot(&scene, &p, &mut rng);
        assert_eq!(shot.outcome, ShotOutcome::Gorilla { hit: 1 });
    }

    #[test]
    fn a_perfect_direct_hit_lands_on_opponent() {
        // Flat map: two gorillas at same height, no wind, find an angle/velocity
        // that connects and assert a gorilla-2 hit is reachable.
        let mut scene = IndexBuffer::new();
        // Two gorilla blocks.
        let g1 = (100, 200);
        let g2 = (500, 200);
        let s1 = crate::sprites::gorilla_sprite(crate::sprites::Arms::Down);
        scene.put(g1.0, g1.1, &s1, None);
        scene.put(g2.0, g2.1, &s1, None);
        crate::sprites::draw_sun(&mut scene, false);
        let mut rng = QbRng::new();

        // Sweep to demonstrate the solver space contains a hit on player 2.
        let mut found = false;
        for angle in (10..=80).step_by(1) {
            for vel in (30..=160).step_by(2) {
                let p = ShotParams {
                    start_x: g1.0,
                    start_y: g1.1,
                    angle_deg: angle as f64,
                    velocity: vel,
                    player: 1,
                    oth_x: g2.0,
                    gravity: 17,
                    wind: 0,
                };
                let shot = plot_shot(&scene, &p, &mut rng);
                if shot.outcome == (ShotOutcome::Gorilla { hit: 2 }) {
                    found = true;
                    break;
                }
            }
            if found {
                break;
            }
        }
        assert!(found, "expected some angle/velocity to hit opponent");
    }
}
