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
    players: &[PlayerSnapshot],
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
    let mut wall_depth = vec![f32::INFINITY; width as usize];

    for column in (0..width).step_by(COLUMN_WIDTH) {
        let screen_ratio = (column as f32 + COLUMN_WIDTH as f32 / 2.0) / width as f32;
        let ray_angle = player.direction - FIELD_OF_VIEW / 2.0 + screen_ratio * FIELD_OF_VIEW;
        let Some(hit) = cast_ray(maze, origin, ray_angle) else {
            continue;
        };

        let corrected_distance =
            (hit.distance * (ray_angle - player.direction).cos()).max(MIN_DISTANCE);
        let column_end = (column as usize + COLUMN_WIDTH).min(wall_depth.len());
        wall_depth[column as usize..column_end].fill(corrected_distance);
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

    draw_other_players(
        canvas,
        player,
        players,
        &wall_depth,
        projection_distance,
        width,
        height,
    )?;

    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct SpriteProjection {
    depth: f32,
    center_x: f32,
    size: f32,
}

fn draw_other_players(
    canvas: &mut Canvas<Window>,
    observer: &PlayerSnapshot,
    players: &[PlayerSnapshot],
    wall_depth: &[f32],
    projection_distance: f32,
    screen_width: u32,
    screen_height: u32,
) -> Result<(), String> {
    let mut visible: Vec<_> = players
        .iter()
        .filter(|player| player.id != observer.id)
        .filter_map(|player| {
            project_player(observer, player, projection_distance, screen_width)
                .map(|projection| (player, projection))
        })
        .collect();
    visible.sort_by(|(_, left), (_, right)| right.depth.total_cmp(&left.depth));

    for (_, projection) in visible {
        draw_eye_sprite(canvas, projection, wall_depth, screen_height)?;
    }
    Ok(())
}

fn project_player(
    observer: &PlayerSnapshot,
    target: &PlayerSnapshot,
    projection_distance: f32,
    screen_width: u32,
) -> Option<SpriteProjection> {
    let difference_x = target.x - observer.x;
    let difference_y = target.y - observer.y;
    let forward_x = observer.direction.cos();
    let forward_y = observer.direction.sin();
    let depth = difference_x * forward_x + difference_y * forward_y;
    if depth <= 0.05 {
        return None;
    }

    let sideways = -difference_x * forward_y + difference_y * forward_x;
    let center_x = screen_width as f32 / 2.0 + sideways / depth * projection_distance;
    let size = projection_distance / depth * 0.7;
    if center_x + size / 2.0 < 0.0 || center_x - size / 2.0 >= screen_width as f32 {
        return None;
    }

    Some(SpriteProjection {
        depth,
        center_x,
        size,
    })
}

fn draw_eye_sprite(
    canvas: &mut Canvas<Window>,
    projection: SpriteProjection,
    wall_depth: &[f32],
    screen_height: u32,
) -> Result<(), String> {
    let half_width = projection.size / 2.0;
    let left = (projection.center_x - half_width).floor() as i32;
    let right = (projection.center_x + half_width).ceil() as i32;
    let center_y = screen_height as f32 / 2.0;

    for screen_x in (left..=right).step_by(COLUMN_WIDTH) {
        if screen_x < 0 || screen_x as usize >= wall_depth.len() {
            continue;
        }
        if projection.depth >= wall_depth[screen_x as usize] {
            continue;
        }

        let normalized_x = (screen_x as f32 - projection.center_x) / half_width;
        let eye_half_height = (1.0 - normalized_x.abs()).max(0.0) * projection.size * 0.3;
        if eye_half_height < 1.0 {
            continue;
        }

        canvas.set_draw_color(Color::RGB(225, 225, 215));
        canvas.fill_rect(Rect::new(
            screen_x,
            (center_y - eye_half_height).round() as i32,
            COLUMN_WIDTH as u32,
            (eye_half_height * 2.0).round() as u32,
        ))?;

        if normalized_x.abs() < 0.23 {
            canvas.set_draw_color(Color::RGB(190, 35, 35));
            canvas.fill_rect(Rect::new(
                screen_x,
                (center_y - eye_half_height * 0.55).round() as i32,
                COLUMN_WIDTH as u32,
                (eye_half_height * 1.1).round() as u32,
            ))?;
        }
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

    #[test]
    fn player_in_front_projects_to_screen_center() {
        let observer = PlayerSnapshot {
            id: 1,
            x: 1.5,
            y: 1.5,
            direction: 0.0,
        };
        let target = PlayerSnapshot {
            id: 2,
            x: 3.5,
            y: 1.5,
            direction: 0.0,
        };

        let projection = project_player(&observer, &target, 800.0, 800).unwrap();

        assert!((projection.center_x - 400.0).abs() < 0.001);
        assert_eq!(projection.depth, 2.0);
    }

    #[test]
    fn player_behind_the_camera_is_not_projected() {
        let observer = PlayerSnapshot {
            id: 1,
            x: 3.5,
            y: 1.5,
            direction: 0.0,
        };
        let target = PlayerSnapshot {
            id: 2,
            x: 1.5,
            y: 1.5,
            direction: 0.0,
        };

        assert!(project_player(&observer, &target, 800.0, 800).is_none());
    }
}
