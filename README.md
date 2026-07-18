# gorillas-bas-rs

A behaviorally-faithful terminal port of the QBasic **GORILLAS 2.2 (Deluxe)**
banana-throwing artillery game, built with `clap` + `crossterm` + `ratatui`.

The original ran in EGA `SCREEN 9` (640×350, 16 colours). This port keeps the
game logic on that exact coordinate system and renders it into your terminal
using Unicode **sub-cell** block glyphs or the optional `chafa-syms-rs`
renderer, so the physics, cityscape generation, scoring, and "feel" match the
1991 QBasic source op-for-op — while the picture scales to whatever terminal
size you have.

It also adds an optional **computer opponent** with five difficulty levels.

## Building & running

```sh
cargo build --release
./target/release/gorillas          # you (left) vs. the computer (right)
```

By default player 1 is a human and player 2 is the AI. Press **P** / **Enter**
at the title screen to play, **Q** / **Esc** to quit.

## Controls

The game is played the way the original was — by typing two numbers per turn:

| Prompt      | Input                                                      |
|-------------|------------------------------------------------------------|
| `Angle:`    | Launch angle in degrees (0–90), then **Enter**             |
| `Velocity:` | Launch speed, then **Enter**                               |

- Digits type into the current field; **Backspace** edits.
- Each round remembers your previous angle/velocity as the default — just press
  **Enter** to reuse it (as in the original).
- **Q** / **Esc** quits the current game.
- During the banana's flight, **Esc** skips the rest of the animation and **Q**
  quits.

Wind pushes the banana each turn; gravity pulls it down. The wind is shown in the
bottom HUD as an ASCII arrow whose shaft length is the speed and whose head shows
direction — e.g. `--=>` (blowing right) or `<=----` (blowing left). Destroy the
other gorilla to score.

## Command-line options

```
--player1 <NAME>       Name of player 1 (left gorilla)      [default: "Player 1"]
--player2 <NAME>       Name of player 2 (right gorilla)
--ai                   Player 2 is the computer (this is the default)
--two-player           Player 2 is a second human (disables the AI)
-d, --difficulty <1-5> Computer difficulty, 1 easiest … 5 hardest  [default: 3]
-r, --rounds <1-99>    Max rounds in a match                       [default: 4]
-g, --gravity <1-99>   Gravity (competition = 17, Earth ≈ 10)      [default: 17]
--wind <-15..=15>      Fix the wind instead of randomising per round
--seed <N>             Seed the RNG for a reproducible battlefield
--render <MODE>        Renderer: half | sextant | octant | chafa   [default: octant]
--chafa-colors <MODE>  Color mode: none | 2 | 8 | 16 | 16/8 | 240 | 256 | full
--chafa-fg <COLOR>     Default foreground color                    [default: white]
--chafa-bg <COLOR>     Default background color                    [default: black]
--chafa-work <1-9>     Work/quality factor                         [default: 5]
--chafa-threads <N>    Worker threads; -1 selects automatically    [default: -1]
--chafa-symbols <SPEC> Chafa-style symbol selector
--chafa-fg-only        Use foreground colors only
--chafa-invert         Swap foreground and background colors
--no-sound             Accepted for parity; the TUI is silent
```

Examples:

```sh
gorillas --two-player                        # two humans, hot-seat
gorillas -d 5                                # fight the toughest computer
gorillas --render sextant                    # if your font lacks octants
gorillas --render chafa                      # Chafa's default symbol rendering
gorillas --render chafa --chafa-symbols ascii --chafa-colors 16
gorillas --seed 1234 --wind 7 -g 10          # reproducible, moon-ish, fixed wind
```

## Rendering modes

The 640×350 EGA framebuffer is scaled and reduced to a grid of terminal cells.
Each built-in mode picks its two dominant colours and packs the sub-pixels into
a Unicode block glyph. Three densities and the Chafa renderer are available via
`--render`:

| Mode      | Sub-cells | Glyphs                              | Notes                         |
|-----------|-----------|-------------------------------------|-------------------------------|
| `half`    | 1×2       | `▀ ▄ █`                              | Universally supported         |
| `sextant` | 2×3       | Unicode 13 "legacy computing"       | Needs a sextant-capable font  |
| `octant`  | 2×4       | Unicode 16 block octants (default)  | Highest density; newest fonts |
| `chafa`   | 8×8 match | Tunable Unicode symbol set          | Most accurate; more CPU work  |

If the picture looks like boxes/tofu, your terminal font lacks the glyphs — drop
to `--render sextant` or `--render half`. A larger terminal window always yields
a sharper image, since more cells means more sub-pixels.

Chafa rendering uses the pure-Rust
[`chafa-syms-rs`](https://crates.io/crates/chafa-syms-rs) library. Its applicable
[`chafa-syms-cli`](https://crates.io/crates/chafa-syms-cli) controls are exposed
with a `--chafa-` prefix and require `--render chafa`. `--chafa-symbols` accepts
the CLI's selector syntax, including named sets such as `block`, `border`,
`ascii`, `braille`, `sextant`, `octant`, and `all`. Colors accept `rrggbb`,
`#rrggbb`, or the basic names `black`, `red`, `green`, `yellow`, `blue`,
`magenta`, `cyan`, `white`, and `gray`/`grey`.

The standalone CLI's size, scale, and font-ratio options do not apply because
GORILLAS always stretches its 640×350 framebuffer to the current ratatui
viewport. Its optimize and format options only control ANSI serialization,
which this integration bypasses by writing Chafa's cells directly to ratatui.

## Fidelity notes

What was ported directly from `Gorillas.bas` (EGA / `SCREEN 9` branch):

- **RNG.** QBasic's LCG (`seed = seed*0xFD43FD + 0xC39EC3 mod 2²⁴`), `RANDOMIZE`,
  `RND`, and `FNRan` are reproduced exactly, so seeded battlefields match.
- **Cityscape.** `MakeCityScape` — slope selection, building widths/heights,
  window placement and colours, and the shadowed `CASE` quirks — is preserved.
- **Gorilla placement.** `PlaceGorillas` picks the 2nd/3rd buildings from each
  edge, as in the source.
- **Physics.** `PlotShot`'s ballistic integration is ported step-for-step:
  the `x = x₀ + Vx·t + ½·(Wind/5)·t²`, `y = y₀ + (−Vy·t + ½·g·t²)·(H/350)`
  update, the `t += 0.1` stepping, bottom-of-screen bounce, off-screen miss
  handling, the player-2 angle mirror, and the leading-edge `POINT` collision
  probes against an indexed framebuffer.
- **Sun.** The sun reacts (shocked face) when a banana passes in front of it,
  exactly where the BASIC checked `SunHt`.
- **EGA palette.** DOSBox's 6-bit-value→RGB formula is used, so on-screen colours
  match what DOSBox shows.
- **Scoring & flow.** Round/point handling and the "reuse previous shot" default
  follow the original. Defaults mirror `Gorillas.ini` (gravity 17, 4 rounds).

Collision uses an **indexed** framebuffer whose colour indices carry gameplay
meaning (sky, gorilla/object, explosion, sun) — just like the original reading
pixels back with `POINT`. RGB is derived only for display. The banana is an
overlay drawn per frame, never baked into the collision scene (so it can't
collide with itself).

## The computer opponent

The AI reuses the **real** `plot_shot` simulator, so it aims on the actual
battlefield — respecting the current wind, gravity, craters, and skyline. It
grid-searches angle/velocity for a shot that would destroy the opponent, then
degrades that ideal by difficulty:

| Difficulty | Search resolution | Aiming error | Wind awareness |
|-----------:|-------------------|--------------|----------------|
| 1 (easiest)| coarse            | large        | ~30%           |
| 2          | ↓                 | ↓            | ~55%           |
| 3 (default)| medium            | medium       | ~75%           |
| 4          | ↑                 | ↓            | ~90%           |
| 5 (hardest)| fine              | small        | 100%           |

At level 5 it searches finely, adds almost no error, and fully compensates for
wind; at level 1 it searches coarsely, misjudges its aim, and only partly reads
the wind — like a weaker human player. Aiming error is triangular (biased toward
centre), and a separate scratch RNG keeps the AI's internal simulations from
disturbing the game's RNG stream.

## Project layout

| File                   | Responsibility                                              |
|------------------------|-------------------------------------------------------------|
| `src/rng.rs`           | QBasic-compatible LCG (`RND`, `RANDOMIZE`, `FNRan`)         |
| `src/ega.rs`           | EGA palette, indexed framebuffer, drawing primitives, `POINT`|
| `src/sprites.rs`       | Gorilla, sun, and banana sprites                            |
| `src/cityscape.rs`     | `MakeCityScape`, wind, `PlaceGorillas`                      |
| `src/physics.rs`       | `PlotShot` trajectory & collision                          |
| `src/ai.rs`            | Configurable computer opponent                             |
| `src/subcell.rs`       | RGB → terminal-cell reduction (half/sextant/octant)        |
| `src/subcell_tables.rs`| Generated bitmask → Unicode block-glyph lookup tables      |
| `src/render.rs`        | Built-in/Chafa framebuffer rendering into ratatui           |
| `src/config.rs`        | Game configuration (mirrors `Gorillas.ini`)                |
| `src/app.rs`           | Terminal setup, game loop, input, animation                |
| `src/main.rs`          | `clap` CLI → `GameConfig` → run                            |

## Testing

```sh
cargo nextest run # unit tests: RNG, palette, cityscape, physics, AI, rendering
cargo clippy      # lint-clean
```
