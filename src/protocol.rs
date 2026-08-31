use std::error::Error;
use std::fmt::{self, Display, Formatter};

const MAGIC: &[u8; 4] = b"MZWR";
const VERSION: u8 = 1;
const JOIN: u8 = 1;
const INPUT: u8 = 2;
const ACCEPTED: u8 = 101;
const REJECTED: u8 = 102;
const SNAPSHOT: u8 = 103;

pub const MAX_PACKET_SIZE: usize = 1_400;
pub const MAX_USERNAME_BYTES: usize = 20;
pub const MAX_SNAPSHOT_PLAYERS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientMessage {
    Join {
        username: String,
    },
    Input {
        player_id: u32,
        sequence: u32,
        forward: i8,
        turn: i8,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerSnapshot {
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub direction: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ServerMessage {
    Accepted {
        player_id: u32,
        level: u8,
    },
    Rejected {
        reason: String,
    },
    Snapshot {
        tick: u32,
        level: u8,
        players: Vec<PlayerSnapshot>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PacketError {
    InvalidHeader,
    UnsupportedVersion(u8),
    UnknownMessage(u8),
    InvalidLength,
    InvalidText,
    TextTooLong,
    InvalidInput,
    InvalidNumber,
    TooManyPlayers,
}

impl Display for PacketError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidHeader => write!(formatter, "invalid packet header"),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported protocol version {version}")
            }
            Self::UnknownMessage(kind) => write!(formatter, "unknown message type {kind}"),
            Self::InvalidLength => write!(formatter, "invalid packet length"),
            Self::InvalidText => write!(formatter, "packet contains invalid UTF-8"),
            Self::TextTooLong => write!(formatter, "text field is too long"),
            Self::InvalidInput => write!(formatter, "movement input is outside the valid range"),
            Self::InvalidNumber => write!(formatter, "packet contains a non-finite number"),
            Self::TooManyPlayers => write!(formatter, "snapshot contains too many players"),
        }
    }
}

impl Error for PacketError {}

pub fn encode_client(message: &ClientMessage) -> Result<Vec<u8>, PacketError> {
    match message {
        ClientMessage::Join { username } => {
            let mut packet = packet_header(JOIN);
            push_text(&mut packet, username, MAX_USERNAME_BYTES)?;
            Ok(packet)
        }
        ClientMessage::Input {
            player_id,
            sequence,
            forward,
            turn,
        } => {
            validate_axis(*forward)?;
            validate_axis(*turn)?;

            let mut packet = packet_header(INPUT);
            packet.extend_from_slice(&player_id.to_be_bytes());
            packet.extend_from_slice(&sequence.to_be_bytes());
            packet.push(*forward as u8);
            packet.push(*turn as u8);
            Ok(packet)
        }
    }
}

pub fn decode_client(packet: &[u8]) -> Result<ClientMessage, PacketError> {
    let (kind, payload) = split_packet(packet)?;

    match kind {
        JOIN => Ok(ClientMessage::Join {
            username: read_text(payload, MAX_USERNAME_BYTES)?,
        }),
        INPUT if payload.len() == 10 => {
            let forward = payload[8] as i8;
            let turn = payload[9] as i8;
            validate_axis(forward)?;
            validate_axis(turn)?;

            Ok(ClientMessage::Input {
                player_id: read_u32(&payload[..4])?,
                sequence: read_u32(&payload[4..8])?,
                forward,
                turn,
            })
        }
        INPUT => Err(PacketError::InvalidLength),
        _ => Err(PacketError::UnknownMessage(kind)),
    }
}

pub fn encode_server(message: &ServerMessage) -> Result<Vec<u8>, PacketError> {
    match message {
        ServerMessage::Accepted { player_id, level } => {
            let mut packet = packet_header(ACCEPTED);
            packet.extend_from_slice(&player_id.to_be_bytes());
            packet.push(*level);
            Ok(packet)
        }
        ServerMessage::Rejected { reason } => {
            let mut packet = packet_header(REJECTED);
            push_text(&mut packet, reason, u8::MAX as usize)?;
            Ok(packet)
        }
        ServerMessage::Snapshot {
            tick,
            level,
            players,
        } => {
            if players.len() > MAX_SNAPSHOT_PLAYERS {
                return Err(PacketError::TooManyPlayers);
            }

            let mut packet = packet_header(SNAPSHOT);
            packet.extend_from_slice(&tick.to_be_bytes());
            packet.push(*level);
            packet.push(players.len() as u8);
            for player in players {
                if !player.x.is_finite() || !player.y.is_finite() || !player.direction.is_finite() {
                    return Err(PacketError::InvalidNumber);
                }
                packet.extend_from_slice(&player.id.to_be_bytes());
                packet.extend_from_slice(&player.x.to_be_bytes());
                packet.extend_from_slice(&player.y.to_be_bytes());
                packet.extend_from_slice(&player.direction.to_be_bytes());
            }
            Ok(packet)
        }
    }
}

pub fn decode_server(packet: &[u8]) -> Result<ServerMessage, PacketError> {
    let (kind, payload) = split_packet(packet)?;

    match kind {
        ACCEPTED if payload.len() == 5 => Ok(ServerMessage::Accepted {
            player_id: read_u32(&payload[..4])?,
            level: payload[4],
        }),
        ACCEPTED => Err(PacketError::InvalidLength),
        REJECTED => Ok(ServerMessage::Rejected {
            reason: read_text(payload, u8::MAX as usize)?,
        }),
        SNAPSHOT if payload.len() >= 6 => {
            let player_count = payload[5] as usize;
            if player_count > MAX_SNAPSHOT_PLAYERS || payload.len() != 6 + player_count * 16 {
                return Err(PacketError::InvalidLength);
            }

            let mut players = Vec::with_capacity(player_count);
            for player in payload[6..].chunks_exact(16) {
                let snapshot = PlayerSnapshot {
                    id: read_u32(&player[..4])?,
                    x: read_f32(&player[4..8])?,
                    y: read_f32(&player[8..12])?,
                    direction: read_f32(&player[12..16])?,
                };
                if !snapshot.x.is_finite()
                    || !snapshot.y.is_finite()
                    || !snapshot.direction.is_finite()
                {
                    return Err(PacketError::InvalidNumber);
                }
                players.push(snapshot);
            }

            Ok(ServerMessage::Snapshot {
                tick: read_u32(&payload[..4])?,
                level: payload[4],
                players,
            })
        }
        SNAPSHOT => Err(PacketError::InvalidLength),
        _ => Err(PacketError::UnknownMessage(kind)),
    }
}

fn packet_header(kind: u8) -> Vec<u8> {
    let mut packet = Vec::with_capacity(32);
    packet.extend_from_slice(MAGIC);
    packet.push(VERSION);
    packet.push(kind);
    packet
}

fn split_packet(packet: &[u8]) -> Result<(u8, &[u8]), PacketError> {
    if packet.len() < 6 || &packet[..4] != MAGIC {
        return Err(PacketError::InvalidHeader);
    }
    if packet[4] != VERSION {
        return Err(PacketError::UnsupportedVersion(packet[4]));
    }

    Ok((packet[5], &packet[6..]))
}

fn push_text(packet: &mut Vec<u8>, text: &str, maximum: usize) -> Result<(), PacketError> {
    let bytes = text.as_bytes();
    if bytes.len() > maximum || bytes.len() > u8::MAX as usize {
        return Err(PacketError::TextTooLong);
    }

    packet.push(bytes.len() as u8);
    packet.extend_from_slice(bytes);
    Ok(())
}

fn read_text(payload: &[u8], maximum: usize) -> Result<String, PacketError> {
    let Some((&length, text)) = payload.split_first() else {
        return Err(PacketError::InvalidLength);
    };
    if length as usize > maximum || text.len() != length as usize {
        return Err(PacketError::InvalidLength);
    }

    String::from_utf8(text.to_vec()).map_err(|_| PacketError::InvalidText)
}

fn read_u32(bytes: &[u8]) -> Result<u32, PacketError> {
    let bytes = bytes.try_into().map_err(|_| PacketError::InvalidLength)?;
    Ok(u32::from_be_bytes(bytes))
}

fn read_f32(bytes: &[u8]) -> Result<f32, PacketError> {
    let bytes = bytes.try_into().map_err(|_| PacketError::InvalidLength)?;
    Ok(f32::from_be_bytes(bytes))
}

fn validate_axis(value: i8) -> Result<(), PacketError> {
    if (-1..=1).contains(&value) {
        Ok(())
    } else {
        Err(PacketError::InvalidInput)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_message_round_trip() {
        let message = ClientMessage::Join {
            username: "chris".to_owned(),
        };

        let packet = encode_client(&message).unwrap();

        assert_eq!(decode_client(&packet).unwrap(), message);
    }

    #[test]
    fn accepted_message_round_trip() {
        let message = ServerMessage::Accepted {
            player_id: 42,
            level: 2,
        };

        let packet = encode_server(&message).unwrap();

        assert_eq!(decode_server(&packet).unwrap(), message);
    }

    #[test]
    fn rejected_message_round_trip() {
        let message = ServerMessage::Rejected {
            reason: "Server is full".to_owned(),
        };

        let packet = encode_server(&message).unwrap();

        assert_eq!(decode_server(&packet).unwrap(), message);
    }

    #[test]
    fn input_message_round_trip() {
        let message = ClientMessage::Input {
            player_id: 7,
            sequence: 19,
            forward: 1,
            turn: -1,
        };

        let packet = encode_client(&message).unwrap();

        assert_eq!(decode_client(&packet).unwrap(), message);
    }

    #[test]
    fn snapshot_message_round_trip() {
        let message = ServerMessage::Snapshot {
            tick: 30,
            level: 1,
            players: vec![
                PlayerSnapshot {
                    id: 1,
                    x: 1.5,
                    y: 2.5,
                    direction: 0.0,
                },
                PlayerSnapshot {
                    id: 2,
                    x: 7.25,
                    y: 4.75,
                    direction: 1.5,
                },
            ],
        };

        let packet = encode_server(&message).unwrap();

        assert_eq!(decode_server(&packet).unwrap(), message);
    }

    #[test]
    fn maximum_snapshot_stays_below_packet_limit() {
        let players = (0..MAX_SNAPSHOT_PLAYERS)
            .map(|id| PlayerSnapshot {
                id: id as u32,
                x: 1.5,
                y: 1.5,
                direction: 0.0,
            })
            .collect();
        let message = ServerMessage::Snapshot {
            tick: 1,
            level: 1,
            players,
        };

        let packet = encode_server(&message).unwrap();

        assert!(packet.len() <= MAX_PACKET_SIZE);
        assert_eq!(decode_server(&packet).unwrap(), message);
    }

    #[test]
    fn invalid_movement_axis_is_not_encoded() {
        let message = ClientMessage::Input {
            player_id: 1,
            sequence: 1,
            forward: 2,
            turn: 0,
        };

        assert_eq!(encode_client(&message), Err(PacketError::InvalidInput));
    }

    #[test]
    fn overlong_username_is_not_encoded() {
        let message = ClientMessage::Join {
            username: "x".repeat(MAX_USERNAME_BYTES + 1),
        };

        assert_eq!(encode_client(&message), Err(PacketError::TextTooLong));
    }

    #[test]
    fn malformed_packet_is_not_decoded() {
        assert_eq!(
            decode_client(b"not a Maze Wars packet"),
            Err(PacketError::InvalidHeader)
        );
    }
}
