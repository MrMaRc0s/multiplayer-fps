# Maze Wars — Next Developer Handoff

## Control instructions

- Project root: .
- Remote: `https://platform.zone01.gr/git/cgkaldan/multiplayer-fps.git`
- Starting commit: `99afaba` (`feat: render other players in first-person view`)
- Starting branch: `main`, synchronized with `origin/main`
- Rust edition: 2024
- GUI library: SDL2 through the `sdl2` crate
- Work in small, coherent, buildable commits.
- Before every commit, tell the user:
  1. exactly what changed;
  2. how to run it;
  3. what visible behavior to expect;
  4. which automated checks passed;
  5. the proposed commit message.
- Wait for explicit user approval before each commit.
- Do not push unless the user asks.
- Do not commit `NEXT_DEV_PLAN.md` or `AUDIT_NOTES.md`.
- Keep tracked documentation limited to `README.md`.
- Do not implement bonuses, AI players, a level editor, procedural generation, host history, observer mode, or chat unless the user explicitly expands scope.
- Combat is now required: the original networked Maze War is a competitive shooting game, not only a maze-navigation game.
- Dev 2 has already started the three-level work. Dev B must not rewrite, replace, or independently implement levels; rebase onto Dev 2's completed work before integrating combat where shared files overlap.
- Optimize for the supplied audit: warning-free builds, visible UI requirements, three levels, UDP multiplayer, 10+ clients, and over 50 FPS.
- Preserve the existing manual binary packet format. Do not replace it with a networking framework or serialization dependency without a concrete need.

## Current verified state

The following already works and is pushed:

- Separate `client` and `server` binaries.
- Server binds UDP `0.0.0.0:34254` and accepts up to 64 players.
- Client asks for server IP/port and username.
- UDP join acceptance/rejection and retry behavior.
- Friendly unreachable-server error.
- Authoritative server movement at 60 ticks per second.
- Input sequence validation and source-address/player-ID authentication.
- Wall collision with sliding along walls.
- Compact snapshots for up to 64 players below the 1,400-byte packet limit.
- Ten-second inactive-player timeout.
- SDL2 resizable game window.
- First-person monochrome ray-cast maze view.
- Minimap showing the full maze, local player in green, and other players in red.
- Other players rendered as floating eye sprites in first-person view with wall occlusion.
- W/S or Up/Down movement; A/D or Left/Right turning; Escape exits.
- 30 unit tests pass.
- `cargo clippy --all-targets -- -D warnings` passes.

## Historical gameplay baseline

There is no single definitive "original" build: Maze/Maze War changed between the 1973 NASA version, the 1974 MIT networked version, and the later Xerox Alto/Cedar versions. For this project, preserve the interface and movement already implemented and reproduce the common networked shooting rules that are consistently documented.

Required original-game behavior that was previously missing:

- Players shoot other players in the maze. This is a core rule, not a bonus.
- A shot travels rapidly and straight in the direction the player is facing; walls stop it.
- A successful hit gives the shooter **10 points** and removes **5 points** from the target.
- After being hit, the target remains in the maze and cannot be hit again for **2 seconds**, giving them time to escape. Do not add health, lives, ammo, reloading, weapons, or a modern death/kill system.
- Scores are visible in the game interface alongside player identity.
- Other players remain represented as directional floating eyeballs; this is already implemented.
- The server must decide whether a shot hits, update scores, and enforce hit protection. Clients must never award their own points.

Historically documented behavior that we will **not retrofit** because it conflicts with the current approved implementation or is outside the assignment:

- Original movement used one-cell steps and 90-degree turns; keep the existing smooth movement and turning.
- Some versions allowed turning around and peeking left/right without moving; these are optional and out of scope.
- The historical map generally showed only the local player's position. Keep the existing minimap behavior because it is already implemented and is useful for the supplied multiplayer audit.
- Text chat, observer mode, AI robots, custom maze files, and the level editor existed in some versions, but remain excluded bonuses.
- The assignment's three progressively harder levels are a project requirement rather than a universal original Maze War rule. Dev 2 owns this work.

Research references:

- Computer History Museum Xerox archive, 1985 Cedar `MazeWar` documentation: describes the game as multiplayer "seek-and-destroy" and lists forward, backward, turn, turn-around, shoot, and peek controls: <https://xeroxparcarchive.computerhistory.org/cyan/cedarchest6.1/documentation/.MazeWarDoc.Tioga%211.html>
- Guinness World Records history and mechanics summary: documents shooting, straight-moving bullets, `+10/-5` scoring, grid movement, 90-degree turns, and peeking: <https://www.guinnessworldrecords.com/world-records/95409-first-first-person-shooter-fps-videogame>
- Historical overview with version distinctions: documents the two-second protection after a hit, visible scores, map behavior, player counts, and eyeball avatars: <https://en.wikipedia.org/wiki/Maze_%281973_video_game%29>

Run the current version with:

```bash
cargo run --bin server
```

In separate terminals:

```bash
cargo run
```

## Source map

- `src/bin/client.rs`: terminal prompts and client startup.
- `src/bin/server.rs`: thin server entry point.
- `src/client.rs`: UDP connection, input sending, and snapshot reception.
- `src/protocol.rs`: message definitions plus manual byte encoding/decoding.
- `src/server.rs`: non-blocking UDP loop, 60 Hz ticking, and broadcasts.
- `src/server/state.rs`: authoritative players, input, movement, collision, snapshots, and timeouts.
- `src/world.rs`: maze tiles, spawn position, and collision queries.
- `src/window.rs`: SDL event/render loop and keyboard input.
- `src/window/view.rs`: ray casting, wall depth, and eye-sprite projection.
- `src/window/minimap.rs`: top-down maze and player markers.
- `AUDIT_NOTES.md`: ignored local learning/audit notes; update it but never commit it.

## Remaining definition of done

- [ ] FPS is visibly displayed inside the game window.
- [ ] FPS remains above 50 during normal movement and multiplayer testing.
- [ ] Three mazes exist and clearly increase in difficulty/dead ends.
- [ ] Players can visibly progress through all three levels.
- [ ] Players receive separate valid spawn positions where possible.
- [ ] Players can fire straight-moving shots that stop at walls and can hit opponents.
- [ ] Shot validation, collision, scoring, and two-second hit protection are server-authoritative.
- [ ] A hit awards the shooter 10 points and subtracts 5 points from the target.
- [ ] Usernames and scores are visible in the game interface.
- [ ] Shooting remains responsive and synchronized with multiple clients without reducing the frame rate below 50 FPS.
- [ ] Ten simultaneous clients can join and remain active.
- [ ] Multiplayer is tested for at least three minutes.
- [ ] A client can connect using a non-loopback/LAN server address, or arbitrary binding is demonstrated when cross-machine traffic is unavailable.
- [ ] Debug and release builds compile without warnings.
- [ ] README contains final setup, controls, architecture, and audit instructions.

## Commit-sized implementation plan

### 1. Display frame rate

Goal: satisfy the missing FPS UI requirement without adding a font dependency.

Implementation guidance:

- Add a small module such as `src/window/hud.rs`.
- Track rendered frames over a rolling interval of roughly 0.5–1.0 seconds.
- Draw `FPS: <number>` inside the upper-right of the SDL canvas using a tiny bitmap font for only the required letters, colon, and digits.
- Do not display FPS only in the terminal or window title; the audit asks for it in the interface.
- Keep measurement independent of server tick rate.
- Avoid per-frame heap allocation in the counter/drawing path where practical.
- Add deterministic tests for FPS calculation by supplying elapsed durations rather than sleeping in tests.

Acceptance checks:

```bash
cargo test window
cargo clippy --all-targets -- -D warnings
cargo run --bin server
cargo run
```

Expected visible result: an FPS counter appears in the upper-right and remains above 50 while moving.

Proposed commit:

```text
feat: display frame rate
```

### 2. Add levels two and three plus spawn sets — Dev 2 (already in progress)

Goal: define all required content in the pure world module before networking depends on it.

Implementation guidance:

- Extend `src/world.rs` with three static, readable ASCII mazes.
- Add `Maze::for_level(level)` or an equally simple API.
- Level 1 stays the easiest.
- Level 2 should add more branches and dead ends.
- Level 3 should be visibly denser and have the most dead ends.
- Add several valid spawn positions per maze so simultaneous players do not overlap immediately.
- Add an exit tile, for example `E`, and treat it as walkable floor for collision.
- Validate every map in tests: equal row widths, closed outer border, walkable spawns, walkable/reachable exit, and increasing dead-end count.
- Keep maps static and hand-authored; procedural generation is a bonus and out of scope.

Acceptance checks:

```bash
cargo test world
cargo clippy --all-targets -- -D warnings
```

This commit may have no runtime level switching yet. Show the three ASCII maps to the user before requesting the commit.

Proposed commit:

```text
feat: add three maze levels and spawn points
```

### 3. Add authoritative level progression — Dev 2

Goal: let the server move the shared game session through all three mazes.

Implementation guidance:

- Store the current level/maze in `ServerState`; remove hard-coded level `1` values.
- Assign joining players to separate valid spawn positions.
- When a player reaches the exit tile, advance the shared server level.
- Respawn all connected players in valid positions and reset their movement input.
- A simple cycle `1 -> 2 -> 3 -> 1` is acceptable and easy to demonstrate.
- Ensure `Accepted` and `Snapshot` report the real level.
- Add state tests for distinct spawns, progression, respawning, and cycling after level 3.

Acceptance checks:

```bash
cargo test server::state
cargo clippy --all-targets -- -D warnings
```

Proposed commit:

```text
feat: add authoritative level progression
```

### 4. Make level changes visible in the client — Dev 2

Goal: close the vertical slice so progression is visually testable.

Implementation guidance:

- Select the client `Maze` from the newest snapshot level instead of always using level one.
- Make both ray casting and minimap switch on the same snapshot.
- Display the current level in the HUD.
- Ensure a level transition does not keep stale player positions or wall-depth data.
- Run two clients and confirm both switch levels together.

Acceptance checks:

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo run --bin server
cargo run
```

Expected visible result: reaching `E` switches both clients to the next, harder maze.

Proposed commit:

```text
feat: synchronize level progression
```

## Dev B track: original Maze War combat

Dev B owns the work below. Do not begin by changing level definitions. First inspect Dev 2's branch/status, wait until the relevant level changes are committed, then rebase or merge so combat is built on the current protocol and server state. Keep every step buildable and ask the user before every commit as required above.

### B1. Extend the protocol for firing and scores

Goal: define bounded combat messages without trusting clients or breaking the UDP packet-size limit.

Implementation guidance:

- Add a client-to-server `Fire` message containing the authenticated player ID and a monotonically increasing sequence number. Do not accept a client-supplied hit target or score.
- Extend player snapshot data with a signed score. New players start at zero; negative scores are valid.
- Add compact server-to-client combat events for shot fired and player hit. Give events sequence IDs so clients can ignore duplicates or stale UDP deliveries.
- Prefer combat events over placing every projectile in the main player snapshot, so a 64-player snapshot remains below 1,400 bytes.
- Keep manual encoding/decoding and explicit packet tags consistent with `src/protocol.rs`.
- Reject truncated, oversized, malformed, or trailing-byte packets in tests.

Acceptance checks:

```bash
cargo test protocol
cargo clippy --all-targets -- -D warnings
```

Proposed commit:

```text
feat: add combat protocol messages
```

### B2. Add server-authoritative shots, hits, and scoring

Goal: implement the original competitive rule set entirely on the server.

Implementation guidance:

- Put combat simulation in a focused module such as `src/server/combat.rs`; keep `src/server/state.rs` responsible for coordinating player/session state.
- Spawn a shot from the player's authoritative position and facing direction when a valid fire input arrives.
- Simulate rapid straight-line travel at the existing 60 Hz server tick. Use swept segment collision between the previous and new shot positions so fast shots cannot pass through walls or players between ticks.
- Stop a shot on the first wall or eligible player it reaches. Never allow self-hits or hits through walls.
- On hit, add 10 to the shooter's score and subtract 5 from the target's score.
- Give the target exactly two seconds of server-enforced hit protection. The player stays where they are and can move during protection; there is no health, death, respawn, ammo, or reload mechanic.
- Limit fire rate and active shot lifetime. Use fixed upper bounds so malicious or fast clients cannot grow memory or bandwidth without limit.
- Preserve scores across Dev 2's level transitions for the duration of the connection.
- Broadcast authoritative fire/hit events and include updated scores in normal snapshots.

Required tests:

- a clear straight shot hits the nearest opponent;
- a wall blocks a shot;
- a shot cannot hit its shooter;
- only the first target in a corridor is hit;
- score changes are exactly `+10/-5` and may go negative;
- repeated fire during the target's two-second protection does not score;
- the target becomes hittable after protection expires;
- spoofed player IDs, stale fire sequences, and excessive fire rates are ignored;
- shot state stays bounded with many connected players;
- scores survive a level transition.

Acceptance checks:

```bash
cargo test server
cargo clippy --all-targets -- -D warnings
```

Proposed commit:

```text
feat: add authoritative shooting and scoring
```

### B3. Add shooting controls and visible feedback

Goal: make combat understandable and testable in the graphical client.

Implementation guidance:

- Map Space and left mouse button to fire; send one fire request per press rather than every rendered frame.
- Render a short monochrome shot/tracer animation from authoritative combat events. Preserve the original black-and-white visual style.
- Give the shooter clear hit confirmation and give the target brief visual feedback when hit, without adding gore or modern weapon graphics.
- Make the two-second protected state visually understandable but subtle; for example, blink or invert the affected eye/player indicator.
- Keep rendering event-driven and allocation-light. Missing a visual UDP event must not affect the authoritative score or future snapshots.
- Add deterministic tests for any projection or animation-lifetime calculations.

Acceptance checks:

```bash
cargo test window
cargo clippy --all-targets -- -D warnings
cargo run --bin server
cargo run
```

Expected visible result: one client can fire down a corridor, walls block the shot, another visible eye can be hit, and both clients receive understandable feedback.

Proposed commit:

```text
feat: render multiplayer combat feedback
```

### B4. Display the multiplayer scoreboard

Goal: reproduce the original visible scoring rule without obscuring the maze view.

Implementation guidance:

- Show each connected username and signed score in a compact HUD panel.
- Clearly identify the local player and keep ordering deterministic, preferably highest score first with player ID as the tie-breaker.
- Reuse the small bitmap-font/HUD work from the FPS task rather than adding a second text-rendering system.
- Ensure 10 players remain readable; if more players connect, clip or page the display without allocating heavily every frame.
- Verify score changes appear from authoritative snapshots even if a combat event packet was lost.

Acceptance checks:

```bash
cargo test window
cargo test protocol
cargo clippy --all-targets -- -D warnings
```

Expected visible result: after a hit, the shooter gains 10 and the target loses 5 on every connected client's scoreboard.

Proposed commit:

```text
feat: display multiplayer scores
```

### 5. Multiplayer and performance audit

Goal: verify the supplied audit rather than adding features.

Procedure:

- Run `cargo fmt --check`.
- Run `cargo test` and confirm every test passes.
- Run `cargo clippy --all-targets -- -D warnings`.
- Run `cargo build` and `cargo build --release`; inspect for warnings.
- Start one server and at least 10 clients with unique usernames.
- Keep them connected for at least three minutes.
- Move and fire from multiple clients; verify minimap markers, first-person eyes, wall-blocked shots, score changes, hit protection, collision, and level changes.
- Confirm displayed FPS remains above 50 and the game feels smooth.
- Test a client with the host machine's LAN IP. If cross-machine traffic is unavailable, record that the server binds `0.0.0.0` and demonstrate multiple localhost clients.
- Watch the server for crashes, invalid-packet errors, or unintended timeouts.
- Fix each discovered defect as its own small change and request approval before each fix commit.
- Do not commit generated logs or temporary load-test artifacts.

There may be no commit for this step if no defect is found.

### 6. Finish the only tracked documentation

Goal: replace the placeholder README with concise final project instructions.

README must include:

- SDL2 prerequisite installation examples for the team's supported OS.
- Server and client run commands.
- Default UDP port and LAN connection explanation.
- Controls.
- Shooting, scoring, and the two-second hit-protection rule.
- Three-level behavior.
- Short architecture explanation and source map.
- Test/lint/build commands.
- Four-developer ownership split.
- No claims about tests that were not actually performed.

Keep detailed study notes in ignored `AUDIT_NOTES.md`, not in additional tracked documents.

Acceptance checks:

- Follow the README from a fresh terminal.
- Confirm every command and expected behavior is accurate.
- Run the complete automated check set once more.

Proposed commit:

```text
docs: add setup and audit instructions
```

## Final handoff behavior

At completion, report:

- final commit IDs;
- which audit items passed;
- any audit item not tested and why;
- measured FPS and test conditions;
- number of simultaneous clients and test duration;
- exact commands to reproduce the final checks.

Ask the user before the final push. Do not add bonuses while unresolved audit requirements remain.