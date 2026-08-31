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
    "#.....#.......#",
    "###############",
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
}

#[derive(Debug, Clone, Copy)]
pub struct Maze {
    rows: &'static [&'static str],
    spawn: Position,
}

impl Maze {
    pub fn level_one() -> Self {
        Self {
            rows: &LEVEL_ONE,
            spawn: Position { x: 1.5, y: 1.5 },
        }
    }

    pub fn width(&self) -> usize {
        self.rows[0].len()
    }

    pub fn height(&self) -> usize {
        self.rows.len()
    }

    pub fn spawn(&self) -> Position {
        self.spawn
    }

    pub fn tile(&self, x: i32, y: i32) -> Tile {
        if x < 0 || y < 0 || x as usize >= self.width() || y as usize >= self.height() {
            return Tile::Wall;
        }

        match self.rows[y as usize].as_bytes()[x as usize] {
            b'#' => Tile::Wall,
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
            .all(|(x, y)| self.tile(x.floor() as i32, y.floor() as i32) == Tile::Floor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_have_the_same_width() {
        let maze = Maze::level_one();

        assert!(maze.rows.iter().all(|row| row.len() == maze.width()));
    }

    #[test]
    fn outer_border_is_closed() {
        let maze = Maze::level_one();
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

    #[test]
    fn spawn_has_room_for_a_player() {
        let maze = Maze::level_one();

        assert!(maze.can_occupy(maze.spawn(), 0.2));
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
