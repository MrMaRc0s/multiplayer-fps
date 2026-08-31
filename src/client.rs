use crate::protocol::{
    ClientMessage, MAX_PACKET_SIZE, ServerMessage, decode_server, encode_client,
};
use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

pub const DEFAULT_SERVER_ADDRESS: &str = "127.0.0.1:34254";
const CONNECTION_ATTEMPTS: usize = 3;
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Debug)]
pub struct Connection {
    socket: UdpSocket,
    player_id: u32,
    level: u8,
}

impl Connection {
    pub fn socket(&self) -> &UdpSocket {
        &self.socket
    }

    pub fn player_id(&self) -> u32 {
        self.player_id
    }

    pub fn level(&self) -> u8 {
        self.level
    }
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
                    ServerMessage::Accepted { player_id, level } => Ok(Connection {
                        socket,
                        player_id,
                        level,
                    }),
                    ServerMessage::Rejected { reason } => {
                        Err(format!("server rejected the connection: {reason}"))
                    }
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
