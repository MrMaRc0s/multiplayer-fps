use crate::world::{Maze, Position, Tile};
use std::collections::HashMap;

pub const SHOT_SPEED: f32 = 25.0;
pub const PLAYER_RADIUS: f32 = 0.2;
pub const MAX_ACTIVE_SHOTS: usize = 32;
pub const FIRE_COOLDOWN_TICKS: u32 = 15;
pub const PROTECTION_TICKS: u32 = 120;
pub const MAX_SHOT_LIFETIME_SECONDS: f32 = 2.0;
pub const HIT_AWARD: i32 = 10;
pub const HIT_PENALTY: i32 = 5;
const MAX_PENDING_EVENTS: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CombatPlayer {
    pub position: Position,
    pub score: i32,
}

impl CombatPlayer {
    pub fn new(position: Position, score: i32) -> Self {
        Self { position, score }
    }
}

pub type CombatTarget = CombatPlayer;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActiveShot {
    pub id: u32,
    pub shooter_id: u32,
    pub position: Position,
    pub direction: f32,
    pub lifetime_seconds: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CombatEvent {
    ShotFired {
        event_id: u32,
        shooter_id: u32,
        position: Position,
        direction: f32,
    },
    PlayerHit {
        event_id: u32,
        shooter_id: u32,
        target_id: u32,
        hit_position: Position,
        shooter_score: i32,
        target_score: i32,
    },
}

impl CombatEvent {
    pub fn event_id(&self) -> u32 {
        match self {
            Self::ShotFired { event_id, .. } => *event_id,
            Self::PlayerHit { event_id, .. } => *event_id,
        }
    }
}

#[derive(Debug, Default)]
pub struct CombatManager {
    shots: Vec<ActiveShot>,
    last_fire_tick: HashMap<u32, u32>,
    hit_protection: HashMap<u32, u32>,
    scores: HashMap<u32, i32>,
    pending_events: Vec<CombatEvent>,
    next_event_id: u32,
    next_shot_id: u32,
}

impl CombatManager {
    pub fn new() -> Self {
        Self {
            shots: Vec::with_capacity(MAX_ACTIVE_SHOTS),
            last_fire_tick: HashMap::new(),
            hit_protection: HashMap::new(),
            scores: HashMap::new(),
            pending_events: Vec::with_capacity(MAX_PENDING_EVENTS),
            next_event_id: 1,
            next_shot_id: 1,
        }
    }

    pub fn active_shots_count(&self) -> usize {
        self.shots.len()
    }

    pub fn active_shots(&self) -> &[ActiveShot] {
        &self.shots
    }

    pub fn is_protected(&self, player_id: u32, current_tick: u32) -> bool {
        if let Some(&hit_tick) = self.hit_protection.get(&player_id) {
            let elapsed = current_tick.wrapping_sub(hit_tick);
            elapsed < PROTECTION_TICKS
        } else {
            false
        }
    }

    pub fn score(&self, player_id: u32) -> i32 {
        self.scores.get(&player_id).copied().unwrap_or(0)
    }

    pub fn set_score(&mut self, player_id: u32, score: i32) {
        self.scores.insert(player_id, score);
    }

    /// Drops in-flight shots. Used on level transition: shots carry old-maze
    /// coordinates and must not be evaluated against the new maze geometry.
    pub fn clear_shots(&mut self) {
        self.shots.clear();
    }

    /// Drops in-flight shots belonging to a specific player upon respawn.
    pub fn clear_player_shots(&mut self, player_id: u32) {
        self.shots.retain(|shot| shot.shooter_id != player_id);
    }

    pub fn remove_player(&mut self, player_id: u32) {
        self.last_fire_tick.remove(&player_id);
        self.hit_protection.remove(&player_id);
        self.scores.remove(&player_id);
        // Drop the departed player's in-flight shots so they can neither hit
        // vs a later maze with stale coordinates nor resurrect the pruned
        // score entry when a stale shot later connects.
        self.shots.retain(|shot| shot.shooter_id != player_id);
    }

    pub fn try_fire(
        &mut self,
        shooter_id: u32,
        shooter_pos: Position,
        direction: f32,
        maze: &Maze,
        tick: u32,
    ) -> bool {
        if !shooter_pos.x.is_finite() || !shooter_pos.y.is_finite() || !direction.is_finite() {
            return false;
        }

        if self.shots.len() >= MAX_ACTIVE_SHOTS {
            return false;
        }

        if let Some(&last_tick) = self.last_fire_tick.get(&shooter_id) {
            let elapsed = tick.wrapping_sub(last_tick);
            if elapsed < FIRE_COOLDOWN_TICKS {
                return false;
            }
        }

        let map_x = shooter_pos.x.floor() as i32;
        let map_y = shooter_pos.y.floor() as i32;
        if maze.tile(map_x, map_y) == Tile::Wall {
            return false;
        }

        self.last_fire_tick.insert(shooter_id, tick);

        let shot_id = self.next_shot_id;
        self.next_shot_id = self.next_shot_id.wrapping_add(1).max(1);

        self.shots.push(ActiveShot {
            id: shot_id,
            shooter_id,
            position: shooter_pos,
            direction,
            lifetime_seconds: 0.0,
        });

        let event_id = self.next_event_id();
        self.push_event(CombatEvent::ShotFired {
            event_id,
            shooter_id,
            position: shooter_pos,
            direction,
        });

        true
    }

    pub fn update(
        &mut self,
        elapsed_seconds: f32,
        maze: &Maze,
        players: &mut HashMap<u32, CombatPlayer>,
        tick: u32,
    ) -> Vec<CombatEvent> {
        if !elapsed_seconds.is_finite() || elapsed_seconds <= 0.0 {
            return self.drain_events();
        }
        let dt = elapsed_seconds.min(0.1);

        for (&id, player) in players.iter() {
            self.scores.entry(id).or_insert(player.score);
        }
        // Finding #8: prune departed IDs so stale shots cannot resurrect them
        // via or_insert/insert below.
        self.scores.retain(|id, _| players.contains_key(id));

        let step_distance = SHOT_SPEED * dt;
        let mut surviving_shots = Vec::with_capacity(self.shots.len());
        let mut shots = std::mem::take(&mut self.shots);

        for mut shot in shots.drain(..) {
            let p0 = shot.position;
            let dir_x = shot.direction.cos();
            let dir_y = shot.direction.sin();
            let p1 = Position {
                x: p0.x + dir_x * step_distance,
                y: p0.y + dir_y * step_distance,
            };

            let wall_hit = swept_wall_collision(maze, p0, p1);
            let wall_t = wall_hit.map(|(t, _)| t).unwrap_or(f32::INFINITY);

            let mut best_player_t = f32::INFINITY;
            let mut best_player_id = None;

            for (&id, player) in players.iter() {
                if id == shot.shooter_id {
                    continue;
                }
                if self.is_protected(id, tick) {
                    continue;
                }
                if let Some(t) = swept_circle_collision(p0, p1, player.position, PLAYER_RADIUS)
                    && t < best_player_t
                {
                    best_player_t = t;
                    best_player_id = Some(id);
                }
            }

            if let Some(target_id) = best_player_id
                && best_player_t <= wall_t
                && best_player_t <= 1.0
            {
                // Finding #8: drop stale shots whose shooter/target departed;
                // neither score nor arm hit-protection nor resurrect scores.
                if !players.contains_key(&target_id) || !players.contains_key(&shot.shooter_id) {
                    continue;
                }

                let hit_position = Position {
                    x: p0.x + best_player_t * (p1.x - p0.x),
                    y: p0.y + best_player_t * (p1.y - p0.y),
                };

                if let Some(target) = players.get_mut(&target_id) {
                    target.score -= HIT_PENALTY;
                }
                if let Some(shooter) = players.get_mut(&shot.shooter_id) {
                    shooter.score += HIT_AWARD;
                }

                let target_score = players.get(&target_id).map(|p| p.score).unwrap_or_else(|| {
                    let s = self.scores.entry(target_id).or_insert(0);
                    *s -= HIT_PENALTY;
                    *s
                });
                let shooter_score = players
                    .get(&shot.shooter_id)
                    .map(|p| p.score)
                    .unwrap_or_else(|| {
                        let s = self.scores.entry(shot.shooter_id).or_insert(0);
                        *s += HIT_AWARD;
                        *s
                    });
                self.scores.insert(target_id, target_score);
                self.scores.insert(shot.shooter_id, shooter_score);

                self.hit_protection.insert(target_id, tick);

                let event_id = self.next_event_id();
                self.push_event(CombatEvent::PlayerHit {
                    event_id,
                    shooter_id: shot.shooter_id,
                    target_id,
                    hit_position,
                    shooter_score,
                    target_score,
                });
            } else if wall_t <= 1.0 {
                // Stopped at wall
            } else {
                shot.position = p1;
                shot.lifetime_seconds += dt;
                if shot.lifetime_seconds < MAX_SHOT_LIFETIME_SECONDS {
                    surviving_shots.push(shot);
                }
            }
        }

        self.shots = surviving_shots;

        self.hit_protection
            .retain(|_, &mut hit_tick| tick.wrapping_sub(hit_tick) < PROTECTION_TICKS + 60);
        self.last_fire_tick
            .retain(|_, &mut fire_tick| tick.wrapping_sub(fire_tick) < FIRE_COOLDOWN_TICKS + 60);

        self.drain_events()
    }

    pub fn drain_events(&mut self) -> Vec<CombatEvent> {
        std::mem::take(&mut self.pending_events)
    }

    fn next_event_id(&mut self) -> u32 {
        let id = self.next_event_id;
        self.next_event_id = self.next_event_id.wrapping_add(1).max(1);
        id
    }

    fn push_event(&mut self, event: CombatEvent) {
        if self.pending_events.len() >= MAX_PENDING_EVENTS {
            self.pending_events.remove(0);
        }
        self.pending_events.push(event);
    }
}

pub fn swept_wall_collision(
    maze: &Maze,
    start: Position,
    end: Position,
) -> Option<(f32, Position)> {
    let mut map_x = start.x.floor() as i32;
    let mut map_y = start.y.floor() as i32;

    if maze.tile(map_x, map_y) == Tile::Wall {
        return Some((0.0, start));
    }

    let dx = end.x - start.x;
    let dy = end.y - start.y;

    if dx.abs() < 1e-9 && dy.abs() < 1e-9 {
        return None;
    }

    let (step_x, mut t_max_x, t_delta_x) = if dx > 0.0 {
        (1, (map_x as f32 + 1.0 - start.x) / dx, 1.0 / dx)
    } else if dx < 0.0 {
        (-1, (start.x - map_x as f32) / (-dx), 1.0 / (-dx))
    } else {
        (0, f32::INFINITY, f32::INFINITY)
    };

    let (step_y, mut t_max_y, t_delta_y) = if dy > 0.0 {
        (1, (map_y as f32 + 1.0 - start.y) / dy, 1.0 / dy)
    } else if dy < 0.0 {
        (-1, (start.y - map_y as f32) / (-dy), 1.0 / (-dy))
    } else {
        (0, f32::INFINITY, f32::INFINITY)
    };

    let max_steps = (maze.width() + maze.height()) * 2 + 10;
    for _ in 0..max_steps {
        let (t_hit, next_x, next_y) = if (t_max_x - t_max_y).abs() < 1e-7 {
            let t = t_max_x;
            t_max_x += t_delta_x;
            t_max_y += t_delta_y;
            map_x += step_x;
            map_y += step_y;
            (t, map_x, map_y)
        } else if t_max_x < t_max_y {
            let t = t_max_x;
            t_max_x += t_delta_x;
            map_x += step_x;
            (t, map_x, map_y)
        } else {
            let t = t_max_y;
            t_max_y += t_delta_y;
            map_y += step_y;
            (t, map_x, map_y)
        };

        if t_hit > 1.0 {
            break;
        }

        if maze.tile(next_x, next_y) == Tile::Wall
            || (step_x != 0
                && step_y != 0
                && (maze.tile(next_x - step_x, next_y) == Tile::Wall
                    && maze.tile(next_x, next_y - step_y) == Tile::Wall))
        {
            let hit_pos = Position {
                x: start.x + t_hit * dx,
                y: start.y + t_hit * dy,
            };
            return Some((t_hit, hit_pos));
        }
    }

    None
}

pub fn swept_circle_collision(
    start: Position,
    end: Position,
    center: Position,
    radius: f32,
) -> Option<f32> {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let delta_x = start.x - center.x;
    let delta_y = start.y - center.y;

    let r2 = radius * radius;
    let c = delta_x * delta_x + delta_y * delta_y - r2;
    if c <= 0.0 {
        return Some(0.0);
    }

    let a = dx * dx + dy * dy;
    if a < 1e-9 {
        return None;
    }

    let b = 2.0 * (dx * delta_x + dy * delta_y);
    if b >= 0.0 {
        return None;
    }

    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return None;
    }

    let t = (-b - disc.sqrt()) / (2.0 * a);
    if (0.0..=1.0).contains(&t) {
        Some(t)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clear_straight_shot_hits_the_nearest_opponent() {
        let maze = Maze::level_one();
        let mut combat = CombatManager::new();
        let mut players = HashMap::new();
        players.insert(1, CombatPlayer::new(Position { x: 1.5, y: 1.5 }, 0));
        players.insert(2, CombatPlayer::new(Position { x: 4.5, y: 1.5 }, 0));

        assert!(combat.try_fire(1, Position { x: 1.5, y: 1.5 }, 0.0, &maze, 0));

        let mut hit_detected = false;
        for tick in 1..=10 {
            let events = combat.update(1.0 / 60.0, &maze, &mut players, tick);
            for event in events {
                if let CombatEvent::PlayerHit {
                    shooter_id,
                    target_id,
                    ..
                } = event
                {
                    assert_eq!(shooter_id, 1);
                    assert_eq!(target_id, 2);
                    hit_detected = true;
                }
            }
            if hit_detected {
                break;
            }
        }

        assert!(hit_detected, "expected a player hit event");
        assert_eq!(players[&1].score, 10);
        assert_eq!(players[&2].score, -5);
        assert_eq!(combat.active_shots_count(), 0);
    }

    #[test]
    fn a_wall_blocks_a_shot() {
        let maze = Maze::level_one();
        let mut combat = CombatManager::new();
        let mut players = HashMap::new();
        // Row 3 (y=3.5) has a wall at x=6 between x=1.5 and x=7.5
        players.insert(1, CombatPlayer::new(Position { x: 1.5, y: 3.5 }, 0));
        players.insert(2, CombatPlayer::new(Position { x: 7.5, y: 3.5 }, 0));

        assert!(combat.try_fire(1, Position { x: 1.5, y: 3.5 }, 0.0, &maze, 0));

        for tick in 1..=20 {
            let events = combat.update(1.0 / 60.0, &maze, &mut players, tick);
            for event in events {
                assert!(!matches!(event, CombatEvent::PlayerHit { .. }));
            }
        }

        assert_eq!(players[&2].score, 0);
        assert_eq!(players[&1].score, 0);
        assert_eq!(combat.active_shots_count(), 0);
    }

    #[test]
    fn a_shot_cannot_hit_its_shooter() {
        let maze = Maze::level_one();
        let mut combat = CombatManager::new();
        let mut players = HashMap::new();
        players.insert(1, CombatPlayer::new(Position { x: 1.5, y: 1.5 }, 0));

        assert!(combat.try_fire(1, Position { x: 1.5, y: 1.5 }, 0.0, &maze, 0));

        for tick in 1..=10 {
            let events = combat.update(1.0 / 60.0, &maze, &mut players, tick);
            for event in events {
                if let CombatEvent::PlayerHit { target_id, .. } = event {
                    panic!("shooter was hit by own shot: target={target_id}");
                }
            }
        }

        assert_eq!(players[&1].score, 0);
    }

    #[test]
    fn only_the_first_target_in_a_corridor_is_hit() {
        let maze = Maze::level_one();
        let mut combat = CombatManager::new();
        let mut players = HashMap::new();
        players.insert(1, CombatPlayer::new(Position { x: 1.5, y: 1.5 }, 0));
        players.insert(2, CombatPlayer::new(Position { x: 3.5, y: 1.5 }, 0));
        players.insert(3, CombatPlayer::new(Position { x: 5.5, y: 1.5 }, 0));

        assert!(combat.try_fire(1, Position { x: 1.5, y: 1.5 }, 0.0, &maze, 0));

        let mut hit_targets = Vec::new();
        for tick in 1..=15 {
            let events = combat.update(1.0 / 60.0, &maze, &mut players, tick);
            for event in events {
                if let CombatEvent::PlayerHit { target_id, .. } = event {
                    hit_targets.push(target_id);
                }
            }
        }

        assert_eq!(hit_targets, vec![2]);
        assert_eq!(players[&1].score, 10);
        assert_eq!(players[&2].score, -5);
        assert_eq!(players[&3].score, 0);
        assert_eq!(combat.active_shots_count(), 0);
    }

    #[test]
    fn score_changes_are_exactly_plus_10_minus_5_and_may_go_negative() {
        let maze = Maze::level_one();
        let mut combat = CombatManager::new();
        let mut players = HashMap::new();
        players.insert(1, CombatPlayer::new(Position { x: 1.5, y: 1.5 }, 0));
        players.insert(2, CombatPlayer::new(Position { x: 3.5, y: 1.5 }, 0));

        assert!(combat.try_fire(1, Position { x: 1.5, y: 1.5 }, 0.0, &maze, 0));

        for tick in 1..=10 {
            combat.update(1.0 / 60.0, &maze, &mut players, tick);
        }

        assert_eq!(players[&1].score, 10);
        assert_eq!(players[&2].score, -5);

        // After protection expires (120 ticks), target can be hit again into more negative territory
        assert!(combat.try_fire(1, Position { x: 1.5, y: 1.5 }, 0.0, &maze, 130));

        for tick in 131..=140 {
            combat.update(1.0 / 60.0, &maze, &mut players, tick);
        }

        assert_eq!(players[&1].score, 20);
        assert_eq!(players[&2].score, -10);
    }

    #[test]
    fn repeated_fire_during_targets_two_second_protection_does_not_score() {
        let maze = Maze::level_one();
        let mut combat = CombatManager::new();
        let mut players = HashMap::new();
        players.insert(1, CombatPlayer::new(Position { x: 1.5, y: 1.5 }, 0));
        players.insert(2, CombatPlayer::new(Position { x: 3.5, y: 1.5 }, 0));

        assert!(combat.try_fire(1, Position { x: 1.5, y: 1.5 }, 0.0, &maze, 0));
        for tick in 1..=10 {
            combat.update(1.0 / 60.0, &maze, &mut players, tick);
        }
        assert_eq!(players[&1].score, 10);
        assert_eq!(players[&2].score, -5);
        assert!(combat.is_protected(2, 10));

        // Fire again at tick 20 (rate limit allowed, but target is protected)
        assert!(combat.try_fire(1, Position { x: 1.5, y: 1.5 }, 0.0, &maze, 20));
        for tick in 21..=30 {
            let events = combat.update(1.0 / 60.0, &maze, &mut players, tick);
            for event in events {
                assert!(!matches!(event, CombatEvent::PlayerHit { .. }));
            }
        }

        // Scores unchanged
        assert_eq!(players[&1].score, 10);
        assert_eq!(players[&2].score, -5);
    }

    #[test]
    fn target_becomes_hittable_after_protection_expires() {
        let maze = Maze::level_one();
        let mut combat = CombatManager::new();
        let mut players = HashMap::new();
        players.insert(1, CombatPlayer::new(Position { x: 1.5, y: 1.5 }, 0));
        players.insert(2, CombatPlayer::new(Position { x: 3.5, y: 1.5 }, 0));

        assert!(combat.try_fire(1, Position { x: 1.5, y: 1.5 }, 0.0, &maze, 0));
        let mut first_hit_tick = 0;
        for tick in 1..=10 {
            let events = combat.update(1.0 / 60.0, &maze, &mut players, tick);
            if events
                .iter()
                .any(|e| matches!(e, CombatEvent::PlayerHit { .. }))
            {
                first_hit_tick = tick;
                break;
            }
        }
        assert!(first_hit_tick > 0);
        assert!(combat.is_protected(2, first_hit_tick));
        assert!(combat.is_protected(2, first_hit_tick + PROTECTION_TICKS - 1));
        assert!(!combat.is_protected(2, first_hit_tick + PROTECTION_TICKS));

        let second_fire_tick = first_hit_tick + PROTECTION_TICKS;
        assert!(combat.try_fire(1, Position { x: 1.5, y: 1.5 }, 0.0, &maze, second_fire_tick));

        let mut second_hit_detected = false;
        for tick in (second_fire_tick + 1)..=(second_fire_tick + 10) {
            let events = combat.update(1.0 / 60.0, &maze, &mut players, tick);
            if events
                .iter()
                .any(|e| matches!(e, CombatEvent::PlayerHit { .. }))
            {
                second_hit_detected = true;
                break;
            }
        }

        assert!(second_hit_detected);
        assert_eq!(players[&1].score, 20);
        assert_eq!(players[&2].score, -10);
    }

    #[test]
    fn rate_limit_stops_spam_firing() {
        let maze = Maze::level_one();
        let mut combat = CombatManager::new();

        assert!(combat.try_fire(1, Position { x: 1.5, y: 1.5 }, 0.0, &maze, 0));

        for tick in 1..FIRE_COOLDOWN_TICKS {
            assert!(!combat.try_fire(1, Position { x: 1.5, y: 1.5 }, 0.0, &maze, tick));
        }

        assert!(combat.try_fire(
            1,
            Position { x: 1.5, y: 1.5 },
            0.0,
            &maze,
            FIRE_COOLDOWN_TICKS
        ));
        assert!(!combat.try_fire(
            1,
            Position { x: 1.5, y: 1.5 },
            0.0,
            &maze,
            FIRE_COOLDOWN_TICKS + 1
        ));

        // Different player can fire independently
        assert!(combat.try_fire(
            2,
            Position { x: 3.5, y: 1.5 },
            0.0,
            &maze,
            FIRE_COOLDOWN_TICKS + 1
        ));
    }

    #[test]
    fn shot_memory_stays_bounded() {
        let maze = Maze::level_one();
        let mut combat = CombatManager::new();

        for player_id in 1..=MAX_ACTIVE_SHOTS as u32 {
            assert!(
                combat.try_fire(player_id, Position { x: 1.5, y: 1.5 }, 0.0, &maze, 0),
                "player {player_id} should be able to fire"
            );
        }
        assert_eq!(combat.active_shots_count(), MAX_ACTIVE_SHOTS);

        // 33rd shot should be rejected
        assert!(!combat.try_fire(99, Position { x: 1.5, y: 1.5 }, 0.0, &maze, 0));
        assert_eq!(combat.active_shots_count(), MAX_ACTIVE_SHOTS);

        // Advance time past wall collision / max lifetime
        let mut players = HashMap::new();
        for tick in 1..=150 {
            combat.update(1.0 / 60.0, &maze, &mut players, tick);
        }

        assert_eq!(combat.active_shots_count(), 0);
    }

    #[test]
    fn swept_segment_prevents_tunneling_high_speed() {
        let maze = Maze::level_one();
        let mut combat = CombatManager::new();
        let mut players = HashMap::new();
        players.insert(1, CombatPlayer::new(Position { x: 1.5, y: 1.5 }, 0));
        players.insert(2, CombatPlayer::new(Position { x: 3.5, y: 1.5 }, 0));

        assert!(combat.try_fire(1, Position { x: 1.5, y: 1.5 }, 0.0, &maze, 0));

        // In a single large time step (0.1s -> 2.5 tiles), the bullet jumps from 1.5 to 4.0,
        // which completely encompasses the opponent at 3.5. Swept collision must catch it!
        let events = combat.update(0.1, &maze, &mut players, 1);
        let hit = events
            .iter()
            .any(|e| matches!(e, CombatEvent::PlayerHit { target_id: 2, .. }));
        assert!(
            hit,
            "swept collision must detect hit even across large step"
        );
        assert_eq!(players[&1].score, 10);
        assert_eq!(players[&2].score, -5);
    }
}
