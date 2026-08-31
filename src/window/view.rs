use crate::protocol::PlayerSnapshot;
use crate::world::{Maze, Position, Tile};
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;
use std::f32::consts::PI;

const FIELD_OF_VIEW: f32 = PI / 3.0;
const COLUMN_WIDTH: usize = 2;
const MIN_DISTANCE: f32 = 0.001;

#[derive(Debug, Clone, Copy)]
struct RayHit {
    distance: f32,
    hit_vertical_side: bool,
}

pub(super) fn draw(
    canvas: &mut Canvas<Window>,
    maze: &Maze,
    player: Option<&PlayerSnapshot>,
) -> Result<(), String> {
    let (width, height) = canvas.output_size()?;
    let horizon = height / 2;

    canvas.set_draw_color(Color::RGB(3, 3, 4));
    canvas.fill_rect(Rect::new(0, 0, width, horizon))?;
    canvas.set_draw_color(Color::RGB(12, 13, 14));
    canvas.fill_rect(Rect::new(0, horizon as i32, width, height - horizon))?;

    let Some(player) = player else {
        return Ok(());
    };

    let origin = Position {
        x: player.x,
        y: player.y,
    };
    let projection_distance = width as f32 / (2.0 * (FIELD_OF_VIEW / 2.0).tan());

    for column in (0..width).step_by(COLUMN_WIDTH) {
        let screen_ratio = (column as f32 + COLUMN_WIDTH as f32 / 2.0) / width as f32;
        let ray_angle = player.direction - FIELD_OF_VIEW / 2.0 + screen_ratio * FIELD_OF_VIEW;
        let Some(hit) = cast_ray(maze, origin, ray_angle) else {
            continue;
        };

        let corrected_distance =
            (hit.distance * (ray_angle - player.direction).cos()).max(MIN_DISTANCE);
        let wall_height = (projection_distance / corrected_distance).min(height as f32 * 2.0);
        let wall_top = (height as f32 / 2.0 - wall_height / 2.0).round() as i32;
        let brightness = wall_brightness(corrected_distance, hit.hit_vertical_side);

        canvas.set_draw_color(Color::RGB(brightness, brightness, brightness));
        canvas.fill_rect(Rect::new(
            column as i32,
            wall_top,
            COLUMN_WIDTH as u32,
            wall_height.round() as u32,
        ))?;
    }

    Ok(())
}

fn cast_ray(maze: &Maze, origin: Position, angle: f32) -> Option<RayHit> {
    let direction_x = angle.cos();
    let direction_y = angle.sin();
    let mut map_x = origin.x.floor() as i32;
    let mut map_y = origin.y.floor() as i32;

    let delta_x = if direction_x.abs() < f32::EPSILON {
        f32::INFINITY
    } else {
        (1.0 / direction_x).abs()
    };
    let delta_y = if direction_y.abs() < f32::EPSILON {
        f32::INFINITY
    } else {
        (1.0 / direction_y).abs()
    };
    let step_x = if direction_x < 0.0 { -1 } else { 1 };
    let step_y = if direction_y < 0.0 { -1 } else { 1 };
    let mut side_x = if direction_x < 0.0 {
        (origin.x - map_x as f32) * delta_x
    } else {
        (map_x as f32 + 1.0 - origin.x) * delta_x
    };
    let mut side_y = if direction_y < 0.0 {
        (origin.y - map_y as f32) * delta_y
    } else {
        (map_y as f32 + 1.0 - origin.y) * delta_y
    };

    for _ in 0..maze.width() + maze.height() {
        let (distance, hit_vertical_side) = if side_x < side_y {
            map_x += step_x;
            let distance = side_x;
            side_x += delta_x;
            (distance, true)
        } else {
            map_y += step_y;
            let distance = side_y;
            side_y += delta_y;
            (distance, false)
        };

        if maze.tile(map_x, map_y) == Tile::Wall {
            return Some(RayHit {
                distance,
                hit_vertical_side,
            });
        }
    }

    None
}

fn wall_brightness(distance: f32, hit_vertical_side: bool) -> u8 {
    let distance_shade = (225.0 / (1.0 + distance * 0.12)).clamp(45.0, 225.0);
    let side_shade = if hit_vertical_side { 1.0 } else { 0.72 };
    (distance_shade * side_shade).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ray_hits_the_west_border_from_spawn() {
        let maze = Maze::level_one();

        let hit = cast_ray(&maze, maze.spawn(), PI).unwrap();

        assert!((hit.distance - 0.5).abs() < 0.001);
    }

    #[test]
    fn ray_crosses_the_open_corridor_to_the_east() {
        let maze = Maze::level_one();

        let hit = cast_ray(&maze, maze.spawn(), 0.0).unwrap();

        assert!((hit.distance - 12.5).abs() < 0.001);
    }
}
