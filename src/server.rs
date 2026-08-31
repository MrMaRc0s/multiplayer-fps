use crate::protocol::{
    ClientMessage, MAX_PACKET_SIZE, ServerMessage, decode_client, encode_server,
};
use std::collections::HashMap;
use std::io;
use std::net::{SocketAddr, UdpSocket};

pub const SERVER_ADDRESS: &str = "0.0.0.0:34254";
pub const MAX_PLAYERS: usize = 64;

#[derive(Debug)]
struct Player {
    id: u32,
    username: String,
}

#[derive(Debug)]
struct ServerState {
    players: HashMap<SocketAddr, Player>,
    next_player_id: u32,
}

impl ServerState {
    fn new() -> Self {
        Self {
            players: HashMap::new(),
            next_player_id: 1,
        }
    }

    fn join(&mut self, address: SocketAddr, username: String) -> ServerMessage {
        if let Some(player) = self.players.get(&address) {
            return ServerMessage::Accepted {
                player_id: player.id,
                level: 1,
            };
        }

        let username = username.trim();
        let rejection = if username.is_empty() {
            Some("Username cannot be empty")
        } else if self.players.len() >= MAX_PLAYERS {
            Some("Server is full")
        } else if self
            .players
            .values()
            .any(|player| player.username == username)
        {
            Some("Username is already in use")
        } else {
            None
        };

        if let Some(reason) = rejection {
            return ServerMessage::Rejected {
                reason: reason.to_owned(),
            };
        }

        let player_id = self.next_player_id;
        self.next_player_id = self.next_player_id.wrapping_add(1).max(1);
        self.players.insert(
            address,
            Player {
                id: player_id,
                username: username.to_owned(),
            },
        );

        ServerMessage::Accepted {
            player_id,
            level: 1,
        }
    }
}

pub fn run() -> io::Result<()> {
    let socket = UdpSocket::bind(SERVER_ADDRESS)?;
    let mut state = ServerState::new();
    let mut buffer = [0_u8; MAX_PACKET_SIZE];

    println!("Maze Wars server listening on {SERVER_ADDRESS}");
    println!("Waiting for players (capacity: {MAX_PLAYERS})...");

    loop {
        let (length, source) = socket.recv_from(&mut buffer)?;
        let message = match decode_client(&buffer[..length]) {
            Ok(message) => message,
            Err(error) => {
                eprintln!("Ignored invalid packet from {source}: {error}");
                continue;
            }
        };

        match message {
            ClientMessage::Join { username } => {
                let previous_player_count = state.players.len();
                let response = state.join(source, username);

                if state.players.len() > previous_player_count {
                    let player = &state.players[&source];
                    println!(
                        "{} joined from {source} as player {}",
                        player.username, player.id
                    );
                }

                send_response(&socket, source, &response)?;
            }
            ClientMessage::Input { .. } => {}
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn address(port: u16) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], port))
    }

    #[test]
    fn joins_a_new_player() {
        let mut state = ServerState::new();

        let response = state.join(address(4000), "chris".to_owned());

        assert_eq!(
            response,
            ServerMessage::Accepted {
                player_id: 1,
                level: 1
            }
        );
        assert_eq!(state.players.len(), 1);
    }

    #[test]
    fn repeated_join_keeps_the_same_id() {
        let mut state = ServerState::new();
        let player_address = address(4000);
        let first = state.join(player_address, "chris".to_owned());

        let repeated = state.join(player_address, "chris".to_owned());

        assert_eq!(repeated, first);
        assert_eq!(state.players.len(), 1);
    }

    #[test]
    fn duplicate_username_is_rejected() {
        let mut state = ServerState::new();
        state.join(address(4000), "chris".to_owned());

        let response = state.join(address(4001), "chris".to_owned());

        assert_eq!(
            response,
            ServerMessage::Rejected {
                reason: "Username is already in use".to_owned()
            }
        );
    }

    #[test]
    fn full_server_rejects_the_next_player() {
        let mut state = ServerState::new();
        for index in 0..MAX_PLAYERS {
            let response = state.join(address(4000 + index as u16), format!("player-{index}"));
            assert!(matches!(response, ServerMessage::Accepted { .. }));
        }

        let response = state.join(address(5000), "one-too-many".to_owned());

        assert_eq!(
            response,
            ServerMessage::Rejected {
                reason: "Server is full".to_owned()
            }
        );
    }
}
