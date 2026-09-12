use crate::server::combat::{swept_circle_collision, swept_wall_collision};
use crate::world::{Maze, Position, Tile};

pub const ENEMY_RADIUS: f32 = 0.22;
pub const ENEMY_SPEED: f32 = 1.3;
pub const PLAYER_COLLISION_RADIUS: f32 = 0.2;
const MAX_MAZE_DIM: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Enemy {
    pub id: u32,
    pub position: Position,
    pub direction: f32,
    pub radius: f32,
    pub speed: f32,
    pub alive: bool,
    pub spawn_position: Position,
}

impl Enemy {
    pub fn new(id: u32, spawn_position: Position) -> Self {
        Self {
            id,
            position: spawn_position,
            direction: 0.0,
            radius: ENEMY_RADIUS,
            speed: ENEMY_SPEED,
            alive: true,
            spawn_position,
        }
    }

    pub fn is_alive(&self) -> bool {
        self.alive
    }
}

#[derive(Debug, Clone)]
pub struct EnemyManager {
    enemies: Vec<Enemy>,
    enabled: bool,
    player_lost: bool,
    current_level: u8,
}

impl Default for EnemyManager {
    fn default() -> Self {
        Self::new()
    }
}

impl EnemyManager {
    pub fn new() -> Self {
        Self {
            enemies: Vec::new(),
            enabled: false,
            player_lost: false,
            current_level: 1,
        }
    }

    pub fn for_maze(maze: &Maze, player_pos: Option<Position>) -> Self {
        let mut manager = Self::new();
        manager.spawn_enemies_for_maze(maze, player_pos);
        manager
    }

    pub fn current_level(&self) -> u8 {
        self.current_level
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn toggle_enabled(&mut self) -> bool {
        self.enabled = !self.enabled;
        self.enabled
    }

    pub fn has_player_lost(&self) -> bool {
        self.player_lost
    }

    pub fn clear_player_lost(&mut self) {
        self.player_lost = false;
    }

    pub fn total_enemy_count(&self) -> usize {
        self.enemies.len()
    }

    pub fn active_enemy_count(&self) -> usize {
        if !self.enabled {
            return 0;
        }
        self.enemies.iter().filter(|e| e.alive).count()
    }

    pub fn is_cleared(&self) -> bool {
        self.enabled && !self.enemies.is_empty() && self.enemies.iter().all(|e| !e.alive)
    }

    pub fn enemies(&self) -> &[Enemy] {
        &self.enemies
    }

    pub fn enemies_mut(&mut self) -> &mut [Enemy] {
        &mut self.enemies
    }

    pub fn reset(&mut self, maze: &Maze, player_pos: Option<Position>) {
        self.player_lost = false;
        self.spawn_enemies_for_maze(maze, player_pos);
    }

    pub fn spawn_enemies_for_maze(&mut self, maze: &Maze, player_pos: Option<Position>) {
        self.enemies.clear();
        self.player_lost = false;
        self.current_level = maze.level();

        let spawns = maze.spawns();
        let mut enemy_id = 1;

        for (i, &spawn) in spawns.iter().enumerate() {
            if i == 0 && spawns.len() > 1 {
                continue;
            }

            if let Some(ppos) = player_pos {
                let dx = spawn.x - ppos.x;
                let dy = spawn.y - ppos.y;
                if (dx * dx + dy * dy).sqrt() < 3.0 {
                    continue;
                }
            }

            if maze.can_occupy(spawn, ENEMY_RADIUS) {
                self.enemies.push(Enemy::new(enemy_id, spawn));
                enemy_id += 1;
            }
        }

        if self.enemies.is_empty() {
            for y in 1..maze.height() as i32 - 1 {
                for x in 1..maze.width() as i32 - 1 {
                    if maze.tile(x, y) == Tile::Floor {
                        let pos = Position {
                            x: x as f32 + 0.5,
                            y: y as f32 + 0.5,
                        };
                        if let Some(ppos) = player_pos {
                            let dx = pos.x - ppos.x;
                            let dy = pos.y - ppos.y;
                            if (dx * dx + dy * dy).sqrt() < 3.0 {
                                continue;
                            }
                        }
                        if maze.can_occupy(pos, ENEMY_RADIUS) {
                            self.enemies.push(Enemy::new(enemy_id, pos));
                            enemy_id += 1;
                            if self.enemies.len() >= 3 {
                                break;
                            }
                        }
                    }
                }
                if self.enemies.len() >= 3 {
                    break;
                }
            }
        }
    }

    pub fn update(&mut self, dt: f32, maze: &Maze, player_pos: Position) -> bool {
        if !self.enabled || !dt.is_finite() || dt <= 0.0 {
            return self.player_lost;
        }

        let dt = dt.min(0.1);

        for enemy in &mut self.enemies {
            if !enemy.alive {
                continue;
            }

            let target = find_bfs_target(maze, enemy.position, player_pos);
            enemy.direction = step_towards(
                &mut enemy.position,
                target,
                enemy.speed,
                enemy.radius,
                dt,
                maze,
            );
        }

        apply_enemy_separation(&mut self.enemies, maze);

        for enemy in &self.enemies {
            if !enemy.alive {
                continue;
            }
            let dx = enemy.position.x - player_pos.x;
            let dy = enemy.position.y - player_pos.y;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist < enemy.radius + PLAYER_COLLISION_RADIUS {
                self.player_lost = true;
                return true;
            }
        }

        self.player_lost
    }

    pub fn check_shot_hit(&mut self, start: Position, end: Position) -> Option<(u32, Position)> {
        if !self.enabled {
            return None;
        }

        let mut best_t = f32::INFINITY;
        let mut hit_index = None;

        for (i, enemy) in self.enemies.iter().enumerate() {
            if !enemy.alive {
                continue;
            }
            if let Some(t) = swept_circle_collision(start, end, enemy.position, enemy.radius)
                && t < best_t
            {
                best_t = t;
                hit_index = Some(i);
            }
        }

        if let Some(index) = hit_index {
            self.enemies[index].alive = false;
            let hit_pos = Position {
                x: start.x + best_t * (end.x - start.x),
                y: start.y + best_t * (end.y - start.y),
            };
            Some((self.enemies[index].id, hit_pos))
        } else {
            None
        }
    }
}

pub fn find_bfs_target(maze: &Maze, start: Position, goal: Position) -> Position {
    let w = maze.width().min(MAX_MAZE_DIM);
    let h = maze.height().min(MAX_MAZE_DIM);

    let start_x = (start.x.floor() as i32).clamp(0, w as i32 - 1) as usize;
    let start_y = (start.y.floor() as i32).clamp(0, h as i32 - 1) as usize;
    let goal_x = (goal.x.floor() as i32).clamp(0, w as i32 - 1) as usize;
    let goal_y = (goal.y.floor() as i32).clamp(0, h as i32 - 1) as usize;

    if start_x == goal_x && start_y == goal_y {
        return goal;
    }

    if swept_wall_collision(maze, start, goal).is_none() {
        return goal;
    }

    let mut dist = [[u16::MAX; MAX_MAZE_DIM]; MAX_MAZE_DIM];
    let mut queue = [(0_usize, 0_usize); MAX_MAZE_DIM * MAX_MAZE_DIM];
    let mut head = 0;
    let mut tail = 0;

    dist[goal_x][goal_y] = 0;
    queue[tail] = (goal_x, goal_y);
    tail += 1;

    let neighbors: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

    while head < tail {
        let (cx, cy) = queue[head];
        head += 1;
        let d = dist[cx][cy];

        for (dx, dy) in neighbors {
            let nx = cx as i32 + dx;
            let ny = cy as i32 + dy;
            if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
                let unx = nx as usize;
                let uny = ny as usize;
                if maze.tile(nx, ny) != Tile::Wall && dist[unx][uny] == u16::MAX {
                    dist[unx][uny] = d.saturating_add(1);
                    queue[tail] = (unx, uny);
                    tail += 1;
                }
            }
        }
    }

    let mut best_dist = dist[start_x][start_y];
    let mut best_step = (start_x, start_y);

    for (dx, dy) in neighbors {
        let nx = start_x as i32 + dx;
        let ny = start_y as i32 + dy;
        if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
            let unx = nx as usize;
            let uny = ny as usize;
            if dist[unx][uny] < best_dist {
                best_dist = dist[unx][uny];
                best_step = (unx, uny);
            }
        }
    }

    Position {
        x: best_step.0 as f32 + 0.5,
        y: best_step.1 as f32 + 0.5,
    }
}

pub fn step_towards(
    pos: &mut Position,
    target: Position,
    speed: f32,
    radius: f32,
    dt: f32,
    maze: &Maze,
) -> f32 {
    let dx = target.x - pos.x;
    let dy = target.y - pos.y;
    let dist = (dx * dx + dy * dy).sqrt();
    if dist < 1e-4 {
        return 0.0;
    }

    let dir_x = dx / dist;
    let dir_y = dy / dist;
    let direction = dir_y.atan2(dir_x);

    let step = (speed * dt).min(dist);
    let move_x = dir_x * step;
    let move_y = dir_y * step;

    let next_full = Position {
        x: pos.x + move_x,
        y: pos.y + move_y,
    };
    if maze.can_occupy(next_full, radius) {
        *pos = next_full;
        return direction;
    }

    let next_x = Position {
        x: pos.x + move_x,
        y: pos.y,
    };
    if maze.can_occupy(next_x, radius) {
        pos.x = next_x.x;
    }

    let next_y = Position {
        x: pos.x,
        y: pos.y + move_y,
    };
    if maze.can_occupy(next_y, radius) {
        pos.y = next_y.y;
    }

    direction
}

fn apply_enemy_separation(enemies: &mut [Enemy], maze: &Maze) {
    let len = enemies.len();
    for i in 0..len {
        if !enemies[i].alive {
            continue;
        }
        for j in (i + 1)..len {
            if !enemies[j].alive {
                continue;
            }
            let dx = enemies[j].position.x - enemies[i].position.x;
            let dy = enemies[j].position.y - enemies[i].position.y;
            let dist_sq = dx * dx + dy * dy;
            let min_dist = enemies[i].radius + enemies[j].radius;
            if dist_sq < min_dist * min_dist && dist_sq > 1e-6 {
                let dist = dist_sq.sqrt();
                let overlap = 0.5 * (min_dist - dist);
                let push_x = (dx / dist) * overlap;
                let push_y = (dy / dist) * overlap;

                let p1 = Position {
                    x: enemies[i].position.x - push_x,
                    y: enemies[i].position.y - push_y,
                };
                if maze.can_occupy(p1, enemies[i].radius) {
                    enemies[i].position = p1;
                }

                let p2 = Position {
                    x: enemies[j].position.x + push_x,
                    y: enemies[j].position.y + push_y,
                };
                if maze.can_occupy(p2, enemies[j].radius) {
                    enemies[j].position = p2;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enemy_spawning_valid_walkable_locations() {
        for level in 1..=3 {
            let maze = Maze::for_level(level).unwrap();
            let player_pos = maze.spawn();
            let manager = EnemyManager::for_maze(&maze, Some(player_pos));

            assert!(!manager.enemies().is_empty(), "level {level} must spawn enemies");
            for enemy in manager.enemies() {
                assert!(
                    maze.can_occupy(enemy.position, enemy.radius),
                    "enemy spawn {:?} must be walkable in level {level}",
                    enemy.position
                );
                let tile = maze.tile(
                    enemy.position.x.floor() as i32,
                    enemy.position.y.floor() as i32,
                );
                assert_ne!(tile, Tile::Wall, "enemy cannot spawn inside a wall");
                assert!(enemy.alive, "enemy must spawn alive");
            }
        }
    }

    #[test]
    fn test_bfs_pathfinding_and_wall_sliding() {
        let maze = Maze::level_one();
        let start = Position { x: 1.5, y: 1.5 };
        let goal = Position { x: 1.5, y: 3.5 };

        let target = find_bfs_target(&maze, start, goal);
        assert!(target.x > 0.0 && target.y > 0.0);

        let mut pos = start;
        let dir = step_towards(&mut pos, target, 1.0, ENEMY_RADIUS, 0.1, &maze);
        assert!(dir.is_finite());
        assert!(maze.can_occupy(pos, ENEMY_RADIUS));
        assert_ne!(pos, start);

        let mut wall_pos = Position { x: 1.25, y: 1.5 };
        let wall_target = Position { x: 0.5, y: 1.5 };
        let _ = step_towards(&mut wall_pos, wall_target, 1.0, ENEMY_RADIUS, 0.1, &maze);
        assert!(
            maze.can_occupy(wall_pos, ENEMY_RADIUS),
            "movement must not penetrate walls"
        );
    }

    #[test]
    fn test_enemy_collision_touching_player() {
        let maze = Maze::level_one();
        let mut manager = EnemyManager::new();
        manager.set_enabled(true);
        manager.enemies.push(Enemy::new(1, Position { x: 1.5, y: 1.5 }));

        let player_pos = Position { x: 1.5, y: 1.5 };
        let lost = manager.update(0.016, &maze, player_pos);
        assert!(lost, "alive enemy touching player must trigger player loss");
        assert!(manager.has_player_lost());

        manager.clear_player_lost();
        assert!(!manager.has_player_lost());

        manager.enemies[0].alive = false;
        let lost_dead = manager.update(0.016, &maze, player_pos);
        assert!(!lost_dead, "dead enemy must not trigger player loss");

        manager.enemies[0].alive = true;
        manager.enemies[0].position = Position { x: 5.5, y: 1.5 };
        let lost_far = manager.update(0.016, &maze, player_pos);
        assert!(!lost_far, "far enemy must not trigger player loss");
    }

    #[test]
    fn test_shot_swept_collision_killing_enemies_without_respawn() {
        let maze = Maze::level_one();
        let mut manager = EnemyManager::new();
        manager.set_enabled(true);
        manager.enemies.push(Enemy::new(1, Position { x: 5.0, y: 1.5 }));
        manager.enemies.push(Enemy::new(2, Position { x: 8.0, y: 1.5 }));

        let start = Position { x: 1.5, y: 1.5 };
        let end = Position { x: 6.0, y: 1.5 };

        let hit = manager.check_shot_hit(start, end);
        assert!(hit.is_some(), "shot swept collision must hit enemy 1");
        let (id1, pos1) = hit.unwrap();
        assert_eq!(id1, 1, "shot swept collision must hit enemy 1");
        assert!(pos1.x > start.x && pos1.x <= 5.0, "hit pos must be before or at enemy center");
        assert_eq!(pos1.y, 1.5);
        assert!(!manager.enemies()[0].alive, "hit enemy must not be alive");
        assert!(manager.enemies()[1].alive, "unhit enemy 2 must still be alive");
        assert!(!manager.is_cleared(), "maze not cleared yet");

        let hit_again = manager.check_shot_hit(start, end);
        assert_eq!(
            hit_again, None,
            "dead enemy must not be hit again or respawn"
        );

        let end2 = Position { x: 10.0, y: 1.5 };
        let hit2 = manager.check_shot_hit(start, end2);
        assert!(hit2.is_some(), "second shot should hit enemy 2");
        let (id2, pos2) = hit2.unwrap();
        assert_eq!(id2, 2, "second shot should hit enemy 2");
        assert!(pos2.x > 5.0 && pos2.x <= 8.0);
        assert!(manager.is_cleared(), "all enemies dead -> maze is cleared");

        manager.update(0.1, &maze, start);
        assert!(!manager.enemies()[0].alive);
        assert!(!manager.enemies()[1].alive);
    }

    #[test]
    fn test_midgame_toggle_shortcut_enabling_disabling() {
        let maze = Maze::level_one();
        let mut manager = EnemyManager::for_maze(&maze, None);
        assert!(!manager.is_enabled(), "enemies must start disabled by default");
        assert_eq!(manager.active_enemy_count(), 0);

        let toggled = manager.toggle_enabled();
        assert!(toggled);
        assert!(manager.is_enabled());
        let count_active = manager.active_enemy_count();
        assert!(count_active > 0);

        let toggled_off = manager.toggle_enabled();
        assert!(!toggled_off);
        assert!(!manager.is_enabled());
        assert_eq!(manager.active_enemy_count(), 0);

        let start = Position { x: 0.0, y: 0.0 };
        let end = Position { x: 20.0, y: 20.0 };
        assert_eq!(manager.check_shot_hit(start, end), None);

        let toggled_back = manager.toggle_enabled();
        assert!(toggled_back);
        assert!(manager.is_enabled());
        assert_eq!(manager.active_enemy_count(), count_active);
    }

    #[test]
    fn test_enemy_reset() {
        let maze = Maze::level_one();
        let mut manager = EnemyManager::for_maze(&maze, None);
        manager.set_enabled(true);
        for e in manager.enemies_mut() {
            e.alive = false;
        }
        assert!(manager.is_cleared());

        manager.reset(&maze, None);
        assert!(!manager.is_cleared());
        assert!(manager.active_enemy_count() > 0);
        assert!(!manager.has_player_lost());
    }

    #[test]
    fn test_level_transition_preserves_enabled_flag() {
        let maze1 = Maze::level_one();
        let maze2 = Maze::for_level(2).unwrap();

        // When disabled, remains disabled after spawning for new maze
        let mut manager_disabled = EnemyManager::for_maze(&maze1, None);
        assert!(!manager_disabled.is_enabled());
        manager_disabled.spawn_enemies_for_maze(&maze2, None);
        assert!(!manager_disabled.is_enabled());
        assert_eq!(manager_disabled.active_enemy_count(), 0);

        // When enabled, remains enabled after spawning for new maze
        let mut manager_enabled = EnemyManager::for_maze(&maze1, None);
        manager_enabled.set_enabled(true);
        manager_enabled.spawn_enemies_for_maze(&maze2, None);
        assert!(manager_enabled.is_enabled());
        assert!(manager_enabled.active_enemy_count() > 0);
    }
}
