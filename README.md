# Maze Wars

A high-performance, multiplayer first-person shooter recreation of the classic 1973/1974 **Maze War** in Rust (2024 edition) using SDL2. Players connect over UDP to a central authoritative server, navigate retro monochrome 2.5D ray-cast mazes, engage in competitive combat with straight-moving projectiles, track real-time scores, and progress through three increasingly difficult levels.

---

## Prerequisites & Installation

Maze Wars requires [Rust](https://www.rust-lang.org/) (2024 edition) and SDL2 development libraries.

### Debian / Ubuntu
```bash
sudo apt-get update
sudo apt-get install -y libsdl2-dev
```

### Fedora / RHEL
```bash
sudo dnf install SDL2-devel
```

### Arch Linux
```bash
sudo pacman -S sdl2
```

### macOS (Homebrew)
```bash
brew install sdl2
```

### Windows
Install `sdl2` via [vcpkg](https://github.com/microsoft/vcpkg) (`vcpkg install sdl2:x64-windows`) or download the development libraries from [libsdl.org](https://www.libsdl.org/) and place the `.lib`/`.dll` files in your library path.

---

## Building & Running

Ensure your terminal is navigated to the project directory containing `Cargo.toml`:
```bash
# Navigate to the project root
cd multiplayer-fps-1
```

### 1. Start the Game Server
The server binds to UDP port `34254` across all interfaces (`0.0.0.0`):
```bash
cargo run --bin server
```
Or for the optimized release build:
```bash
cargo run --release --bin server
```

### 2. Start a Game Client
Run the client in a separate terminal:
```bash
cargo run
```
Or:
```bash
cargo run --bin client
```

Upon launch, the client prompts for:
1. **Server IP Address**: Enter an IP address with optional port (e.g. `127.0.0.1:34254`, `192.168.1.50:34254`, or press Enter to accept the default `127.0.0.1:34254`).
2. **Username**: Enter a player name (1 to 20 UTF-8 bytes).

If the server is unreachable, the client displays a friendly error explaining that the server is not running and suggests starting it with `cargo run --bin server`.

---

## Networking & Multiplayer Architecture

- **Protocol**: UDP datagrams with custom binary serialization prefixed with magic bytes `MZWR` and version `1`.
- **Packet Size Bound**: Compact snapshots keep full 64-player state below 1,400 bytes to avoid MTU fragmentation.
- **Server Binding**: Binds `0.0.0.0:34254` to support localhost, LAN, and WAN clients simultaneously.
- **Client Socket**: Binds an ephemeral local port (`0.0.0.0:0`) with automatic join retransmissions.
- **Tick Rate**: Authoritative 60 Hz simulation loop (`16.6ms` tick duration).
- **Inactivity Timeout**: Players sending no packets for 600 consecutive ticks (10.0 seconds at 60 Hz) are automatically removed, freeing their slots and usernames.

---

## Controls

| Key / Input | Action |
|-------------|--------|
| `W` or `Up Arrow` | Move forward / Menu navigate up |
| `S` or `Down Arrow` | Move backward / Menu navigate down |
| `A` or `Left Arrow` | Turn left |
| `D` or `Right Arrow` | Turn right |
| `Space` or `Left Mouse Button` | Fire shot / Menu activate option |
| `Escape` | Toggle In-Game Pause Menu (options, resume, quit) |
| `E` | Toggle Autonomous Enemies in maze ON / OFF |
| `C` | Toggle Persistent Aiming Crosshair ON / OFF |
| `R` | Retry / Respawn after being caught by an enemy |
| `Return` / `Enter` | Select / Toggle highlighted option in Pause Menu |
| `Q` | Quick quit from Pause Menu or Game Over screen |

Opposing movement keys cancel out deterministically.

---

## Gameplay & Combat Rules

- **Shooting**: Players fire straight-line projectiles traveling at high velocity (`25.0` tiles/s). Shots stop upon hitting a maze wall, opponent, or enemy.
- **Authoritative Combat**: The server computes swept-segment raycasts to prevent tunneling through walls or opponents between ticks.
- **Scoring**:
  - Hitting an opponent awards **+10 points** to the shooter.
  - The hit target is penalized **-5 points** (scores may become negative).
  - Scores persist across level transitions for the duration of the connection.
- **Two-Second Hit Protection**:
  - When hit by an opponent, the target remains in the maze and receives exactly **2.0 seconds** (120 server ticks) of invulnerability.
  - During protection, the player can move and navigate freely to escape, but cannot be hit again.
  - Protected players visually alternate/blink in the first-person view and minimap.
  - Preserves original Maze War rules with no ammo reloading.
- **Autonomous Enemies in the Maze**:
  - Pressing `E` (or enabling in the Pause Menu) spawns autonomous hostile enemies roaming the maze corridors.
  - **Corridor Navigation & AI**: Enemies use BFS pathfinding and direct line-of-sight tracking to hunt the player, with proper orthogonal wall sliding to prevent clipping.
  - **Loss Condition ("You Lose")**: If an alive enemy touches the player, the game transitions to a `"YOU LOSE!"` state, halting movement and prompting the player to Retry (`R`), Disable Enemies (`E`), or open the Pause Menu (`Escape`).
  - **Defeating Enemies & No Respawn**: Shooting an enemy with a straight projectile destroys them instantly (`check_shot_hit`), with bullet tracers stopping cleanly at the impact point. Dead enemies **never respawn back**—once all enemies are eliminated, a `"MAZE CLEARED! ALL ENEMIES DEFEATED"` victory banner is displayed and the maze remains clear.
  - **Midgame Disabling**: Pressing `E` midgame instantly removes all enemies from both the 3D first-person view and the minimap, disabling the AI completely.
  - **Minimap Visualization**: Enemies are tracked on the minimap as high-contrast orange/red diamond markers with yellow core pips.
- **Persistent Aiming Crosshair**:
  - An aiming crosshair is visible continuously at the center of the screen during gameplay to enhance aiming precision.
  - Can be toggled ON or OFF at any time using the `C` keyboard shortcut or via the Pause Menu.
- **In-Game Pause Menu (`Escape`)**:
  - Pressing `Escape` opens an in-game pause modal card styled in retro phosphor aesthetics.
  - Options include:
    1. **`RESUME GAME`**: Resumes active gameplay (or press `Escape`).
    2. **`CROSSHAIR: [ON] / [OFF]`**: Toggles persistent crosshair.
    3. **`ENEMIES: [ON] / [OFF]`**: Toggles autonomous enemies.
    4. **`RESET ENEMIES`**: Re-spawns current level enemies.
    5. **`QUIT GAME`**: Disconnects and quits cleanly.

---

## Mazes & Level Progression

The game features three static ASCII mazes of strictly increasing difficulty:

| Level | Dimensions | Dead Ends | Complexity & Layout |
|:-----:|:----------:|:---------:|---------------------|
| **1** | 15 × 11 | 1 | Open central corridors, simple dead ends |
| **2** | 21 × 15 | 6 | Branched corridors, tight turns, increased dead ends |
| **3** | 25 × 19 | 17 | Dense labyrinth with maximum dead ends |

Dead-end counts are produced by `Maze::count_dead_ends` (interior floor/exit tiles with ≥3 wall neighbours; exit-alcove counts).

- **Exit Progression**: Reaching the exit tile (`E`) immediately triggers an authoritative level transition (`1 -> 2 -> 3 -> 1`).
- **Spawns**: Each maze defines multiple distinct spawn points (5 in Level 1, 6 in Level 2, 8 in Level 3) to prevent initial overlap.
- **Enemy Level Scaling**: When enemy mode is enabled, enemies dynamically adapt to the active level with proportional counts and spawn placement.

---

## User Interface & Performance

- **First-Person View**: Monochrome 2.5D DDA raycaster with distance shading and horizontal wall depth.
- **Eyeball Sprites**: Other players render as floating eyeball sprites with accurate wall occlusion depth buffering.
- **Enemy Sprites**: Autonomous enemies render as menacing crimson/amber shelled entities sorted by distance with wall occlusion.
- **Minimap**: Full overhead map in the upper-left corner showing walls, exit tile, local player in green (`RGB(70, 220, 125)`), opponents in red (`RGB(225, 73, 75)`), and active enemies in orange/red diamond markers (`RGB(255, 120, 20)`).
- **Persistent Crosshair**: High-visibility retro green aiming reticle at the center of the viewport with center pip and corner brackets.
- **HUD & Scoreboard**:
  - Top-right panel displays real-time rolling `FPS` and current `LEVEL`.
  - Top-center panel displays the live scoreboard with usernames and signed scores, sorted descending by score with the local player highlighted.
  - Rendered with a zero-allocation 3×5 bitmap font, ensuring stable 60+ FPS performance (well above the 50 FPS audit requirement).
- **Interactive Retro Dialogs**:
  - Pause Menu modal with keyboard navigation (`Up`/`Down`/`W`/`S`/`Enter`/`Space`/`Escape`/`Q`).
  - Game Over modal on enemy touch with instant Retry (`R`) and Disable (`E`).
  - Congratulatory `"MAZE CLEARED!"` victory banner.

---

## Project Structure & Source Map

```
multiplayer-fps-1/
├── Cargo.toml              # Package definition and dependencies (sdl2)
├── build.rs                # SDL2 linking configuration
├── tests/
│   └── audit_tests.rs      # Comprehensive automated audit verification suite
├── src/
│   ├── lib.rs              # Library root exporting modules
│   ├── bin/
│   │   ├── server.rs       # Server executable entry point
│   │   └── client.rs       # Client CLI prompts and launcher
│   ├── client.rs           # Client networking, connection retry, and snapshot handling
│   ├── enemy.rs            # Autonomous enemy AI, BFS pathfinding, swept combat & collision
│   ├── protocol.rs         # Binary packet encoding/decoding and validation
│   ├── server.rs           # Server UDP loop, 60 Hz tick, and broadcast management
│   ├── world.rs            # Maze layouts, tile queries, spawn positions, dead ends
│   ├── server/
│   │   ├── combat.rs       # Authoritative shooting, swept collision, +10/-5 scoring, protection
│   │   └── state.rs        # Authoritative movement, sliding, level progression, timeouts
│   └── window/
│       ├── hud.rs          # Rolling FPS counter, bitmap font, top-10 scoreboard
│       ├── menu.rs         # In-game pause menu, game-over screen, and victory banners
│       ├── minimap.rs      # Top-down maze minimap with player and enemy indicators
│       └── view.rs         # 2.5D raycaster, distance shading, eye/enemy sprites, crosshair, effects
```

---

## Verification & Automated Checks

All tests, formatting, and linting pass with zero warnings:

```bash
# Verify formatting
cargo fmt --check

# Run full compiler check across all targets
cargo check --all-targets

# Compile debug and release binaries
cargo build
cargo build --release

# Run strict Clippy linter with warnings denied
cargo clippy --all-targets -- -D warnings

# Execute all unit tests and integration audit checks (100 tests)
cargo test
```

For the complete peer audit evaluation guide, question-by-question explanations, and deep-dive technical defenses, see [`docs/audit_guide.md`](docs/audit_guide.md) and [`docs/audit.md`](docs/audit.md).

---

## Team Ownership Breakdown

| Developer | Responsibility Area | Deliverables |
|:---------:|:--------------------|:-------------|
| **Developer 1** | Client-Server Architecture & HUD | UDP network foundation, non-blocking loops, binary protocol wire format, rolling FPS counter, bitmap font rendering |
| **Developer 2** | Levels & World Navigation | Three ASCII mazes with increasing dead ends, spawn point distribution, authoritative exit-tile progression (`1 -> 2 -> 3 -> 1`) |
| **Developer 3** | Combat Mechanics & Scoring | Server-authoritative shooting, swept collision detection, +10/-5 scoring, 2-second hit protection, first-person visual tracers and feedback, scoreboard |
| **Developer 4** | Networking, Reliability & Audit | Server `0.0.0.0` binding, 10-second inactivity timeouts, 12-client stress test suite, live process testing, final setup and audit documentation |
