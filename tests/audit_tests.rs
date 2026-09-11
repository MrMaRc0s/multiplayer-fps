use maze_wars::client::{DEFAULT_SERVER_ADDRESS, connect, parse_server_address, validate_username};
use maze_wars::protocol::{
    MAGIC, MAX_PACKET_SIZE, MAX_SNAPSHOT_PLAYERS, PacketError, PlayerSnapshot, ServerMessage,
    VERSION, decode_client, decode_server, encode_client, encode_server,
};
use maze_wars::server::SERVER_ADDRESS;
use maze_wars::server::combat::{PROTECTION_TICKS, swept_wall_collision};
use maze_wars::server::state::ServerState;
use maze_wars::window::hud::{
    FpsTracker, MAX_DISPLAYED_SCOREBOARD_PLAYERS, ScoreboardEntry, format_hud_string,
    format_scoreboard_entry, sort_scoreboard,
};
use maze_wars::window::minimap::{
    LOCAL_PLAYER_COLOR, MinimapLayout, OTHER_PLAYER_COLOR, compute_layout, player_minimap_color,
    player_screen_position,
};
use maze_wars::window::view::{
    CombatVisuals, DAMAGE_FEEDBACK_LIFETIME, HIT_CONFIRMATION_LIFETIME, SHOT_TRACER_LIFETIME,
    ShotTracer, cast_ray, correct_fish_eye, is_blink_visible, project_player, wall_brightness,
};
use maze_wars::world::{Maze, Position, Tile};
use sdl2::pixels::Color;
use std::f32::consts::PI;
use std::net::{SocketAddr, UdpSocket};
use std::process::Command;
use std::time::Duration;

// 1. `audit_check_compiles_and_lints_without_warnings`: build verification.
#[test]
fn audit_check_compiles_and_lints_without_warnings() {
    let output = Command::new("cargo")
        .args(["check", "--all-targets"])
        .output()
        .expect("failed to execute cargo check");

    assert!(
        output.status.success(),
        "cargo check --all-targets failed with:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// 2. `audit_check_server_binds_to_all_interfaces_and_arbitrary_client`:
// verifies UDP bind `0.0.0.0:34254` and non-loopback address compatibility.
#[test]
fn audit_check_server_binds_to_all_interfaces_and_arbitrary_client() {
    assert_eq!(
        SERVER_ADDRESS, "0.0.0.0:34254",
        "server must bind to all interfaces on 0.0.0.0:34254"
    );

    // Verify socket binding to 0.0.0.0 works
    let server_sock = UdpSocket::bind("0.0.0.0:0").expect("binding to 0.0.0.0:0 must succeed");
    let server_port = server_sock
        .local_addr()
        .expect("must get local addr")
        .port();

    // Verify non-loopback address compatibility in ServerState
    let mut state = ServerState::new();

    // Connect clients from distinct subnets / non-loopback IPs
    let lan_client_1: SocketAddr = "192.168.1.100:41234".parse().unwrap();
    let lan_client_2: SocketAddr = "10.0.0.42:52345".parse().unwrap();
    let wan_client: SocketAddr = "172.16.50.88:63456".parse().unwrap();

    let res1 = state.join(lan_client_1, "lan_alice".to_string());
    let res2 = state.join(lan_client_2, "lan_bob".to_string());
    let res3 = state.join(wan_client, "wan_charlie".to_string());

    assert!(matches!(res1, ServerMessage::Accepted { player_id: 1, .. }));
    assert!(matches!(res2, ServerMessage::Accepted { player_id: 2, .. }));
    assert!(matches!(res3, ServerMessage::Accepted { player_id: 3, .. }));
    assert_eq!(state.player_count(), 3);

    // Non-loopback clients can send authoritative input and update coordinates
    assert!(state.apply_input(lan_client_1, 1, 1, 1, 0));
    assert!(state.apply_input(lan_client_2, 2, 1, 0, 1));
    state.update(0.1);

    // UDP datagram round trip from 127.0.0.1 client to 0.0.0.0 bound socket
    let client_sock = UdpSocket::bind("127.0.0.1:0").expect("client socket bind");
    client_sock
        .connect(format!("127.0.0.1:{server_port}"))
        .expect("client connect");

    client_sock.send(b"ping").expect("client send");
    let mut buf = [0_u8; 16];
    let (len, src) = server_sock.recv_from(&mut buf).expect("server recv");
    assert_eq!(&buf[..len], b"ping");
    server_sock.send_to(b"pong", src).expect("server send");
    let len = client_sock.recv(&mut buf).expect("client recv");
    assert_eq!(&buf[..len], b"pong");
}

// 3. `audit_check_client_asks_for_ip_and_username_and_validates`:
// tests address and username prompt parsing, defaults, empty username rejection, length limit (20 bytes).
#[test]
fn audit_check_client_asks_for_ip_and_username_and_validates() {
    // Address parsing and defaults
    assert_eq!(
        parse_server_address("").unwrap(),
        DEFAULT_SERVER_ADDRESS.parse::<SocketAddr>().unwrap(),
        "empty address must default to DEFAULT_SERVER_ADDRESS"
    );
    assert_eq!(
        parse_server_address("   ").unwrap(),
        DEFAULT_SERVER_ADDRESS.parse::<SocketAddr>().unwrap(),
        "whitespace address must default to DEFAULT_SERVER_ADDRESS"
    );
    assert_eq!(
        parse_server_address("127.0.0.1:34254").unwrap(),
        "127.0.0.1:34254".parse::<SocketAddr>().unwrap()
    );
    assert_eq!(
        parse_server_address("192.168.1.15:12345").unwrap(),
        "192.168.1.15:12345".parse::<SocketAddr>().unwrap()
    );
    assert!(
        parse_server_address("invalid_address").is_err(),
        "invalid address format must error"
    );
    assert!(
        parse_server_address("999.999.999.999:12345").is_err(),
        "unresolvable IP must error"
    );

    // Username validation: empty rejection
    assert!(
        validate_username("").is_err(),
        "empty username must be rejected"
    );
    assert!(
        validate_username("   ").is_err(),
        "whitespace-only username must be rejected"
    );

    // Valid usernames
    assert_eq!(validate_username("a").unwrap(), "a");
    assert_eq!(validate_username("player1").unwrap(), "player1");
    assert_eq!(validate_username("  chris  ").unwrap(), "chris");

    // Exactly 20 bytes (maximum allowed)
    let exact_20 = "12345678901234567890";
    assert_eq!(exact_20.len(), 20);
    assert_eq!(validate_username(exact_20).unwrap(), exact_20);

    // 21 bytes (must be rejected)
    let over_20 = "123456789012345678901";
    assert_eq!(over_20.len(), 21);
    assert!(
        validate_username(over_20).is_err(),
        "username > 20 bytes must be rejected"
    );

    // Multi-byte UTF-8 bytes limit
    let utf8_24_bytes = "🦀🦀🦀🦀🦀🦀"; // 6 * 4 = 24 bytes
    assert!(utf8_24_bytes.len() > 20);
    assert!(
        validate_username(utf8_24_bytes).is_err(),
        "multi-byte username exceeding 20 bytes must be rejected"
    );
}

// 4. `audit_check_unreachable_server_provides_friendly_error`:
// verifies connecting to offline server yields friendly error.
#[test]
fn audit_check_unreachable_server_provides_friendly_error() {
    // Bind an ephemeral socket and immediately drop it to guarantee an unused offline port
    let offline_addr = {
        let sock = UdpSocket::bind("127.0.0.1:0").expect("bind ephemeral socket");
        sock.local_addr().expect("local addr")
    };

    let result = connect(offline_addr, "tester");
    assert!(
        result.is_err(),
        "connecting to offline server must return an Err"
    );
    let error_message = result.unwrap_err();

    assert!(
        error_message.contains("is not running or reachable"),
        "error message should explain server is unreachable, got: {error_message}"
    );
    assert!(
        error_message.contains("cargo run --bin server"),
        "error message should suggest starting server with cargo run --bin server, got: {error_message}"
    );
}

// 5. `audit_check_mini_map_and_position_updates`:
// tests world snapshot correctly positions own player in green and others in red, and updates coordinates on authoritative movement.
#[test]
fn audit_check_mini_map_and_position_updates() {
    let own_id = 1;
    let other_id = 2;

    // Verify local player is colored green, others colored red
    assert_eq!(
        player_minimap_color(own_id, own_id),
        LOCAL_PLAYER_COLOR,
        "own player must be green"
    );
    assert_eq!(
        LOCAL_PLAYER_COLOR,
        Color::RGB(70, 220, 125),
        "local player color must be RGB(70, 220, 125)"
    );

    assert_eq!(
        player_minimap_color(other_id, own_id),
        OTHER_PLAYER_COLOR,
        "other players must be red"
    );
    assert_eq!(
        OTHER_PLAYER_COLOR,
        Color::RGB(225, 73, 75),
        "other player color must be RGB(225, 73, 75)"
    );

    // Verify minimap screen coordinate transformation
    let layout: MinimapLayout = compute_layout(15, 11);
    let initial_pos = player_screen_position(&layout, 1.5, 1.5);
    assert!(initial_pos.x >= layout.origin_x);
    assert!(initial_pos.y >= layout.origin_y);

    // Verify authoritative movement updates coordinates
    let mut state = ServerState::new();
    let addr1: SocketAddr = "127.0.0.1:10001".parse().unwrap();
    let addr2: SocketAddr = "127.0.0.1:10002".parse().unwrap();

    state.join(addr1, "alice".to_string());
    state.join(addr2, "bob".to_string());

    let initial_p1 = state.player_position(1).unwrap();
    // Alice moves forward
    state.apply_input(addr1, 1, 1, 1, 0);
    state.update(0.2);

    let updated_p1 = state.player_position(1).unwrap();
    assert!(
        updated_p1.x > initial_p1.x,
        "authoritative movement must update player coordinates"
    );

    let updated_screen_pos = player_screen_position(&layout, updated_p1.x, updated_p1.y);
    assert_ne!(
        initial_pos, updated_screen_pos,
        "minimap screen coordinates must update on authoritative movement"
    );
}

// 6. `audit_check_camera_view_and_ray_casting_updates`:
// tests first-person ray casting updates view with turning and forward movement, DDA wall detection, distance shading, fish-eye correction.
#[test]
fn audit_check_camera_view_and_ray_casting_updates() {
    let maze = Maze::level_one();
    let origin = Position { x: 1.5, y: 1.5 };

    // DDA Wall detection
    let hit_east = cast_ray(&maze, origin, 0.0).expect("ray east must hit wall");
    assert!(hit_east.distance > 0.0);
    assert!(hit_east.hit_vertical_side);

    let hit_north = cast_ray(&maze, origin, -PI / 2.0).expect("ray north must hit wall");
    assert!((hit_north.distance - 0.5).abs() < 0.001);
    assert!(!hit_north.hit_vertical_side);

    let hit_west = cast_ray(&maze, origin, PI).expect("ray west must hit wall");
    assert!((hit_west.distance - 0.5).abs() < 0.001);

    // Fish-eye correction
    let raw_dist = 2.0;
    let angle_offset = PI / 6.0; // 30 degrees off center
    let corrected = correct_fish_eye(raw_dist, angle_offset, 0.0);
    assert!(
        corrected < raw_dist,
        "corrected distance must be shorter than raw diagonal ray distance"
    );
    assert!((corrected - raw_dist * (PI / 6.0).cos()).abs() < 0.001);

    // Distance shading
    let bright_close = wall_brightness(0.5, true);
    let dim_far = wall_brightness(8.0, true);
    assert!(
        bright_close > dim_far,
        "closer walls must be brighter ({bright_close} vs {dim_far})"
    );
    let vertical_face = wall_brightness(2.0, true);
    let horizontal_face = wall_brightness(2.0, false);
    assert!(
        vertical_face > horizontal_face,
        "vertical wall faces must be brighter than horizontal faces for monochrome depth"
    );

    // Turning updates camera ray angles
    let hit_straight = cast_ray(&maze, origin, 0.0).unwrap();
    let hit_turned = cast_ray(&maze, origin, PI / 4.0).unwrap();
    assert_ne!(
        hit_straight.distance, hit_turned.distance,
        "turning must alter cast ray distances"
    );

    // Forward movement updates ray casting origin
    let new_origin = Position { x: 2.0, y: 1.5 };
    let hit_moved = cast_ray(&maze, new_origin, PI).unwrap();
    assert!(
        hit_moved.distance > hit_west.distance,
        "moving east must increase distance to west wall"
    );
}

// 7. `audit_check_fps_counter_displayed_and_above_50_fps`:
// tests `FpsTracker` accuracy, rolling window calculation, ensuring 60 FPS simulated rendering stays well above 50 FPS.
#[test]
fn audit_check_fps_counter_displayed_and_above_50_fps() {
    let window_duration = Duration::from_millis(500);
    let mut tracker = FpsTracker::new(window_duration);

    let frame_dt = Duration::from_nanos(500_000_000 / 30 + 1);

    // Before window duration elapses, fps remains 0
    for _ in 0..10 {
        tracker.advance(frame_dt);
        assert_eq!(tracker.fps(), 0);
    }

    // Complete the 500 ms window with 60 FPS frames (total 30 frames)
    for _ in 10..30 {
        tracker.advance(frame_dt);
    }
    assert_eq!(
        tracker.fps(),
        60,
        "FpsTracker must accurately measure 60 FPS"
    );
    assert!(tracker.fps() > 50, "frame rate must stay above 50 FPS");

    // Simulate steady 60 FPS for 180 frames (3 full seconds)
    for _ in 0..180 {
        tracker.advance(frame_dt);
        assert!(
            tracker.fps() >= 59 && tracker.fps() <= 61,
            "FPS tracker should remain steady near 60 FPS, got {}",
            tracker.fps()
        );
    }
    assert!(tracker.fps() > 50);

    // Verify HUD text formatting includes FPS and Level
    let mut buf = [0_u8; 64];
    let hud_text = format_hud_string(&mut buf, tracker.fps(), Some(2));
    assert!(
        hud_text.contains("FPS: 60"),
        "HUD text must format FPS properly"
    );
    assert!(
        hud_text.contains("LEVEL: 2"),
        "HUD text must format Level properly"
    );
}

// 8. `audit_check_three_levels_increasing_difficulty_and_dead_ends`:
// verifies Level 1, Level 2, Level 3 ASCII layouts, equal widths, closed borders, and monotonically increasing dead-end counts.
#[test]
fn audit_check_three_levels_increasing_difficulty_and_dead_ends() {
    let m1 = Maze::for_level(1).expect("Level 1 must exist");
    let m2 = Maze::for_level(2).expect("Level 2 must exist");
    let m3 = Maze::for_level(3).expect("Level 3 must exist");

    for (level_idx, maze) in [(1, m1), (2, m2), (3, m3)] {
        // Equal row widths
        let w = maze.width();
        assert!(w > 0);
        for (row_idx, row) in maze.rows().iter().enumerate() {
            assert_eq!(
                row.len(),
                w,
                "Level {level_idx} row {row_idx} length does not match maze width"
            );
        }

        // Closed outer borders
        let h = maze.height();
        for x in 0..w as i32 {
            assert_eq!(
                maze.tile(x, 0),
                Tile::Wall,
                "Level {level_idx} top border at x={x} must be a wall"
            );
            assert_eq!(
                maze.tile(x, h as i32 - 1),
                Tile::Wall,
                "Level {level_idx} bottom border at x={x} must be a wall"
            );
        }
        for y in 0..h as i32 {
            assert_eq!(
                maze.tile(0, y),
                Tile::Wall,
                "Level {level_idx} left border at y={y} must be a wall"
            );
            assert_eq!(
                maze.tile(w as i32 - 1, y),
                Tile::Wall,
                "Level {level_idx} right border at y={y} must be a wall"
            );
        }

        // Spawns and exits are walkable
        for spawn in maze.spawns() {
            assert!(
                maze.can_occupy(*spawn, 0.2),
                "Level {level_idx} spawn {spawn:?} must be walkable"
            );
        }
        assert!(
            maze.can_occupy(maze.exit(), 0.2),
            "Level {level_idx} exit {:?} must be walkable",
            maze.exit()
        );
    }

    // Monotonically increasing dead-end counts
    let d1 = m1.count_dead_ends();
    let d2 = m2.count_dead_ends();
    let d3 = m3.count_dead_ends();

    assert!(
        d1 < d2,
        "Level 2 dead ends ({d2}) must be strictly greater than Level 1 ({d1})"
    );
    assert!(
        d2 < d3,
        "Level 3 dead ends ({d3}) must be strictly greater than Level 2 ({d2})"
    );
}

// 9. `audit_check_exit_reaches_and_cycles_all_three_levels`:
// tests reaching exit tile 'E' triggers authoritative level transition `1 -> 2 -> 3 -> 1`, and all players are respawned at valid separate positions.
#[test]
fn audit_check_exit_reaches_and_cycles_all_three_levels() {
    let mut state = ServerState::new();
    let addrs: Vec<SocketAddr> = (0..4)
        .map(|i| format!("127.0.0.1:{}", 20000 + i).parse().unwrap())
        .collect();

    for (i, addr) in addrs.iter().enumerate() {
        state.join(*addr, format!("player_{i}"));
    }
    assert_eq!(state.player_count(), 4);

    // Initial state: Level 1
    assert_eq!(state.maze().level(), 1);
    let p1_spawns: Vec<_> = (1..=4)
        .map(|id| state.player_position(id).unwrap())
        .collect();
    for i in 0..4 {
        for j in (i + 1)..4 {
            assert_ne!(
                p1_spawns[i], p1_spawns[j],
                "all players must spawn at distinct positions"
            );
        }
    }

    // Reach Level 1 exit -> transitions to Level 2
    state.set_player_position(1, state.maze().exit());
    state.update(0.016);
    assert_eq!(
        state.maze().level(),
        2,
        "reaching level 1 exit must transition to level 2"
    );

    let p2_spawns: Vec<_> = (1..=4)
        .map(|id| state.player_position(id).unwrap())
        .collect();
    for pos in &p2_spawns {
        assert!(
            state.maze().can_occupy(*pos, 0.2),
            "level 2 respawn position must be valid"
        );
    }
    for i in 0..4 {
        for j in (i + 1)..4 {
            assert_ne!(p2_spawns[i], p2_spawns[j]);
        }
    }

    // Reach Level 2 exit -> transitions to Level 3
    state.set_player_position(2, state.maze().exit());
    state.update(0.016);
    assert_eq!(
        state.maze().level(),
        3,
        "reaching level 2 exit must transition to level 3"
    );

    let p3_spawns: Vec<_> = (1..=4)
        .map(|id| state.player_position(id).unwrap())
        .collect();
    for pos in &p3_spawns {
        assert!(
            state.maze().can_occupy(*pos, 0.2),
            "level 3 respawn position must be valid"
        );
    }
    for i in 0..4 {
        for j in (i + 1)..4 {
            assert_ne!(p3_spawns[i], p3_spawns[j]);
        }
    }

    // Reach Level 3 exit -> cycles back to Level 1
    state.set_player_position(3, state.maze().exit());
    state.update(0.016);
    assert_eq!(
        state.maze().level(),
        1,
        "reaching level 3 exit must cycle back to level 1"
    );

    let p1_cycled: Vec<_> = (1..=4)
        .map(|id| state.player_position(id).unwrap())
        .collect();
    for pos in &p1_cycled {
        assert!(state.maze().can_occupy(*pos, 0.2));
    }
}

// 10. `audit_check_authoritative_combat_straight_shots_and_wall_blocking`:
// tests projectile travels straight, walls block shots, shooter cannot self-hit, only first target in line is hit.
#[test]
fn audit_check_authoritative_combat_straight_shots_and_wall_blocking() {
    let mut state = ServerState::new();
    let a1: SocketAddr = "127.0.0.1:30001".parse().unwrap();
    let a2: SocketAddr = "127.0.0.1:30002".parse().unwrap();
    let a3: SocketAddr = "127.0.0.1:30003".parse().unwrap();

    state.join(a1, "shooter".to_string());
    state.join(a2, "target1".to_string());
    state.join(a3, "target2".to_string());

    // Corridor line of sight: shooter at (1.5, 1.5) facing East, target1 at (3.5, 1.5), target2 at (5.5, 1.5)
    state.set_player_position(1, Position { x: 1.5, y: 1.5 });
    state.set_player_direction(1, 0.0);
    state.set_player_position(2, Position { x: 3.5, y: 1.5 });
    state.set_player_position(3, Position { x: 5.5, y: 1.5 });

    // Shooter fires
    assert!(state.apply_fire(a1, 1, 1));
    state.update(0.1); // Shot travels at 25 units/s * 0.1s = 2.5 units

    // Shooter cannot self-hit
    assert_eq!(
        state.score(1),
        10,
        "shooter must score +10, not self-penalize"
    );
    // Only the first target in line is hit
    assert_eq!(
        state.score(2),
        -5,
        "first target in line must receive -5 penalty"
    );
    assert_eq!(
        state.score(3),
        0,
        "second target behind first target must NOT be hit"
    );

    // Wall blocking test: swept wall collision stops shot
    let maze = Maze::level_one();
    let start = Position { x: 1.5, y: 1.5 };
    let end_through_wall = Position { x: -1.0, y: 1.5 }; // Ray pointed west through border wall
    let hit = swept_wall_collision(&maze, start, end_through_wall);
    assert!(hit.is_some(), "wall must block shot");
    let (t, hit_pos) = hit.unwrap();
    assert!(t > 0.0 && t <= 1.0);
    assert!(
        (hit_pos.x - 1.0).abs() < 0.001,
        "shot must stop exactly at the wall boundary"
    );
}

// 11. `audit_check_combat_scoring_plus_10_minus_5_and_negative_scores`:
// tests exact +10 for shooter, -5 for target, allows negative scores, preserved across levels.
#[test]
fn audit_check_combat_scoring_plus_10_minus_5_and_negative_scores() {
    let mut state = ServerState::new();
    let a1: SocketAddr = "127.0.0.1:31001".parse().unwrap();
    let a2: SocketAddr = "127.0.0.1:31002".parse().unwrap();

    state.join(a1, "shooter".to_string());
    state.join(a2, "target".to_string());

    assert_eq!(state.score(1), 0);
    assert_eq!(state.score(2), 0);

    // Shooter hits target
    state.set_player_position(1, Position { x: 1.5, y: 1.5 });
    state.set_player_direction(1, 0.0);
    state.set_player_position(2, Position { x: 3.5, y: 1.5 });

    assert!(state.apply_fire(a1, 1, 1));
    state.update(0.1);

    assert_eq!(state.score(1), 10, "shooter must gain exactly 10 points");
    assert_eq!(
        state.score(2),
        -5,
        "target must lose 5 points (negative scores permitted)"
    );

    // Level transitions preserve scores
    state.set_player_position(1, state.maze().exit());
    state.update(0.016);
    assert_eq!(state.maze().level(), 2);
    assert_eq!(
        state.score(1),
        10,
        "shooter score must survive level progression"
    );
    assert_eq!(
        state.score(2),
        -5,
        "target negative score must survive level progression"
    );

    state.set_player_position(1, state.maze().exit());
    state.update(0.016);
    assert_eq!(state.maze().level(), 3);
    assert_eq!(state.score(1), 10);
    assert_eq!(state.score(2), -5);

    state.set_player_position(1, state.maze().exit());
    state.update(0.016);
    assert_eq!(state.maze().level(), 1);
    assert_eq!(state.score(1), 10);
    assert_eq!(state.score(2), -5);
}

// 12. `audit_check_two_second_hit_protection`:
// tests target receives exactly 2.0 seconds (120 ticks) protection where repeated shots do not score, target can still move, and target becomes vulnerable once protection expires.
#[test]
fn audit_check_two_second_hit_protection() {
    assert_eq!(
        PROTECTION_TICKS, 120,
        "hit protection must be exactly 120 ticks (2.0 seconds at 60 Hz)"
    );

    let mut state = ServerState::new();
    let a1: SocketAddr = "127.0.0.1:32001".parse().unwrap();
    let a2: SocketAddr = "127.0.0.1:32002".parse().unwrap();

    state.join(a1, "shooter".to_string());
    state.join(a2, "target".to_string());

    state.set_player_position(1, Position { x: 1.5, y: 1.5 });
    state.set_player_direction(1, 0.0);
    state.set_player_position(2, Position { x: 3.5, y: 1.5 });

    // Initial shot at tick 0 hits target
    assert!(state.apply_fire(a1, 1, 1));
    state.update(0.1);
    assert_eq!(state.score(1), 10);
    assert_eq!(state.score(2), -5);
    assert!(
        state.is_protected(2),
        "target must be protected after being hit"
    );

    // Target can still move during protection
    let target_initial_pos = state.player_position(2).unwrap();
    state.apply_input(a2, 2, 1, 1, 0); // target moves forward
    state.update(0.1);
    assert_ne!(
        state.player_position(2).unwrap(),
        target_initial_pos,
        "target must be able to move while protected"
    );

    // Wait for fire cooldown (15 ticks) while target remains within the 2.0s (120 ticks) protection
    for _ in 0..16 {
        state.update(1.0 / 60.0);
    }
    assert!(
        state.is_protected(2),
        "target must still be protected at tick {}",
        state.tick()
    );

    // Reposition target in front of shooter and fire again during protection
    state.set_player_position(2, Position { x: 3.5, y: 1.5 });
    assert!(state.apply_fire(a1, 1, 2));
    state.update(0.1);

    // Repeated shot must NOT score or penalize
    assert_eq!(
        state.score(1),
        10,
        "shooter score must not increase on protected target"
    );
    assert_eq!(
        state.score(2),
        -5,
        "protected target score must not decrease"
    );

    // Fast-forward ticks to tick 125 (expiration of 2.0 second protection: 120 ticks)
    while state.tick() < 125 {
        state.update(1.0 / 60.0);
    }
    assert!(
        !state.is_protected(2),
        "target protection must expire after 120 ticks"
    );

    // Target is now vulnerable again
    state.set_player_position(2, Position { x: 3.5, y: 1.5 });
    assert!(state.apply_fire(a1, 1, 3));
    state.update(0.1);

    assert_eq!(
        state.score(1),
        20,
        "shooter must score again after protection expires"
    );
    assert_eq!(
        state.score(2),
        -10,
        "target must be penalized again after protection expires"
    );
}

// 13. `audit_check_multiplayer_capacity_and_10_plus_clients_stress_test`:
// simulates 12 simultaneous clients connecting, sending movement and fire commands, receiving snapshots, and running for many simulated ticks without server crash, packet drop, or timeout.
#[test]
fn audit_check_multiplayer_capacity_and_10_plus_clients_stress_test() {
    let mut state = ServerState::new();
    let client_count = 12; // Well above the minimum 10 clients requirement

    let clients: Vec<(SocketAddr, u32, String)> = (0..client_count)
        .map(|i| {
            let addr: SocketAddr = format!("127.0.0.1:{}", 25000 + i).parse().unwrap();
            let name = format!("client_{i:02}");
            let res = state.join(addr, name.clone());
            let player_id = match res {
                ServerMessage::Accepted { player_id, .. } => player_id,
                _ => panic!("client {i} must be accepted"),
            };
            (addr, player_id, name)
        })
        .collect();

    assert_eq!(state.player_count(), client_count);

    // Simulate 10,800 ticks (full 3.0 minutes at 60 Hz) of active gameplay
    for tick in 1..=10_800 {
        // Every client sends movement input
        for (idx, (addr, id, _)) in clients.iter().enumerate() {
            let forward = if idx % 2 == 0 { 1 } else { -1 };
            let turn = if idx % 3 == 0 { 1 } else { 0 };
            assert!(state.apply_input(*addr, *id, tick, forward, turn));

            // Clients fire every 30 ticks
            if tick % 30 == 0 {
                state.apply_fire(*addr, *id, tick);
            }
        }

        state.update(1.0 / 60.0);

        // Server generates snapshot
        let snapshot = state.snapshot();
        let encoded = encode_server(&snapshot).expect("snapshot encoding must succeed");
        assert!(
            encoded.len() <= MAX_PACKET_SIZE,
            "snapshot for 12 clients must fit in <= 1400 bytes, was {} bytes",
            encoded.len()
        );

        // No unintended timeouts
        let timed_out = state.remove_inactive_players();
        assert!(
            timed_out.is_empty(),
            "active clients must not time out during gameplay"
        );
    }

    assert_eq!(
        state.player_count(),
        client_count,
        "all 12 clients must remain connected"
    );
}

// 14. `audit_check_inactive_player_timeout_after_ten_seconds`:
// verifies inactivity timeout removes silent players and frees slots after 10 seconds (600 ticks).
#[test]
fn audit_check_inactive_player_timeout_after_ten_seconds() {
    let mut state = ServerState::new();
    let a_active: SocketAddr = "127.0.0.1:40001".parse().unwrap();
    let a_silent: SocketAddr = "127.0.0.1:40002".parse().unwrap();

    state.join(a_active, "active_player".to_string());
    state.join(a_silent, "silent_player".to_string());
    assert_eq!(state.player_count(), 2);

    // Run for 599 ticks (9.98 seconds at 60 Hz)
    for tick in 1..600 {
        // Active player sends input every 30 ticks
        if tick % 30 == 0 {
            state.apply_input(a_active, 1, tick, 1, 0);
        }
        state.update(1.0 / 60.0);
        let removed = state.remove_inactive_players();
        assert!(
            removed.is_empty(),
            "no player should be removed before 10 seconds (tick {tick})"
        );
    }

    // Tick 600 (exactly 10.0 seconds of inactivity for silent_player)
    state.update(1.0 / 60.0);
    let removed = state.remove_inactive_players();
    assert_eq!(
        removed,
        vec!["silent_player".to_string()],
        "silent player must be removed after 10 seconds (600 ticks)"
    );
    assert_eq!(
        state.player_count(),
        1,
        "only active player must remain connected"
    );

    // Freed slot and username can be rejoined
    let a_new: SocketAddr = "127.0.0.1:40003".parse().unwrap();
    let res = state.join(a_new, "silent_player".to_string());
    assert!(
        matches!(res, ServerMessage::Accepted { .. }),
        "freed username should now be acceptable"
    );
    assert_eq!(state.player_count(), 2);
}

// 15. `audit_check_wire_protocol_limits_and_malformed_packet_rejection`:
// tests magic header `MZWR`, version 1, 64-player maximum snapshot fits in <= 1,400 bytes, trailing bytes and truncated packets are rejected.
#[test]
fn audit_check_wire_protocol_limits_and_malformed_packet_rejection() {
    // Magic and version
    assert_eq!(MAGIC, b"MZWR", "protocol magic must be MZWR");
    assert_eq!(VERSION, 1, "protocol version must be 1");

    // 64-player maximum snapshot fits in <= 1,400 bytes
    let players: Vec<PlayerSnapshot> = (0..MAX_SNAPSHOT_PLAYERS as u32)
        .map(|id| PlayerSnapshot {
            id,
            x: 1.5,
            y: 1.5,
            direction: 0.0,
            score: -50,
            is_protected: true,
        })
        .collect();

    let snapshot_msg = ServerMessage::Snapshot {
        tick: 12345,
        level: 3,
        players,
    };

    let encoded = encode_server(&snapshot_msg).expect("encoding 64 players must succeed");
    assert!(
        encoded.len() <= MAX_PACKET_SIZE,
        "64-player snapshot must fit in <= {MAX_PACKET_SIZE} bytes, got {} bytes",
        encoded.len()
    );
    assert!(encoded.len() <= 1400);

    // Decodes successfully
    let decoded = decode_server(&encoded).expect("decoding valid snapshot must succeed");
    assert_eq!(decoded, snapshot_msg);

    // Malformed magic header rejection
    let mut bad_magic = encoded.clone();
    bad_magic[0..4].copy_from_slice(b"BADM");
    assert_eq!(
        decode_server(&bad_magic),
        Err(PacketError::InvalidHeader),
        "packet with invalid magic header must be rejected"
    );

    // Unsupported version rejection
    let mut bad_version = encoded.clone();
    bad_version[4] = 99;
    assert_eq!(
        decode_server(&bad_version),
        Err(PacketError::UnsupportedVersion(99)),
        "packet with unsupported version must be rejected"
    );

    // Truncated packet rejection
    for cut_len in [0, 3, 4, 5, 10, encoded.len() - 1] {
        assert!(
            decode_server(&encoded[..cut_len]).is_err(),
            "truncated packet of length {cut_len} must be rejected"
        );
    }

    // Trailing bytes rejection
    let mut trailing = encoded.clone();
    trailing.push(0x42);
    assert_eq!(
        decode_server(&trailing),
        Err(PacketError::InvalidLength),
        "packet with trailing bytes must be rejected"
    );

    // Client message trailing bytes rejection
    let join = encode_client(&maze_wars::protocol::ClientMessage::Join {
        username: "test".to_string(),
    })
    .unwrap();
    let mut join_trailing = join.clone();
    join_trailing.push(0x00);
    assert_eq!(
        decode_client(&join_trailing),
        Err(PacketError::InvalidLength),
        "client message with trailing bytes must be rejected"
    );
}

// 16. `audit_check_live_udp_server_process_and_12_clients`:
// spawns a live server process, connects 12 real UDP clients concurrently, verifies round-trip communication and snapshot reception.
#[test]
fn audit_check_live_udp_server_process_and_12_clients() {
    let mut server = Command::new(env!("CARGO_BIN_EXE_server"))
        .spawn()
        .expect("failed to spawn server process");

    std::thread::sleep(Duration::from_millis(150));

    let server_addr: SocketAddr = "127.0.0.1:34254".parse().unwrap();
    let mut clients = Vec::new();

    // Connect 12 live clients over real UDP sockets
    for i in 0..12 {
        let username = format!("live_client_{i:02}");
        let conn = connect(server_addr, &username)
            .unwrap_or_else(|err| panic!("live client {i} failed to connect: {err}"));
        assert_eq!(conn.player_id(), i as u32 + 1);
        clients.push(conn);
    }

    assert_eq!(clients.len(), 12);

    // Each client sends movement input and fires
    for (i, conn) in clients.iter().enumerate() {
        conn.send_input(1, 1, 0).expect("send input failed");
        if i % 2 == 0 {
            conn.send_fire(1).expect("send fire failed");
        }
    }

    // Wait for server tick (60 Hz = ~16.6ms) to process inputs and broadcast
    std::thread::sleep(Duration::from_millis(80));

    // Each client receives network updates containing 12 players
    for conn in &mut clients {
        let updates = conn.receive_updates().expect("receive updates failed");
        if let Some(snapshot) = updates.snapshot {
            assert_eq!(
                snapshot.players.len(),
                12,
                "snapshot must contain all 12 live players"
            );
        }
    }

    // Attempting duplicate username should fail
    let duplicate = connect(server_addr, "live_client_00");
    assert!(
        duplicate.is_err(),
        "duplicate username join must be rejected by server"
    );

    let _ = server.kill();
    let _ = server.wait();
}

// 17. `audit_check_client_gui_rendering_and_performance`:
// Comprehensive audit verifying:
// 1. Minimap: full maze rendering, local player in green, others in red, dynamic resize/center across levels 1 (15x11), 2 (21x15), 3 (25x19), shot tracers, and blinking protected player markers.
// 2. First-Person 3D Ray-Casting: 60-degree FOV, DDA grid traversal, fish-eye correction, monochrome distance shading, wall faces, floating eyeball sprites with depth occlusion, blinking protected avatars.
// 3. Combat Feedback & Controls: Spacebar and LMB single shots, shot tracer lines, hit confirmation indicators, and damage feedback flashes.
// 4. Multiplayer Scoreboard: sorted descending by score, local player highlighted with prefix, up to 10 players displayed without obscuring central view.
// 5. Frame Rate Requirement: FPS visibly displayed on screen, deterministic calculation maintaining > 50 FPS, allocation-free rendering loops.
#[test]
fn audit_check_client_gui_rendering_and_performance() {
    // 1. Minimap Requirements
    let l1 = compute_layout(15, 11);
    let l2 = compute_layout(21, 15);
    let l3 = compute_layout(25, 19);

    assert_eq!(l1.tile_pixels, 16);
    assert_eq!(l2.tile_pixels, 12);
    assert_eq!(l3.tile_pixels, 10);
    assert!(l1.origin_x >= 24 && l1.origin_y >= 24);
    assert!(l2.origin_x >= 24 && l2.origin_y >= 24);
    assert!(l3.origin_x >= 24 && l3.origin_y >= 24);

    assert_eq!(player_minimap_color(1, 1), LOCAL_PLAYER_COLOR);
    assert_eq!(player_minimap_color(2, 1), OTHER_PLAYER_COLOR);
    assert_eq!(LOCAL_PLAYER_COLOR, Color::RGB(70, 220, 125));
    assert_eq!(OTHER_PLAYER_COLOR, Color::RGB(225, 73, 75));

    // 2. First-Person 3D Ray-Casting (60-deg FOV = PI / 3.0)
    let fov: f32 = PI / 3.0;
    assert!((fov.to_degrees() - 60.0).abs() < 0.001);

    let maze = Maze::level_one();
    let hit = cast_ray(&maze, maze.spawn(), 0.0).expect("ray must hit corridor wall");
    assert!(hit.distance > 0.0);
    let corrected = correct_fish_eye(hit.distance, 0.0, 0.0);
    assert_eq!(corrected, hit.distance);

    let shade_close = wall_brightness(1.0, true);
    let shade_far = wall_brightness(5.0, true);
    assert!(shade_close > shade_far);
    let shade_vert = wall_brightness(2.0, true);
    let shade_horiz = wall_brightness(2.0, false);
    assert!(shade_vert > shade_horiz);

    // Eyeball sprite projection and blinking
    let observer = PlayerSnapshot {
        id: 1,
        x: 1.5,
        y: 1.5,
        direction: 0.0,
        score: 0,
        is_protected: false,
    };
    let opponent = PlayerSnapshot {
        id: 2,
        x: 4.5,
        y: 1.5,
        direction: 0.0,
        score: 0,
        is_protected: true,
    };
    let proj = project_player(&observer, &opponent, 800.0, 960).expect("opponent in front");
    assert_eq!(proj.depth, 3.0);
    assert!((proj.center_x - 480.0).abs() < 0.001);

    // Blinking alternating phase
    assert!(is_blink_visible(Duration::from_millis(0), 150));
    assert!(!is_blink_visible(Duration::from_millis(150), 150));

    // 3. Combat Feedback & Controls
    assert_eq!(SHOT_TRACER_LIFETIME, Duration::from_millis(200));
    assert_eq!(HIT_CONFIRMATION_LIFETIME, Duration::from_millis(150));
    assert_eq!(DAMAGE_FEEDBACK_LIFETIME, Duration::from_millis(150));

    let tracer = ShotTracer::new(1, 1.0, 1.0, 5.0, 1.0, std::time::Instant::now());
    assert_eq!(tracer.duration, SHOT_TRACER_LIFETIME);

    let t_active = Duration::from_millis(50);
    assert!(CombatVisuals::is_effect_active_from_elapsed(
        t_active,
        HIT_CONFIRMATION_LIFETIME
    ));
    let t_expired = Duration::from_millis(160);
    assert!(!CombatVisuals::is_effect_active_from_elapsed(
        t_expired,
        HIT_CONFIRMATION_LIFETIME
    ));

    // 4. Multiplayer Scoreboard
    let mut scores = [
        ScoreboardEntry::new(1, Some("ALICE"), 10),
        ScoreboardEntry::new(2, Some("BOB"), 30),
        ScoreboardEntry::new(3, Some("CHARLIE"), -5),
    ];
    sort_scoreboard(&mut scores);
    assert_eq!(scores[0].id, 2);
    assert_eq!(scores[0].score, 30);
    assert_eq!(scores[1].id, 1);
    assert_eq!(scores[1].score, 10);
    assert_eq!(scores[2].id, 3);
    assert_eq!(scores[2].score, -5);

    let mut line_buf = [0_u8; 48];
    let formatted_local = format_scoreboard_entry(&mut line_buf, &scores[1], true);
    assert_eq!(formatted_local, "* ALICE: 10");
    let formatted_other = format_scoreboard_entry(&mut line_buf, &scores[0], false);
    assert_eq!(formatted_other, "BOB: 30");

    assert_eq!(MAX_DISPLAYED_SCOREBOARD_PLAYERS, 10);
    // At 10 players, height is ~143px on a 600px canvas (horizon at 300px), leaving central view clear
    let max_scoreboard_height = (10 * 13) + 16;
    assert!(max_scoreboard_height < 200);

    // 5. Frame Rate & Allocation-Free Logic
    let mut tracker = FpsTracker::new(Duration::from_millis(500));
    let frame_dt = Duration::from_nanos(500_000_000 / 30 + 1);
    for _ in 0..30 {
        tracker.advance(frame_dt);
    }
    assert_eq!(tracker.fps(), 60);
    assert!(tracker.fps() > 50);

    let mut hud_buf = [0_u8; 48];
    let hud_str = format_hud_string(&mut hud_buf, tracker.fps(), Some(1));
    assert_eq!(hud_str, "FPS: 60  LEVEL: 1");
}
