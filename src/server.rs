use self::state::ServerState;
use crate::protocol::{
    ClientMessage, MAX_PACKET_SIZE, ServerMessage, decode_client, encode_server,
};
use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::thread;
use std::time::{Duration, Instant};

mod state;

pub const SERVER_ADDRESS: &str = "0.0.0.0:34254";
pub const MAX_PLAYERS: usize = 64;
const TICK_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / 60);

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
            }

            send_response(socket, source, &response)?;
        }
        ClientMessage::Input {
            player_id,
            sequence,
            forward,
            turn,
        } => {
            state.apply_input(source, player_id, sequence, forward, turn);
        }
    }

    Ok(())
}

fn broadcast_snapshot(socket: &UdpSocket, state: &ServerState) -> io::Result<()> {
    let packet = encode_server(&state.snapshot()).map_err(io::Error::other)?;
    for address in state.player_addresses() {
        if let Err(error) = socket.send_to(&packet, address) {
            eprintln!("Could not send snapshot to {address}: {error}");
        }
    }
    Ok(())
}

fn send_response(
    socket: &UdpSocket,
    destination: SocketAddr,
    message: &ServerMessage,
) -> io::Result<()> {
    let packet = encode_server(message).map_err(io::Error::other)?;
    socket.send_to(&packet, destination)?;
    Ok(())
}
