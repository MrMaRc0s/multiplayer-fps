use std::error::Error;
use std::fmt::{self, Display, Formatter};

const MAGIC: &[u8; 4] = b"MZWR";
const VERSION: u8 = 1;
const JOIN: u8 = 1;
const ACCEPTED: u8 = 101;
const REJECTED: u8 = 102;

pub const MAX_PACKET_SIZE: usize = 1_400;
pub const MAX_USERNAME_BYTES: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientMessage {
    Join { username: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerMessage {
    Accepted { player_id: u32, level: u8 },
    Rejected { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PacketError {
    InvalidHeader,
    UnsupportedVersion(u8),
    UnknownMessage(u8),
    InvalidLength,
    InvalidText,
    TextTooLong,
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
    }
}

pub fn decode_client(packet: &[u8]) -> Result<ClientMessage, PacketError> {
    let (kind, payload) = split_packet(packet)?;

    match kind {
        JOIN => Ok(ClientMessage::Join {
            username: read_text(payload, MAX_USERNAME_BYTES)?,
        }),
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
    }
}

pub fn decode_server(packet: &[u8]) -> Result<ServerMessage, PacketError> {
    let (kind, payload) = split_packet(packet)?;

    match kind {
        ACCEPTED if payload.len() == 5 => {
            let id_bytes = payload[..4]
                .try_into()
                .map_err(|_| PacketError::InvalidLength)?;
            Ok(ServerMessage::Accepted {
                player_id: u32::from_be_bytes(id_bytes),
                level: payload[4],
            })
        }
        ACCEPTED => Err(PacketError::InvalidLength),
        REJECTED => Ok(ServerMessage::Rejected {
            reason: read_text(payload, u8::MAX as usize)?,
        }),
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
