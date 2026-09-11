use crate::protocol::{
    ClientMessage, CombatEventKind, MAX_PACKET_SIZE, PlayerSnapshot, ServerMessage, decode_server,
    encode_client,
};
use std::io;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::Duration;

pub const DEFAULT_SERVER_ADDRESS: &str = "127.0.0.1:34254";
const CONNECTION_ATTEMPTS: usize = 3;
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Debug)]
pub struct Connection {
    socket: UdpSocket,
    player_id: u32,
    level: u8,
    last_snapshot_tick: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorldSnapshot {
    pub tick: u32,
    pub level: u8,
    pub players: Vec<PlayerSnapshot>,
}

#[derive(Debug, Default, Clone)]
pub struct NetworkUpdates {
    pub snapshot: Option<WorldSnapshot>,
    pub combat_events: Vec<CombatEventKind>,
    pub roster: Option<Vec<(u32, String)>>,
}

impl Connection {
    pub fn player_id(&self) -> u32 {
        self.player_id
    }

    pub fn level(&self) -> u8 {
        self.level
    }

    pub fn send_input(&self, sequence: u32, forward: i8, turn: i8) -> Result<(), String> {
        let packet = encode_client(&ClientMessage::Input {
            player_id: self.player_id,
            sequence,
            forward,
            turn,
        })
        .map_err(|error| format!("could not prepare movement input: {error}"))?;

        self.socket
            .send(&packet)
            .map_err(|error| format!("could not send movement input: {error}"))?;
        Ok(())
    }

    pub fn send_fire(&self, sequence: u32) -> Result<(), String> {
        let packet = encode_client(&ClientMessage::Fire {
            player_id: self.player_id,
            sequence,
        })
        .map_err(|error| format!("could not prepare fire input: {error}"))?;

        self.socket
            .send(&packet)
            .map_err(|error| format!("could not send fire input: {error}"))?;
        Ok(())
    }

    pub fn receive_updates(&mut self) -> Result<NetworkUpdates, String> {
        let mut buffer = [0_u8; MAX_PACKET_SIZE];
        let mut updates = NetworkUpdates::default();

        loop {
            match self.socket.recv(&mut buffer) {
                Ok(length) => match decode_server(&buffer[..length])
                    .map_err(|error| format!("server sent an invalid response: {error}"))?
                {
                    ServerMessage::Snapshot {
                        tick,
                        level,
                        players,
                    } if self
                        .last_snapshot_tick
                        .is_none_or(|previous| is_newer_sequence(tick, previous)) =>
                    {
                        self.last_snapshot_tick = Some(tick);
                        self.level = level;
                        updates.snapshot = Some(WorldSnapshot {
                            tick,
                            level,
                            players,
                        });
                    }
                    ServerMessage::CombatEvent { kind, .. } => {
                        updates.combat_events.push(kind);
                    }
                    ServerMessage::Roster { players } => {
                        updates.roster = Some(players);
                    }
                    ServerMessage::Rejected { reason } => {
                        return Err(format!("server ended the connection: {reason}"));
                    }
                    ServerMessage::Accepted { .. } | ServerMessage::Snapshot { .. } => {}
                },
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(format!("could not receive world state: {error}")),
            }
        }

        Ok(updates)
    }

    pub fn receive_latest_snapshot(&mut self) -> Result<Option<WorldSnapshot>, String> {
        self.receive_updates().map(|u| u.snapshot)
    }
}

pub fn parse_server_address(input: &str) -> Result<SocketAddr, String> {
    let trimmed = input.trim();
    let address = if trimmed.is_empty() {
        DEFAULT_SERVER_ADDRESS
    } else {
        trimmed
    };

    address
        .to_socket_addrs()
        .map_err(|error| format!("invalid server address '{address}': {error}"))?
        .next()
        .ok_or_else(|| format!("server address '{address}' did not resolve"))
}

pub fn validate_username(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Name cannot be empty.".to_string());
    }
    if trimmed.len() > crate::protocol::MAX_USERNAME_BYTES {
        return Err(format!(
            "Name must contain 1 to {} bytes.",
            crate::protocol::MAX_USERNAME_BYTES
        ));
    }
    Ok(trimmed.to_string())
}

pub fn connect(server: SocketAddr, username: &str) -> Result<Connection, String> {
    let socket = UdpSocket::bind("0.0.0.0:0")
        .map_err(|error| format!("could not open a UDP socket: {error}"))?;
    socket
        .connect(server)
        .map_err(|error| format!("could not use server address {server}: {error}"))?;
    socket
        .set_read_timeout(Some(RESPONSE_TIMEOUT))
        .map_err(|error| format!("could not configure the UDP socket: {error}"))?;

    let join_packet = encode_client(&ClientMessage::Join {
        username: username.to_owned(),
    })
    .map_err(|error| format!("could not prepare the join request: {error}"))?;
    let mut buffer = [0_u8; MAX_PACKET_SIZE];

    for attempt in 1..=CONNECTION_ATTEMPTS {
        match socket.send(&join_packet) {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {}
            Err(error) => return Err(format!("could not send data to {server}: {error}")),
        }

        match socket.recv(&mut buffer) {
            Ok(length) => {
                let response = decode_server(&buffer[..length])
                    .map_err(|error| format!("server sent an invalid response: {error}"))?;
                return match response {
                    ServerMessage::Accepted { player_id, level } => {
                        socket.set_nonblocking(true).map_err(|error| {
                            format!("could not configure the UDP socket: {error}")
                        })?;
                        Ok(Connection {
                            socket,
                            player_id,
                            level,
                            last_snapshot_tick: None,
                        })
                    }
                    ServerMessage::Rejected { reason } => {
                        Err(format!("server rejected the connection: {reason}"))
                    }
                    ServerMessage::Snapshot { .. }
                    | ServerMessage::CombatEvent { .. }
                    | ServerMessage::Roster { .. } => continue,
                };
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::ConnectionRefused
                ) =>
            {
                if attempt < CONNECTION_ATTEMPTS {
                    println!(
                        "No response from server, retrying ({attempt}/{CONNECTION_ATTEMPTS})..."
                    );
                }
            }
            Err(error) => return Err(format!("could not receive data from {server}: {error}")),
        }
    }

    Err(format!(
        "server {server} is not running or reachable; start it first with `cargo run --bin server`"
    ))
}

fn is_newer_sequence(candidate: u32, previous: u32) -> bool {
    let difference = candidate.wrapping_sub(previous);
    difference != 0 && difference < u32::MAX / 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_sequence_must_move_forward() {
        assert!(is_newer_sequence(11, 10));
        assert!(!is_newer_sequence(10, 10));
        assert!(!is_newer_sequence(9, 10));
    }

    #[test]
    fn snapshot_sequence_can_wrap_to_zero() {
        assert!(is_newer_sequence(0, u32::MAX));
    }
}
