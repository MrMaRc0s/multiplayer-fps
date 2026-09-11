const LEVEL_ONE: [&str; 11] = [
    "###############",
    "#.............#",
    "#.#####.#####.#",
    "#.....#.#.....#",
    "#####.#.#.#####",
    "#.....#.#.....#",
    "#.#####.#####.#",
    "#.............#",
    "#.###.#####.#.#",
    "#.....#......E#",
    "###############",
];

const LEVEL_TWO: [&str; 15] = [
    "#####################",
    "#.#...#.............#",
    "#.#.#.#.####..###.#E#",
    "#.#.#.#.#.........#.#",
    "#.#.#.###.###.#.#.#.#",
    "#...#.#...#.....#.#.#",
    "#####.#.#######.#...#",
    "#...#.#.....#...#.#.#",
    "#.###.##.##.#.###.#.#",
    "#.....#...#...#...#.#",
    "#.#.###...#####.###.#",
    "#...#...#...#.......#",
    "#.#.#.#####...###.#.#",
    "#.....#.......#.....#",
    "#####################",
];

const LEVEL_THREE: [&str; 19] = [
    "#########################",
    "#.#...#E#...#.......#...#",
    "#.#.#.#.#.#.#.###.#.###.#",
    "#.#.#.#...#.....#.#.....#",
    "#.#.#.#.#######.#.#####.#",
    "#...#.#.#.....#.#.#.....#",
    "#####.###.###.###.#####.#",
    "#...#.#...#.....#.......#",
    "#.###.#.#.#####.#####.###",
    "#.#...#.#.#...#.....#...#",
    "#.#.###.###...#####.#.#.#",
    "#...#.....#.#.#.....#.#.#",
    "#.#####.#.#.#.#.#######.#",
    "#.....#.#...#.#.#.......#",
    "#####.###.###.#.###.###.#",
    "#...#...#.#...#.....#.#.#",
    "#..####.###.###.#####.#.#",
    "#...........#...........#",
    "#########################",
];

const SPAWNS_ONE: [Position; 5] = [
    Position { x: 1.5, y: 1.5 },
    Position { x: 13.5, y: 1.5 },
    Position { x: 1.5, y: 7.5 },
    Position { x: 7.5, y: 7.5 },
    Position { x: 13.5, y: 7.5 },
];

const SPAWNS_TWO: [Position; 6] = [
    Position { x: 1.5, y: 13.5 },
    Position { x: 9.5, y: 13.5 },
    Position { x: 19.5, y: 13.5 },
    Position { x: 13.5, y: 1.5 },
    Position { x: 1.5, y: 5.5 },
    Position { x: 15.5, y: 7.5 },
];

const SPAWNS_THREE: [Position; 8] = [
    Position { x: 1.5, y: 17.5 },
    Position { x: 9.5, y: 17.5 },
    Position { x: 17.5, y: 17.5 },
    Position { x: 23.5, y: 17.5 },
    Position { x: 1.5, y: 5.5 },
    Position { x: 23.5, y: 5.5 },
    Position { x: 9.5, y: 7.5 },
    Position { x: 17.5, y: 7.5 },
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tile {
    Floor,
    Wall,
    Exit,
}

#[derive(Debug, Clone, Copy)]
pub struct Maze {
    level: u8,
    rows: &'static [&'static str],
    spawns: &'static [Position],
    exit: Position,
}

impl Maze {
    pub fn level_one() -> Self {
        Self::for_level(1).expect("level 1 must be defined")
    }

    pub fn for_level(level: u8) -> Option<Self> {
        match level {
            1 => Some(Self {
                level: 1,
                rows: &LEVEL_ONE,
                spawns: &SPAWNS_ONE,
                exit: Position { x: 13.5, y: 9.5 },
            }),
            2 => Some(Self {
                level: 2,
                rows: &LEVEL_TWO,
                spawns: &SPAWNS_TWO,
                exit: Position { x: 19.5, y: 2.5 },
            }),
            3 => Some(Self {
                level: 3,
                rows: &LEVEL_THREE,
                spawns: &SPAWNS_THREE,
                exit: Position { x: 7.5, y: 1.5 },
            }),
            _ => None,
        }
    }

    pub fn level(&self) -> u8 {
        self.level
    }

    pub fn width(&self) -> usize {
        self.rows[0].len()
    }

    pub fn height(&self) -> usize {
        self.rows.len()
    }

    pub fn spawns(&self) -> &'static [Position] {
        self.spawns
    }

    pub fn spawn(&self) -> Position {
        self.spawns[0]
    }

    pub fn spawn_at(&self, index: usize) -> Position {
        self.spawns[index % self.spawns.len()]
    }

    pub fn exit(&self) -> Position {
        self.exit
    }

    pub fn is_exit(&self, x: i32, y: i32) -> bool {
        self.tile(x, y) == Tile::Exit
    }

    pub fn tile(&self, x: i32, y: i32) -> Tile {
        if x < 0 || y < 0 || x as usize >= self.width() || y as usize >= self.height() {
            return Tile::Wall;
        }

        match self.rows[y as usize].as_bytes()[x as usize] {
            b'#' => Tile::Wall,
            b'E' => Tile::Exit,
            _ => Tile::Floor,
        }
    }

    pub fn can_occupy(&self, position: Position, radius: f32) -> bool {
        if !position.x.is_finite() || !position.y.is_finite() || radius < 0.0 {
            return false;
        }

        let corners = [
            (position.x - radius, position.y - radius),
            (position.x + radius, position.y - radius),
            (position.x - radius, position.y + radius),
            (position.x + radius, position.y + radius),
        ];

        corners
            .iter()
            .all(|(x, y)| self.tile(x.floor() as i32, y.floor() as i32) != Tile::Wall)
    }

    pub fn count_dead_ends(&self) -> usize {
        let mut count = 0;
        for y in 1..self.height() as i32 - 1 {
            for x in 1..self.width() as i32 - 1 {
                if self.tile(x, y) != Tile::Wall {
                    let walls = [
                        self.tile(x - 1, y),
                        self.tile(x + 1, y),
                        self.tile(x, y - 1),
                        self.tile(x, y + 1),
                    ]
                    .iter()
                    .filter(|&&t| t == Tile::Wall)
                    .count();
                    if walls >= 3 {
                        count += 1;
                    }
                }
            }
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashSet, VecDeque};

    #[test]
    fn rows_have_the_same_width() {
        for level in 1..=3 {
            let maze = Maze::for_level(level).unwrap();
            assert!(maze.rows.iter().all(|row| row.len() == maze.width()));
        }
    }

    #[test]
    fn outer_border_is_closed() {
        for level in 1..=3 {
            let maze = Maze::for_level(level).unwrap();
            let last_x = maze.width() as i32 - 1;
            let last_y = maze.height() as i32 - 1;

            for x in 0..=last_x {
                assert_eq!(maze.tile(x, 0), Tile::Wall);
                assert_eq!(maze.tile(x, last_y), Tile::Wall);
            }
            for y in 0..=last_y {
                assert_eq!(maze.tile(0, y), Tile::Wall);
                assert_eq!(maze.tile(last_x, y), Tile::Wall);
            }
        }
    }

    #[test]
    fn all_spawns_and_exits_are_walkable() {
        for level in 1..=3 {
            let maze = Maze::for_level(level).unwrap();
            for spawn in maze.spawns() {
                assert!(maze.can_occupy(*spawn, 0.2));
            }
            assert!(maze.can_occupy(maze.exit(), 0.2));
            assert_eq!(
                maze.tile(maze.exit().x.floor() as i32, maze.exit().y.floor() as i32),
                Tile::Exit
            );
        }
    }

    #[test]
    fn all_levels_have_fully_reachable_exits_and_spawns() {
        for level in 1..=3 {
            let maze = Maze::for_level(level).unwrap();
            let start = (maze.spawn().x.floor() as i32, maze.spawn().y.floor() as i32);
            let mut visited = HashSet::new();
            let mut queue = VecDeque::new();

            visited.insert(start);
            queue.push_back(start);

            while let Some((cx, cy)) = queue.pop_front() {
                for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                    let nx = cx + dx;
                    let ny = cy + dy;
                    if nx > 0
                        && ny > 0
                        && (nx as usize) < maze.width() - 1
                        && (ny as usize) < maze.height() - 1
                        && maze.tile(nx, ny) != Tile::Wall
                        && visited.insert((nx, ny))
                    {
                        queue.push_back((nx, ny));
                    }
                }
            }

            // Verify exit is reachable
            let exit_tile = (maze.exit().x.floor() as i32, maze.exit().y.floor() as i32);
            assert!(
                visited.contains(&exit_tile),
                "exit tile must be reachable in level {level}"
            );

            // Verify every spawn point is reachable
            for spawn in maze.spawns() {
                let spawn_tile = (spawn.x.floor() as i32, spawn.y.floor() as i32);
                assert!(
                    visited.contains(&spawn_tile),
                    "spawn tile must be reachable in level {level}"
                );
            }
        }
    }

    #[test]
    fn dead_ends_increase_with_difficulty() {
        let dead_ends_1 = Maze::for_level(1).unwrap().count_dead_ends();
        let dead_ends_2 = Maze::for_level(2).unwrap().count_dead_ends();
        let dead_ends_3 = Maze::for_level(3).unwrap().count_dead_ends();

        assert!(
            dead_ends_1 < dead_ends_2,
            "level 2 ({dead_ends_2}) must have more dead ends than level 1 ({dead_ends_1})"
        );
        assert!(
            dead_ends_2 < dead_ends_3,
            "level 3 ({dead_ends_3}) must have more dead ends than level 2 ({dead_ends_2})"
        );
    }

    #[test]
    fn walls_and_outside_positions_are_blocked() {
        let maze = Maze::level_one();

        assert!(!maze.can_occupy(Position { x: 0.5, y: 0.5 }, 0.2));
        assert!(!maze.can_occupy(Position { x: -1.0, y: 1.5 }, 0.2));
    }

    #[test]
    fn player_radius_prevents_clipping_a_wall() {
        let maze = Maze::level_one();

        assert!(maze.can_occupy(Position { x: 1.5, y: 1.5 }, 0.2));
        assert!(!maze.can_occupy(Position { x: 1.1, y: 1.5 }, 0.2));
    }
}
