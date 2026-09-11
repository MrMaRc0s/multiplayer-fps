use super::{MAX_PLAYERS, TICKS_PER_SECOND};
use crate::protocol::{PlayerSnapshot, ServerMessage};
use crate::world::{Maze, Position};
use std::collections::HashMap;
use std::f32::consts::TAU;
use std::net::SocketAddr;

const PLAYER_RADIUS: f32 = 0.2;
const MOVEMENT_SPEED: f32 = 2.5;
const TURN_SPEED: f32 = 2.4;
const MAX_STEP_SECONDS: f32 = 0.1;
const INACTIVE_PLAYER_TICKS: u32 = TICKS_PER_SECOND * 10;

#[derive(Debug)]
struct Player {
    id: u32,
    username: String,
    position: Position,
    direction: f32,
    forward: i8,
    turn: i8,
    last_input_sequence: Option<u32>,
    last_seen_tick: u32,
}

use super::combat::{CombatEvent, CombatManager, CombatPlayer};

#[derive(Debug)]
pub(super) struct ServerState {
    players: HashMap<SocketAddr, Player>,
    next_player_id: u32,
    tick: u32,
    maze: Maze,
    combat: CombatManager,
}

impl ServerState {
    pub(super) fn new() -> Self {
        Self {
            players: HashMap::new(),
            next_player_id: 1,
            tick: 0,
            maze: Maze::level_one(),
            combat: CombatManager::new(),
        }
    }

    pub(super) fn player_count(&self) -> usize {
        self.players.len()
    }

    pub(super) fn player_identity(&self, address: SocketAddr) -> Option<(&str, u32)> {
        self.players
            .get(&address)
            .map(|player| (player.username.as_str(), player.id))
    }

    pub(super) fn player_addresses(&self) -> impl Iterator<Item = SocketAddr> + '_ {
        self.players.keys().copied()
    }

    pub(super) fn join(&mut self, address: SocketAddr, username: String) -> ServerMessage {
        if let Some(player) = self.players.get_mut(&address) {
            player.last_seen_tick = self.tick;
            return ServerMessage::Accepted {
                player_id: player.id,
                level: self.maze.level(),
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
        let spawn_index = self.players.len();
        self.players.insert(
            address,
            Player {
                id: player_id,
                username: username.to_owned(),
                position: self.maze.spawn_at(spawn_index),
                direction: 0.0,
                forward: 0,
                turn: 0,
                last_input_sequence: None,
                last_seen_tick: self.tick,
            },
        );

        ServerMessage::Accepted {
            player_id,
            level: self.maze.level(),
        }
    }

    pub(super) fn apply_input(
        &mut self,
        address: SocketAddr,
        player_id: u32,
        sequence: u32,
        forward: i8,
        turn: i8,
    ) -> bool {
        let Some(player) = self.players.get_mut(&address) else {
            return false;
        };
        if player.id != player_id
            || player
                .last_input_sequence
                .is_some_and(|previous| !is_newer_sequence(sequence, previous))
        {
            return false;
        }

        player.forward = forward.clamp(-1, 1);
        player.turn = turn.clamp(-1, 1);
        player.last_input_sequence = Some(sequence);
        player.last_seen_tick = self.tick;
        true
    }

    pub(super) fn apply_fire(
        &mut self,
        address: SocketAddr,
        player_id: u32,
        _sequence: u32,
    ) -> bool {
        let Some(player) = self.players.get_mut(&address) else {
            return false;
        };
        if player.id != player_id {
            return false;
        }
        player.last_seen_tick = self.tick;
        let pos = player.position;
        let dir = player.direction;
        self.combat
            .try_fire(player_id, pos, dir, &self.maze, self.tick)
    }

    pub(super) fn update(&mut self, elapsed_seconds: f32) {
        if !elapsed_seconds.is_finite() || elapsed_seconds <= 0.0 {
            return;
        }
        let elapsed_seconds = elapsed_seconds.min(MAX_STEP_SECONDS);

        for player in self.players.values_mut() {
            player.direction = (player.direction
                + f32::from(player.turn) * TURN_SPEED * elapsed_seconds)
                .rem_euclid(TAU);

            let distance = f32::from(player.forward) * MOVEMENT_SPEED * elapsed_seconds;
            let movement_x = player.direction.cos() * distance;
            let movement_y = player.direction.sin() * distance;

            let next_x = Position {
                x: player.position.x + movement_x,
                y: player.position.y,
            };
            if self.maze.can_occupy(next_x, PLAYER_RADIUS) {
                player.position = next_x;
            }

            let next_y = Position {
                x: player.position.x,
                y: player.position.y + movement_y,
            };
            if self.maze.can_occupy(next_y, PLAYER_RADIUS) {
                player.position = next_y;
            }
        }

        let mut combat_players: HashMap<u32, CombatPlayer> = self
            .players
            .values()
            .map(|p| {
                (
                    p.id,
                    CombatPlayer {
                        position: p.position,
                        score: self.combat.score(p.id),
                    },
                )
            })
            .collect();
        self.combat
            .update(elapsed_seconds, &self.maze, &mut combat_players, self.tick);

        let exit_reached = self.players.values().any(|player| {
            self.maze
                .is_exit(player.position.x.floor() as i32, player.position.y.floor() as i32)
        });

        if exit_reached {
            let next_level = (self.maze.level() % 3) + 1;
            self.maze = Maze::for_level(next_level).expect("valid maze level");
            for (i, player) in self.players.values_mut().enumerate() {
                player.position = self.maze.spawn_at(i);
                player.forward = 0;
                player.turn = 0;
                player.last_input_sequence = None;
            }
        }

        self.tick = self.tick.wrapping_add(1);
    }

    pub(super) fn remove_inactive_players(&mut self) -> Vec<String> {
        let current_tick = self.tick;
        let mut removed_usernames = Vec::new();

        self.players.retain(|_, player| {
            let inactive =
                current_tick.wrapping_sub(player.last_seen_tick) >= INACTIVE_PLAYER_TICKS;
            if inactive {
                removed_usernames.push(player.username.clone());
            }
            !inactive
        });

        removed_usernames
    }

    pub(super) fn drain_combat_events(&mut self) -> Vec<CombatEvent> {
        self.combat.drain_events()
    }

    pub(super) fn roster(&self) -> ServerMessage {
        let mut players: Vec<_> = self
            .players
            .values()
            .map(|p| (p.id, p.username.clone()))
            .collect();
        players.sort_by_key(|(id, _)| *id);
        ServerMessage::Roster { players }
    }

    pub(super) fn snapshot(&self) -> ServerMessage {
        let mut players: Vec<_> = self
            .players
            .values()
            .map(|player| PlayerSnapshot {
                id: player.id,
                x: player.position.x,
                y: player.position.y,
                direction: player.direction,
                score: self.combat.score(player.id),
                is_protected: self.combat.is_protected(player.id, self.tick),
            })
            .collect();
        players.sort_by_key(|player| player.id);

        ServerMessage::Snapshot {
            tick: self.tick,
            level: self.maze.level(),
            players,
        }
    }
}

fn is_newer_sequence(candidate: u32, previous: u32) -> bool {
    let difference = candidate.wrapping_sub(previous);
    difference != 0 && difference < u32::MAX / 2
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address(port: u16) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], port))
    }

    fn accepted_player_id(message: ServerMessage) -> u32 {
        match message {
            ServerMessage::Accepted { player_id, .. } => player_id,
            _ => panic!("expected the player to be accepted"),
        }
    }

    #[test]
    fn joins_a_new_player() {
        let mut state = ServerState::new();

        let response = state.join(address(4000), "chris".to_owned());

        assert_eq!(accepted_player_id(response), 1);
        assert_eq!(state.player_count(), 1);
    }

    #[test]
    fn repeated_join_keeps_the_same_id() {
        let mut state = ServerState::new();
        let player_address = address(4000);
        let first = state.join(player_address, "chris".to_owned());

        let repeated = state.join(player_address, "chris".to_owned());

        assert_eq!(repeated, first);
        assert_eq!(state.player_count(), 1);
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

    #[test]
    fn movement_uses_the_newest_authenticated_input() {
        let mut state = ServerState::new();
        let player_address = address(4000);
        let player_id = accepted_player_id(state.join(player_address, "chris".to_owned()));

        assert!(!state.apply_input(address(4001), player_id, 1, 1, 0));
        assert!(state.apply_input(player_address, player_id, 2, 1, 0));
        assert!(!state.apply_input(player_address, player_id, 1, -1, 0));
        state.update(0.1);

        let player = &state.players[&player_address];
        assert!(player.position.x > state.maze.spawn().x);
    }

    #[test]
    fn collision_stops_movement_through_a_wall() {
        let mut state = ServerState::new();
        let player_address = address(4000);
        let player_id = accepted_player_id(state.join(player_address, "chris".to_owned()));
        state.apply_input(player_address, player_id, 1, -1, 0);

        for _ in 0..20 {
            state.update(0.1);
        }

        assert!(state.players[&player_address].position.x >= 1.2);
    }

    #[test]
    fn snapshot_contains_the_authoritative_position() {
        let mut state = ServerState::new();
        let player_address = address(4000);
        let player_id = accepted_player_id(state.join(player_address, "chris".to_owned()));
        state.apply_input(player_address, player_id, 1, 1, 0);
        state.update(0.1);

        let ServerMessage::Snapshot {
            tick,
            level,
            players,
        } = state.snapshot()
        else {
            panic!("expected a snapshot");
        };

        assert_eq!(tick, 1);
        assert_eq!(level, 1);
        assert_eq!(players.len(), 1);
        assert_eq!(players[0].id, player_id);
        assert!(players[0].x > state.maze.spawn().x);
    }

    #[test]
    fn inactive_player_is_removed_after_ten_seconds() {
        let mut state = ServerState::new();
        state.join(address(4000), "chris".to_owned());

        for _ in 0..INACTIVE_PLAYER_TICKS - 1 {
            state.update(1.0 / TICKS_PER_SECOND as f32);
        }
        assert!(state.remove_inactive_players().is_empty());

        state.update(1.0 / TICKS_PER_SECOND as f32);

        assert_eq!(state.remove_inactive_players(), ["chris"]);
        assert_eq!(state.player_count(), 0);
    }

    #[test]
    fn recent_input_keeps_a_player_connected() {
        let mut state = ServerState::new();
        let player_address = address(4000);
        let player_id = accepted_player_id(state.join(player_address, "chris".to_owned()));

        for _ in 0..INACTIVE_PLAYER_TICKS - 1 {
            state.update(1.0 / TICKS_PER_SECOND as f32);
        }
        state.apply_input(player_address, player_id, 1, 0, 0);
        state.update(1.0 / TICKS_PER_SECOND as f32);

        assert!(state.remove_inactive_players().is_empty());
        assert_eq!(state.player_count(), 1);
    }

    #[test]
    fn players_receive_separate_valid_spawn_positions() {
        let mut state = ServerState::new();
        state.join(address(4000), "alice".to_owned());
        state.join(address(4001), "bob".to_owned());
        state.join(address(4002), "charlie".to_owned());

        let pos_alice = state.players[&address(4000)].position;
        let pos_bob = state.players[&address(4001)].position;
        let pos_charlie = state.players[&address(4002)].position;

        assert_ne!(pos_alice, pos_bob);
        assert_ne!(pos_alice, pos_charlie);
        assert_ne!(pos_bob, pos_charlie);
        assert!(state.maze.can_occupy(pos_alice, PLAYER_RADIUS));
        assert!(state.maze.can_occupy(pos_bob, PLAYER_RADIUS));
        assert!(state.maze.can_occupy(pos_charlie, PLAYER_RADIUS));
    }

    #[test]
    fn exit_tile_triggers_authoritative_level_progression_and_cycles() {
        let mut state = ServerState::new();
        let p1 = address(4000);
        state.join(p1, "runner".to_owned());

        assert_eq!(state.maze.level(), 1);

        // Teleport player 1 onto exit of level 1
        state.players.get_mut(&p1).unwrap().position = state.maze.exit();
        state.update(0.016);
        assert_eq!(state.maze.level(), 2);

        // Teleport player 1 onto exit of level 2
        state.players.get_mut(&p1).unwrap().position = state.maze.exit();
        state.update(0.016);
        assert_eq!(state.maze.level(), 3);

        // Teleport player 1 onto exit of level 3 -> cycles back to 1
        state.players.get_mut(&p1).unwrap().position = state.maze.exit();
        state.update(0.016);
        assert_eq!(state.maze.level(), 1);
    }

    #[test]
    fn scores_and_roster_survive_level_progression() {
        let mut state = ServerState::new();
        let p1 = address(4000);
        let p2 = address(4001);
        let id1 = accepted_player_id(state.join(p1, "shooter".to_owned()));
        let id2 = accepted_player_id(state.join(p2, "target".to_owned()));

        // Position shooter and target in clear line of sight
        state.players.get_mut(&p1).unwrap().position = Position { x: 1.5, y: 1.5 };
        state.players.get_mut(&p1).unwrap().direction = 0.0;
        state.players.get_mut(&p2).unwrap().position = Position { x: 3.5, y: 1.5 };

        // Shoot target
        assert!(state.apply_fire(p1, id1, 1));
        state.update(0.1); // Shot travels and hits

        assert_eq!(state.combat.score(id1), 10);
        assert_eq!(state.combat.score(id2), -5);

        // Advance level
        state.players.get_mut(&p1).unwrap().position = state.maze.exit();
        state.update(0.016);
        assert_eq!(state.maze.level(), 2);

        // Confirm scores are preserved across level progression
        assert_eq!(state.combat.score(id1), 10);
        assert_eq!(state.combat.score(id2), -5);

        // Confirm roster is accurate
        let roster = state.roster();
        let ServerMessage::Roster { players } = roster else {
            panic!("expected roster");
        };
        assert_eq!(players, vec![(id1, "shooter".to_owned()), (id2, "target".to_owned())]);
    }
}
