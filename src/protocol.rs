use std::error::Error;
use std::fmt::{self, Display, Formatter};

const MAGIC: &[u8; 4] = b"MZWR";
const VERSION: u8 = 1;
const JOIN: u8 = 1;
const INPUT: u8 = 2;
const FIRE: u8 = 3;
const ACCEPTED: u8 = 101;
const REJECTED: u8 = 102;
const SNAPSHOT: u8 = 103;
const COMBAT_EVENT: u8 = 104;
const ROSTER: u8 = 105;

const COMBAT_EVENT_SHOT_FIRED: u8 = 1;
const COMBAT_EVENT_PLAYER_HIT: u8 = 2;

pub const MAX_PACKET_SIZE: usize = 1_400;
pub const MAX_USERNAME_BYTES: usize = 20;
pub const MAX_SNAPSHOT_PLAYERS: usize = 64;
pub const MAX_ROSTER_PLAYERS: usize = 64;

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
    Fire {
        player_id: u32,
        sequence: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerSnapshot {
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub direction: f32,
    pub score: i32,
    pub is_protected: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CombatEventKind {
    ShotFired {
        shooter_id: u32,
        from_x: f32,
        from_y: f32,
        to_x: f32,
        to_y: f32,
    },
    PlayerHit {
        shooter_id: u32,
        target_id: u32,
    },
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
    CombatEvent {
        sequence: u32,
        kind: CombatEventKind,
    },
    Roster {
        players: Vec<(u32, String)>,
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
    let packet = match message {
        ClientMessage::Join { username } => {
            let mut packet = packet_header(JOIN);
            push_text(&mut packet, username, MAX_USERNAME_BYTES)?;
            packet
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
            packet
        }
        ClientMessage::Fire {
            player_id,
            sequence,
        } => {
            let mut packet = packet_header(FIRE);
            packet.extend_from_slice(&player_id.to_be_bytes());
            packet.extend_from_slice(&sequence.to_be_bytes());
            packet
        }
    };

    if packet.len() > MAX_PACKET_SIZE {
        return Err(PacketError::InvalidLength);
    }
    Ok(packet)
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
        FIRE if payload.len() == 8 => Ok(ClientMessage::Fire {
            player_id: read_u32(&payload[..4])?,
            sequence: read_u32(&payload[4..8])?,
        }),
        FIRE => Err(PacketError::InvalidLength),
        _ => Err(PacketError::UnknownMessage(kind)),
    }
}

pub fn encode_server(message: &ServerMessage) -> Result<Vec<u8>, PacketError> {
    let packet = match message {
        ServerMessage::Accepted { player_id, level } => {
            let mut packet = packet_header(ACCEPTED);
            packet.extend_from_slice(&player_id.to_be_bytes());
            packet.push(*level);
            packet
        }
        ServerMessage::Rejected { reason } => {
            let mut packet = packet_header(REJECTED);
            push_text(&mut packet, reason, u8::MAX as usize)?;
            packet
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
                packet.extend_from_slice(&player.score.to_be_bytes());
                let flags = if player.is_protected { 1_u8 } else { 0_u8 };
                packet.push(flags);
            }
            packet
        }
        ServerMessage::CombatEvent { sequence, kind } => {
            let mut packet = packet_header(COMBAT_EVENT);
            packet.extend_from_slice(&sequence.to_be_bytes());
            match kind {
                CombatEventKind::ShotFired {
                    shooter_id,
                    from_x,
                    from_y,
                    to_x,
                    to_y,
                } => {
                    if !from_x.is_finite()
                        || !from_y.is_finite()
                        || !to_x.is_finite()
                        || !to_y.is_finite()
                    {
                        return Err(PacketError::InvalidNumber);
                    }
                    packet.push(COMBAT_EVENT_SHOT_FIRED);
                    packet.extend_from_slice(&shooter_id.to_be_bytes());
                    packet.extend_from_slice(&from_x.to_be_bytes());
                    packet.extend_from_slice(&from_y.to_be_bytes());
                    packet.extend_from_slice(&to_x.to_be_bytes());
                    packet.extend_from_slice(&to_y.to_be_bytes());
                }
                CombatEventKind::PlayerHit {
                    shooter_id,
                    target_id,
                } => {
                    packet.push(COMBAT_EVENT_PLAYER_HIT);
                    packet.extend_from_slice(&shooter_id.to_be_bytes());
                    packet.extend_from_slice(&target_id.to_be_bytes());
                }
            }
            packet
        }
        ServerMessage::Roster { players } => {
            if players.len() > MAX_ROSTER_PLAYERS {
                return Err(PacketError::TooManyPlayers);
            }

            let mut packet = packet_header(ROSTER);
            packet.push(players.len() as u8);
            for (id, username) in players {
                packet.extend_from_slice(&id.to_be_bytes());
                push_text(&mut packet, username, MAX_USERNAME_BYTES)?;
            }
            packet
        }
    };

    if packet.len() > MAX_PACKET_SIZE {
        return Err(PacketError::InvalidLength);
    }
    Ok(packet)
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
            if player_count > MAX_SNAPSHOT_PLAYERS {
                return Err(PacketError::TooManyPlayers);
            }
            if payload.len() != 6 + player_count * 21 {
                return Err(PacketError::InvalidLength);
            }

            let mut players = Vec::with_capacity(player_count);
            for player in payload[6..].chunks_exact(21) {
                let snapshot = PlayerSnapshot {
                    id: read_u32(&player[..4])?,
                    x: read_f32(&player[4..8])?,
                    y: read_f32(&player[8..12])?,
                    direction: read_f32(&player[12..16])?,
                    score: read_i32(&player[16..20])?,
                    is_protected: (player[20] & 0x01) != 0,
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
        COMBAT_EVENT if payload.len() >= 5 => {
            let sequence = read_u32(&payload[..4])?;
            let event_tag = payload[4];
            match event_tag {
                COMBAT_EVENT_SHOT_FIRED if payload.len() == 25 => {
                    let from_x = read_f32(&payload[9..13])?;
                    let from_y = read_f32(&payload[13..17])?;
                    let to_x = read_f32(&payload[17..21])?;
                    let to_y = read_f32(&payload[21..25])?;
                    if !from_x.is_finite()
                        || !from_y.is_finite()
                        || !to_x.is_finite()
                        || !to_y.is_finite()
                    {
                        return Err(PacketError::InvalidNumber);
                    }
                    Ok(ServerMessage::CombatEvent {
                        sequence,
                        kind: CombatEventKind::ShotFired {
                            shooter_id: read_u32(&payload[5..9])?,
                            from_x,
                            from_y,
                            to_x,
                            to_y,
                        },
                    })
                }
                COMBAT_EVENT_SHOT_FIRED => Err(PacketError::InvalidLength),
                COMBAT_EVENT_PLAYER_HIT if payload.len() == 13 => Ok(ServerMessage::CombatEvent {
                    sequence,
                    kind: CombatEventKind::PlayerHit {
                        shooter_id: read_u32(&payload[5..9])?,
                        target_id: read_u32(&payload[9..13])?,
                    },
                }),
                COMBAT_EVENT_PLAYER_HIT => Err(PacketError::InvalidLength),
                _ => Err(PacketError::UnknownMessage(event_tag)),
            }
        }
        COMBAT_EVENT => Err(PacketError::InvalidLength),
        ROSTER if !payload.is_empty() => {
            let player_count = payload[0] as usize;
            if player_count > MAX_ROSTER_PLAYERS {
                return Err(PacketError::TooManyPlayers);
            }

            let mut offset = 1;
            let mut players = Vec::with_capacity(player_count);
            for _ in 0..player_count {
                if offset + 5 > payload.len() {
                    return Err(PacketError::InvalidLength);
                }
                let id = read_u32(&payload[offset..offset + 4])?;
                offset += 4;
                let name_len = payload[offset] as usize;
                offset += 1;
                if name_len > MAX_USERNAME_BYTES {
                    return Err(PacketError::TextTooLong);
                }
                if offset + name_len > payload.len() {
                    return Err(PacketError::InvalidLength);
                }
                let username = String::from_utf8(payload[offset..offset + name_len].to_vec())
                    .map_err(|_| PacketError::InvalidText)?;
                offset += name_len;
                players.push((id, username));
            }
            if offset != payload.len() {
                return Err(PacketError::InvalidLength);
            }

            Ok(ServerMessage::Roster { players })
        }
        ROSTER => Err(PacketError::InvalidLength),
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
    if packet.len() > MAX_PACKET_SIZE {
        return Err(PacketError::InvalidLength);
    }
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
    if length as usize > maximum {
        return Err(PacketError::TextTooLong);
    }
    if text.len() != length as usize {
        return Err(PacketError::InvalidLength);
    }

    String::from_utf8(text.to_vec()).map_err(|_| PacketError::InvalidText)
}

fn read_u32(bytes: &[u8]) -> Result<u32, PacketError> {
    let bytes = bytes.try_into().map_err(|_| PacketError::InvalidLength)?;
    Ok(u32::from_be_bytes(bytes))
}

fn read_i32(bytes: &[u8]) -> Result<i32, PacketError> {
    let bytes = bytes.try_into().map_err(|_| PacketError::InvalidLength)?;
    Ok(i32::from_be_bytes(bytes))
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
    fn fire_message_round_trip() {
        let message = ClientMessage::Fire {
            player_id: 42,
            sequence: 100,
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
                    score: 10,
                    is_protected: false,
                },
                PlayerSnapshot {
                    id: 2,
                    x: 7.25,
                    y: 4.75,
                    direction: 1.5,
                    score: -5,
                    is_protected: true,
                },
            ],
        };

        let packet = encode_server(&message).unwrap();

        assert_eq!(decode_server(&packet).unwrap(), message);
    }

    #[test]
    fn snapshot_with_negative_score_and_flags() {
        let message = ServerMessage::Snapshot {
            tick: 50,
            level: 3,
            players: vec![PlayerSnapshot {
                id: 10,
                x: 4.25,
                y: 9.75,
                direction: -1.5,
                score: -25,
                is_protected: true,
            }],
        };

        let packet = encode_server(&message).unwrap();
        let decoded = decode_server(&packet).unwrap();
        assert_eq!(decoded, message);
    }

    #[test]
    fn combat_event_shot_fired_round_trip() {
        let message = ServerMessage::CombatEvent {
            sequence: 123,
            kind: CombatEventKind::ShotFired {
                shooter_id: 7,
                from_x: 2.5,
                from_y: 3.5,
                to_x: 10.0,
                to_y: 3.5,
            },
        };

        let packet = encode_server(&message).unwrap();

        assert_eq!(decode_server(&packet).unwrap(), message);
    }

    #[test]
    fn combat_event_player_hit_round_trip() {
        let message = ServerMessage::CombatEvent {
            sequence: 124,
            kind: CombatEventKind::PlayerHit {
                shooter_id: 7,
                target_id: 8,
            },
        };

        let packet = encode_server(&message).unwrap();

        assert_eq!(decode_server(&packet).unwrap(), message);
    }

    #[test]
    fn roster_message_round_trip() {
        let message = ServerMessage::Roster {
            players: vec![
                (1, "alice".to_owned()),
                (2, "bob".to_owned()),
                (3, "charlie".to_owned()),
            ],
        };

        let packet = encode_server(&message).unwrap();

        assert_eq!(decode_server(&packet).unwrap(), message);

        let empty_roster = ServerMessage::Roster { players: vec![] };
        let empty_packet = encode_server(&empty_roster).unwrap();
        assert_eq!(decode_server(&empty_packet).unwrap(), empty_roster);
    }

    #[test]
    fn maximum_snapshot_stays_below_packet_limit() {
        let players = (0..MAX_SNAPSHOT_PLAYERS)
            .map(|id| PlayerSnapshot {
                id: id as u32,
                x: 1.5,
                y: 1.5,
                direction: 0.0,
                score: id as i32,
                is_protected: id % 2 == 0,
            })
            .collect();
        let message = ServerMessage::Snapshot {
            tick: 1,
            level: 1,
            players,
        };

        let packet = encode_server(&message).unwrap();

        assert_eq!(packet.len(), 1_356);
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
        assert_eq!(
            decode_server(b"not a Maze Wars packet"),
            Err(PacketError::InvalidHeader)
        );
        assert_eq!(decode_client(&[1, 2, 3]), Err(PacketError::InvalidHeader));
        assert_eq!(decode_server(&[1, 2, 3]), Err(PacketError::InvalidHeader));
    }

    #[test]
    fn non_finite_numbers_are_rejected() {
        let invalid_snapshot = ServerMessage::Snapshot {
            tick: 1,
            level: 1,
            players: vec![PlayerSnapshot {
                id: 1,
                x: f32::NAN,
                y: 1.0,
                direction: 0.0,
                score: 0,
                is_protected: false,
            }],
        };
        assert_eq!(
            encode_server(&invalid_snapshot),
            Err(PacketError::InvalidNumber)
        );

        let invalid_shot = ServerMessage::CombatEvent {
            sequence: 1,
            kind: CombatEventKind::ShotFired {
                shooter_id: 1,
                from_x: 0.0,
                from_y: f32::INFINITY,
                to_x: 1.0,
                to_y: 1.0,
            },
        };
        assert_eq!(
            encode_server(&invalid_shot),
            Err(PacketError::InvalidNumber)
        );
    }

    #[test]
    fn too_many_players_rejected() {
        let players = (0..=MAX_SNAPSHOT_PLAYERS)
            .map(|id| PlayerSnapshot {
                id: id as u32,
                x: 1.0,
                y: 1.0,
                direction: 0.0,
                score: 0,
                is_protected: false,
            })
            .collect();
        let message = ServerMessage::Snapshot {
            tick: 1,
            level: 1,
            players,
        };
        assert_eq!(encode_server(&message), Err(PacketError::TooManyPlayers));

        let roster_players = (0..=MAX_ROSTER_PLAYERS)
            .map(|id| (id as u32, format!("p{id}")))
            .collect();
        let roster_msg = ServerMessage::Roster {
            players: roster_players,
        };
        assert_eq!(encode_server(&roster_msg), Err(PacketError::TooManyPlayers));
    }

    #[test]
    fn overlong_roster_username_is_rejected() {
        let roster_msg = ServerMessage::Roster {
            players: vec![(1, "x".repeat(MAX_USERNAME_BYTES + 1))],
        };
        assert_eq!(encode_server(&roster_msg), Err(PacketError::TextTooLong));
    }

    #[test]
    fn truncated_packets_are_rejected() {
        // Client Join truncated
        let valid_join = encode_client(&ClientMessage::Join {
            username: "alex".to_owned(),
        })
        .unwrap();
        assert_eq!(
            decode_client(&valid_join[..valid_join.len() - 1]),
            Err(PacketError::InvalidLength)
        );

        // Client Input truncated
        let valid_input = encode_client(&ClientMessage::Input {
            player_id: 1,
            sequence: 1,
            forward: 1,
            turn: 0,
        })
        .unwrap();
        assert_eq!(
            decode_client(&valid_input[..valid_input.len() - 1]),
            Err(PacketError::InvalidLength)
        );

        // Client Fire truncated
        let valid_fire = encode_client(&ClientMessage::Fire {
            player_id: 1,
            sequence: 1,
        })
        .unwrap();
        assert_eq!(
            decode_client(&valid_fire[..valid_fire.len() - 1]),
            Err(PacketError::InvalidLength)
        );

        // Server Accepted truncated
        let valid_accepted = encode_server(&ServerMessage::Accepted {
            player_id: 1,
            level: 1,
        })
        .unwrap();
        assert_eq!(
            decode_server(&valid_accepted[..valid_accepted.len() - 1]),
            Err(PacketError::InvalidLength)
        );

        // Server Rejected truncated
        let valid_rejected = encode_server(&ServerMessage::Rejected {
            reason: "no".to_owned(),
        })
        .unwrap();
        assert_eq!(
            decode_server(&valid_rejected[..valid_rejected.len() - 1]),
            Err(PacketError::InvalidLength)
        );

        // Server Snapshot truncated
        let valid_snapshot = encode_server(&ServerMessage::Snapshot {
            tick: 1,
            level: 1,
            players: vec![PlayerSnapshot {
                id: 1,
                x: 1.0,
                y: 1.0,
                direction: 0.0,
                score: 0,
                is_protected: false,
            }],
        })
        .unwrap();
        assert_eq!(
            decode_server(&valid_snapshot[..valid_snapshot.len() - 5]),
            Err(PacketError::InvalidLength)
        );

        // Server CombatEvent ShotFired truncated
        let valid_shot = encode_server(&ServerMessage::CombatEvent {
            sequence: 1,
            kind: CombatEventKind::ShotFired {
                shooter_id: 1,
                from_x: 0.0,
                from_y: 0.0,
                to_x: 1.0,
                to_y: 1.0,
            },
        })
        .unwrap();
        assert_eq!(
            decode_server(&valid_shot[..valid_shot.len() - 1]),
            Err(PacketError::InvalidLength)
        );

        // Server CombatEvent PlayerHit truncated
        let valid_hit = encode_server(&ServerMessage::CombatEvent {
            sequence: 1,
            kind: CombatEventKind::PlayerHit {
                shooter_id: 1,
                target_id: 2,
            },
        })
        .unwrap();
        assert_eq!(
            decode_server(&valid_hit[..valid_hit.len() - 1]),
            Err(PacketError::InvalidLength)
        );

        // Server Roster truncated
        let valid_roster = encode_server(&ServerMessage::Roster {
            players: vec![(1, "sam".to_owned())],
        })
        .unwrap();
        assert_eq!(
            decode_server(&valid_roster[..valid_roster.len() - 1]),
            Err(PacketError::InvalidLength)
        );
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        // Client Join trailing byte
        let mut join = encode_client(&ClientMessage::Join {
            username: "chris".to_owned(),
        })
        .unwrap();
        join.push(0);
        assert_eq!(decode_client(&join), Err(PacketError::InvalidLength));

        // Client Input trailing byte
        let mut input = encode_client(&ClientMessage::Input {
            player_id: 1,
            sequence: 1,
            forward: 1,
            turn: 0,
        })
        .unwrap();
        input.push(0);
        assert_eq!(decode_client(&input), Err(PacketError::InvalidLength));

        // Client Fire trailing byte
        let mut fire = encode_client(&ClientMessage::Fire {
            player_id: 1,
            sequence: 1,
        })
        .unwrap();
        fire.push(0);
        assert_eq!(decode_client(&fire), Err(PacketError::InvalidLength));

        // Server Accepted trailing byte
        let mut accepted = encode_server(&ServerMessage::Accepted {
            player_id: 1,
            level: 1,
        })
        .unwrap();
        accepted.push(0);
        assert_eq!(decode_server(&accepted), Err(PacketError::InvalidLength));

        // Server Rejected trailing byte
        let mut rejected = encode_server(&ServerMessage::Rejected {
            reason: "full".to_owned(),
        })
        .unwrap();
        rejected.push(0);
        assert_eq!(decode_server(&rejected), Err(PacketError::InvalidLength));

        // Server Snapshot trailing byte
        let mut snapshot = encode_server(&ServerMessage::Snapshot {
            tick: 1,
            level: 1,
            players: vec![],
        })
        .unwrap();
        snapshot.push(0);
        assert_eq!(decode_server(&snapshot), Err(PacketError::InvalidLength));

        // Server CombatEvent ShotFired trailing byte
        let mut shot = encode_server(&ServerMessage::CombatEvent {
            sequence: 1,
            kind: CombatEventKind::ShotFired {
                shooter_id: 1,
                from_x: 0.0,
                from_y: 0.0,
                to_x: 1.0,
                to_y: 1.0,
            },
        })
        .unwrap();
        shot.push(0);
        assert_eq!(decode_server(&shot), Err(PacketError::InvalidLength));

        // Server CombatEvent PlayerHit trailing byte
        let mut hit = encode_server(&ServerMessage::CombatEvent {
            sequence: 1,
            kind: CombatEventKind::PlayerHit {
                shooter_id: 1,
                target_id: 2,
            },
        })
        .unwrap();
        hit.push(0);
        assert_eq!(decode_server(&hit), Err(PacketError::InvalidLength));

        // Server Roster trailing byte
        let mut roster = encode_server(&ServerMessage::Roster {
            players: vec![(1, "sam".to_owned())],
        })
        .unwrap();
        roster.push(0);
        assert_eq!(decode_server(&roster), Err(PacketError::InvalidLength));
    }

    #[test]
    fn oversized_packets_are_rejected() {
        let oversized = vec![0; MAX_PACKET_SIZE + 1];
        assert_eq!(decode_client(&oversized), Err(PacketError::InvalidLength));
        assert_eq!(decode_server(&oversized), Err(PacketError::InvalidLength));
    }

    #[test]
    fn unsupported_version_and_unknown_message_rejected() {
        let bad_version = [b'M', b'Z', b'W', b'R', 99, 1];
        assert_eq!(
            decode_client(&bad_version),
            Err(PacketError::UnsupportedVersion(99))
        );
        assert_eq!(
            decode_server(&bad_version),
            Err(PacketError::UnsupportedVersion(99))
        );

        let unknown_client = [b'M', b'Z', b'W', b'R', 1, 99];
        assert_eq!(
            decode_client(&unknown_client),
            Err(PacketError::UnknownMessage(99))
        );

        let unknown_server = [b'M', b'Z', b'W', b'R', 1, 99];
        assert_eq!(
            decode_server(&unknown_server),
            Err(PacketError::UnknownMessage(99))
        );

        let mut unknown_combat = packet_header(COMBAT_EVENT);
        unknown_combat.extend_from_slice(&1_u32.to_be_bytes());
        unknown_combat.push(99);
        assert_eq!(
            decode_server(&unknown_combat),
            Err(PacketError::UnknownMessage(99))
        );
    }

    #[test]
    fn invalid_text_is_rejected() {
        let mut packet = packet_header(JOIN);
        packet.push(2);
        packet.extend_from_slice(&[0xFF, 0xFF]);
        assert_eq!(decode_client(&packet), Err(PacketError::InvalidText));

        let mut roster_packet = packet_header(ROSTER);
        roster_packet.push(1);
        roster_packet.extend_from_slice(&1_u32.to_be_bytes());
        roster_packet.push(2);
        roster_packet.extend_from_slice(&[0xFF, 0xFF]);
        assert_eq!(decode_server(&roster_packet), Err(PacketError::InvalidText));
    }
}
