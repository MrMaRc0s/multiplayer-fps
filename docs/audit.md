# Local audit and learning notes

This file is deliberately excluded from Git. It is a working study guide, not project documentation.

## Current slice: connection setup

- `src/protocol.rs` manually serializes small messages so every UDP datagram is self-contained.
- The first four bytes (`MZWR`) reject unrelated traffic; the next byte versions the protocol.
- The server binds to `0.0.0.0`, so it does not limit clients to localhost.
- UDP has no connection handshake, so the application defines one with `Join`, `Accepted`, and `Rejected` messages.
- The client retries its join packet because UDP delivery is not guaranteed.
- The server recognizes a repeated join from the same socket and returns the same player ID. This makes a lost `Accepted` response safe to retry.
- The server capacity is 64, above the audit minimum of 10.

## Why the source is split this way

- `src/bin/client.rs` only handles terminal prompts and starts the client connection.
- `src/bin/server.rs` only starts the server.
- `src/client.rs` owns the client UDP socket, retries, and accepted session.
- `src/server.rs` owns connected player state and the server UDP loop.
- `src/protocol.rs` is the shared wire format used by both sides.
- `src/window.rs` owns SDL setup and the window event loop.
- Tests stay beside the code they test, which is a normal Rust convention.

## Maze and collision model

- `src/world.rs` stores the maze as readable text rows: `#` is a wall and `.` is floor.
- World positions use floating-point tile coordinates; `(1.5, 1.5)` is the center of tile `(1, 1)`.
- Collision samples the four corners of the player's radius. This keeps the player body out of walls instead of treating it as a single point.
- Positions outside the map are treated as walls, which keeps players inside even if movement input is malformed.

## Movement packets

- An input packet contains the player ID, an increasing sequence number, and `-1`, `0`, or `1` for forward and turn axes.
- Sequence numbers let the server discard old UDP input that arrives out of order.
- A snapshot contains one server tick plus each player's ID, position, and direction.
- Usernames are not repeated in every snapshot. Keeping each player to 16 bytes allows all 64 players to fit within the 1,400-byte application packet limit.

## Authoritative movement

- The server updates the world at 60 ticks per second; clients send intentions, not trusted positions.
- Input is accepted only when the UDP source address and player ID match the joined player.
- Old or repeated sequence numbers are ignored so delayed UDP packets cannot undo newer input.
- The server applies turning, attempts X and Y movement separately for wall sliding, and checks both against the maze collision model.
- Each server tick broadcasts the resulting authoritative snapshot to every joined address.
- Each join or accepted input refreshes the player's activity tick. After 600 silent ticks (10 seconds at 60 Hz), the server removes that player and frees the username/capacity slot.

## Visible synchronized movement

- The SDL loop reads W/S (or Up/Down) as forward input and A/D (or Left/Right) as turning input.
- Input is sent at 60 messages per second with an increasing sequence number.
- The client socket is non-blocking after joining, so rendering never waits for a UDP packet.
- The client drains queued snapshots every frame and ignores snapshots older than the newest tick it has seen.
- The minimap draws the shared maze, the local player in green, and other players in red. The marker uses the server snapshot, so visible movement proves the complete UDP round trip.

## First-person wall view

- The renderer casts one ray for every two horizontal pixels across a 60-degree field of view.
- Each ray advances through grid cells with DDA (digital differential analysis) until it reaches a wall.
- Wall slice height is inversely proportional to corrected distance; the correction prevents the fish-eye distortion caused by measuring along angled rays.
- Distance shading and slightly darker horizontal wall faces provide depth while keeping the monochrome appearance close to the original Maze Wars style.
- Other players are projected as floating eye sprites, matching the original game's visual idea.
- A per-column wall-depth buffer hides the parts of an eye that are behind walls. Players behind the camera or outside the screen are skipped.
- Players are drawn farthest-first, so a nearby eye correctly covers a farther eye.

## Things every team member should be able to explain

- Why UDP is lower overhead than TCP and what reliability we must add ourselves.
- Why one datagram must contain one complete message.
- Why decoding validates the magic bytes, version, type, and lengths before reading data.
- Why the client uses an ephemeral local port (`0.0.0.0:0`).
- Why the server must bind to `0.0.0.0` for clients on other machines.

## Audit checks accumulated so far

1. Run `cargo test`.
2. Run `cargo clippy --all-targets -- -D warnings`.
3. Start `cargo run --bin server`.
4. Start `cargo run`, accept the default address, and choose a name.
5. Check that the server logs the join and the client opens its SDL window. Press Escape to open the Pause Menu and select Quit Game (or press Q) to close it.
6. Stop the server and try the client again. It should explain how to start the server instead of printing a raw `ConnectionRefused` error.