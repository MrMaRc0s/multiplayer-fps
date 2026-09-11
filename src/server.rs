use self::state::ServerState;
use crate::protocol::{
    ClientMessage, CombatEventKind, MAX_PACKET_SIZE, ServerMessage, decode_client, encode_server,
};
use crate::world::Position;
use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::thread;
use std::time::{Duration, Instant};

pub mod combat;
pub mod state;

pub const SERVER_ADDRESS: &str = "0.0.0.0:34254";
pub const MAX_PLAYERS: usize = 64;
const TICKS_PER_SECOND: u32 = 60;
const TICK_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / TICKS_PER_SECOND as u64);

pub fn run() -> io::Result<()> {
    let socket = UdpSocket::bind(SERVER_ADDRESS)?;
    socket.set_nonblocking(true)?;
    let mut state = ServerState::new();
    let mut buffer = [0_u8; MAX_PACKET_SIZE];
    let mut last_tick = Instant::now();

    println!("Maze Wars server listening on {SERVER_ADDRESS}");
    println!("Waiting for players (capacity: {MAX_PLAYERS})...");

    loop {
        loop {
            match socket.recv_from(&mut buffer) {
                Ok((length, source)) => {
                    handle_packet(&socket, &mut state, source, &buffer[..length])?;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(error),
            }
        }

        let now = Instant::now();
        let elapsed = now.duration_since(last_tick);
        if elapsed >= TICK_INTERVAL {
            state.update(elapsed.as_secs_f32());
            let timed_out = state.remove_inactive_players();
            if !timed_out.is_empty() {
                for username in timed_out {
                    println!("{username} left after timing out");
                }
                broadcast_message(&socket, &state, &state.roster())?;
            }

            let combat_events = state.drain_combat_events();
            for event in combat_events {
                let msg = match event {
                    combat::CombatEvent::ShotFired {
                        event_id,
                        shooter_id,
                        position,
                        direction,
                    } => {
                        let max_end = Position {
                            x: position.x + direction.cos() * 50.0,
                            y: position.y + direction.sin() * 50.0,
                        };
                        let hit_pos = if let Some((_, hit)) =
                            combat::swept_wall_collision(state.maze(), position, max_end)
                        {
                            hit
                        } else {
                            max_end
                        };
                        ServerMessage::CombatEvent {
                            sequence: event_id,
                            kind: CombatEventKind::ShotFired {
                                shooter_id,
                                from_x: position.x,
                                from_y: position.y,
                                to_x: hit_pos.x,
                                to_y: hit_pos.y,
                            },
                        }
                    },
                    combat::CombatEvent::PlayerHit {
                        event_id,
                        shooter_id,
                        target_id,
                        ..
                    } => ServerMessage::CombatEvent {
                        sequence: event_id,
                        kind: CombatEventKind::PlayerHit {
                            shooter_id,
                            target_id,
                        },
                    },
                };
                broadcast_message(&socket, &state, &msg)?;
            }

            broadcast_snapshot(&socket, &state)?;
            last_tick = now;
        }

        thread::sleep(Duration::from_millis(1));
    }
}

fn handle_packet(
    socket: &UdpSocket,
    state: &mut ServerState,
    source: SocketAddr,
    packet: &[u8],
) -> io::Result<()> {
    let message = match decode_client(packet) {
        Ok(message) => message,
        Err(error) => {
            eprintln!("Ignored invalid packet from {source}: {error}");
            return Ok(());
        }
    };

    match message {
        ClientMessage::Join { username } => {
            let previous_player_count = state.player_count();
            let response = state.join(source, username);

            if state.player_count() > previous_player_count
                && let Some((username, player_id)) = state.player_identity(source)
            {
                println!("{username} joined from {source} as player {player_id}");
                send_response(socket, source, &response)?;
                broadcast_message(socket, state, &state.roster())?;
            } else {
                send_response(socket, source, &response)?;
            }
        }
        ClientMessage::Input {
            player_id,
            sequence,
            forward,
            turn,
        } => {
            state.apply_input(source, player_id, sequence, forward, turn);
        }
        ClientMessage::Fire {
            player_id,
            sequence,
        } => {
            state.apply_fire(source, player_id, sequence);
        }
    }

    Ok(())
}

fn broadcast_snapshot(socket: &UdpSocket, state: &ServerState) -> io::Result<()> {
    broadcast_message(socket, state, &state.snapshot())
}

fn broadcast_message(
    socket: &UdpSocket,
    state: &ServerState,
    message: &ServerMessage,
) -> io::Result<()> {
    match encode_server(message) {
        Ok(packet) => {
            for address in state.player_addresses() {
                if let Err(error) = socket.send_to(&packet, address) {
                    eprintln!("Could not send message to {address}: {error}");
                }
            }
        }
        Err(error) => {
            eprintln!("Could not encode broadcast message: {error}");
        }
    }
    Ok(())
}

fn send_response(
    socket: &UdpSocket,
    destination: SocketAddr,
    message: &ServerMessage,
) -> io::Result<()> {
    match encode_server(message) {
        Ok(packet) => {
            if let Err(error) = socket.send_to(&packet, destination) {
                eprintln!("Could not send response to {destination}: {error}");
            }
        }
        Err(error) => {
            eprintln!("Could not encode response for {destination}: {error}");
        }
    }
    Ok(())
}
