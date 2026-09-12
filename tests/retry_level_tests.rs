use maze_wars::client::connect;
use maze_wars::enemy::EnemyManager;
use maze_wars::protocol::{ClientMessage, MAX_PACKET_SIZE, ServerMessage, decode_client, encode_server};
use maze_wars::server::state::ServerState;
use maze_wars::window::menu::{GameState, MenuAction, PAUSE_MENU_ITEMS};
use maze_wars::world::{Maze, Position};
use std::net::UdpSocket;
use std::thread;
use std::time::Duration;

#[test]
fn test_retry_resets_player_and_respawns_all_enemies() {
    let maze = Maze::level_one();
    let spawn = maze.spawn();
    let mut server = ServerState::new();
    let addr = "127.0.0.1:45678".parse().unwrap();

    let accepted = server.join(addr, "tester".to_string());
    let player_id = match accepted {
        ServerMessage::Accepted { player_id, .. } => player_id,
        _ => panic!("join failed"),
    };

    // Initialize enemy manager with player at spawn
    let mut enemy_manager = EnemyManager::for_maze(&maze, Some(spawn));
    enemy_manager.set_enabled(true);
    let initial_enemy_count = enemy_manager.enemies().len();
    assert!(initial_enemy_count > 0);
    let initial_positions: Vec<Position> =
        enemy_manager.enemies().iter().map(|e| e.position).collect();

    // Move player forward away from spawn
    server.apply_input(addr, player_id, 1, 1, 0);
    server.update(0.2);
    let moved_pos = server.player_position(player_id).unwrap();
    assert_ne!(moved_pos, spawn);

    // Player shoots and kills the first enemy
    let first_pos = enemy_manager.enemies()[0].position;
    let hit = enemy_manager.check_shot_hit(
        Position {
            x: first_pos.x - 0.2,
            y: first_pos.y,
        },
        Position {
            x: first_pos.x + 0.2,
            y: first_pos.y,
        },
    );
    assert!(hit.is_some());
    assert_eq!(enemy_manager.active_enemy_count(), initial_enemy_count - 1);

    // Another enemy catches the player
    let alive_idx = enemy_manager
        .enemies()
        .iter()
        .position(|e| e.alive)
        .unwrap();
    enemy_manager.enemies_mut()[alive_idx].position = moved_pos;
    let lost = enemy_manager.update(0.016, &maze, moved_pos);
    assert!(lost);
    assert!(enemy_manager.has_player_lost());

    // Execute RETRY: player triggers retry
    assert!(server.apply_respawn(addr, player_id));
    enemy_manager.reset(&maze, Some(spawn));

    // Verify player is reset to initial spawn
    let respawned_pos = server.player_position(player_id).unwrap();
    assert_eq!(respawned_pos, spawn);
    assert_eq!(server.player_direction(player_id).unwrap(), 0.0);

    // Verify snapshot reflects the reset
    let ServerMessage::Snapshot { players, .. } = server.snapshot() else {
        panic!("expected snapshot");
    };
    assert_eq!(players[0].x, spawn.x);
    assert_eq!(players[0].y, spawn.y);
    assert_eq!(players[0].direction, 0.0);

    // Verify enemy manager is fully reset
    assert!(!enemy_manager.has_player_lost());
    assert!(enemy_manager.is_enabled());
    assert_eq!(enemy_manager.active_enemy_count(), initial_enemy_count);
    for (i, enemy) in enemy_manager.enemies().iter().enumerate() {
        assert!(enemy.alive);
        assert_eq!(enemy.position, initial_positions[i]);
    }
}

#[test]
fn test_authoritative_network_respawn_cycle() {
    let server_socket = UdpSocket::bind("127.0.0.1:0").expect("bind server");
    let server_addr = server_socket.local_addr().expect("local addr");
    server_socket
        .set_read_timeout(Some(Duration::from_millis(500)))
        .expect("timeout");

    // Spawn mock server thread
    let handle = thread::spawn(move || {
        let mut server_state = ServerState::new();
        let mut buffer = [0_u8; MAX_PACKET_SIZE];

        // 1. Receive Join
        let (len, client_addr) = server_socket.recv_from(&mut buffer).expect("recv join");
        let client_msg = decode_client(&buffer[..len]).expect("decode join");
        let player_id = match client_msg {
            ClientMessage::Join { username } => {
                let resp = server_state.join(client_addr, username);
                let encoded = encode_server(&resp).expect("encode accepted");
                server_socket
                    .send_to(&encoded, client_addr)
                    .expect("send accepted");
                match resp {
                    ServerMessage::Accepted { player_id, .. } => player_id,
                    _ => panic!("join rejected"),
                }
            }
            _ => panic!("expected join"),
        };

        // Move player away from spawn
        server_state.set_player_position(player_id, Position { x: 5.5, y: 5.5 });
        server_state.set_player_direction(player_id, 1.2);

        // 2. Receive Respawn
        let (len, respawn_addr) = server_socket.recv_from(&mut buffer).expect("recv respawn");
        assert_eq!(respawn_addr, client_addr);
        let respawn_msg = decode_client(&buffer[..len]).expect("decode respawn");
        match respawn_msg {
            ClientMessage::Respawn {
                player_id: r_player_id,
            } => {
                assert_eq!(r_player_id, player_id);
                assert!(server_state.apply_respawn(client_addr, r_player_id));
            }
            _ => panic!("expected respawn message"),
        }

        // 3. Send Snapshot back to client
        let snapshot = server_state.snapshot();
        let encoded_snap = encode_server(&snapshot).expect("encode snap");
        server_socket
            .send_to(&encoded_snap, client_addr)
            .expect("send snapshot");
    });

    // Client connects and sends respawn
    let mut connection = connect(server_addr, "retry_bot").expect("client connect");

    // Send respawn
    connection.send_respawn().expect("send respawn");

    // Receive snapshot confirming spawn reset with polling
    let mut snapshot = None;
    let deadline = std::time::Instant::now() + Duration::from_millis(1000);
    while std::time::Instant::now() < deadline {
        if let Ok(Some(s)) = connection.receive_latest_snapshot() {
            snapshot = Some(s);
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    let snapshot = snapshot.expect("snapshot received within timeout");
    assert_eq!(snapshot.players.len(), 1);
    let maze = Maze::level_one();
    assert_eq!(snapshot.players[0].x, maze.spawn().x);
    assert_eq!(snapshot.players[0].y, maze.spawn().y);
    assert_eq!(snapshot.players[0].direction, 0.0);

    handle.join().expect("server thread finished");
}

#[test]
fn test_pause_menu_reset_level_flow() {
    assert_eq!(PAUSE_MENU_ITEMS[3], "RESET LEVEL");

    let mut state = GameState::Paused { selected_item: 0 };
    // Navigate down to item 3 (RESET LEVEL)
    state.move_selection_down();
    state.move_selection_down();
    state.move_selection_down();
    assert_eq!(state, GameState::Paused { selected_item: 3 });

    // Activate RESET LEVEL
    let action = state.activate_selection();
    assert_eq!(action, MenuAction::ResetLevel);
    assert_eq!(state, GameState::Playing);
}
