use crate::protocol::PlayerSnapshot;
use crate::world::{Maze, Tile};
use sdl2::pixels::Color;
use sdl2::rect::{Point, Rect};
use sdl2::render::Canvas;
use sdl2::video::Window;

const ORIGIN_X: i32 = 24;
const ORIGIN_Y: i32 = 24;
const TILE_PIXELS: i32 = 16;
const PLAYER_SIZE: u32 = 8;
const DIRECTION_LENGTH: f32 = 11.0;

pub(super) fn draw(
    canvas: &mut Canvas<Window>,
    maze: &Maze,
    own_player_id: u32,
    players: &[PlayerSnapshot],
) -> Result<(), String> {
    for y in 0..maze.height() {
        for x in 0..maze.width() {
            let color = match maze.tile(x as i32, y as i32) {
                Tile::Wall => Color::RGB(55, 88, 105),
                Tile::Floor => Color::RGB(13, 22, 29),
            };
            canvas.set_draw_color(color);
            canvas.fill_rect(Rect::new(
                ORIGIN_X + x as i32 * TILE_PIXELS,
                ORIGIN_Y + y as i32 * TILE_PIXELS,
                TILE_PIXELS as u32,
                TILE_PIXELS as u32,
            ))?;
        }
    }

    canvas.set_draw_color(Color::RGB(120, 145, 155));
    canvas.draw_rect(Rect::new(
        ORIGIN_X,
        ORIGIN_Y,
        maze.width() as u32 * TILE_PIXELS as u32,
        maze.height() as u32 * TILE_PIXELS as u32,
    ))?;

    for player in players.iter().filter(|player| player.id != own_player_id) {
        draw_player(canvas, player, Color::RGB(225, 73, 75))?;
    }
    if let Some(player) = players.iter().find(|player| player.id == own_player_id) {
        draw_player(canvas, player, Color::RGB(70, 220, 125))?;
    }

    Ok(())
}

fn draw_player(
    canvas: &mut Canvas<Window>,
    player: &PlayerSnapshot,
    color: Color,
) -> Result<(), String> {
    let center = Point::new(
        ORIGIN_X + (player.x * TILE_PIXELS as f32).round() as i32,
        ORIGIN_Y + (player.y * TILE_PIXELS as f32).round() as i32,
    );
    let half_size = PLAYER_SIZE as i32 / 2;

    canvas.set_draw_color(color);
    canvas.fill_rect(Rect::new(
        center.x - half_size,
        center.y - half_size,
        PLAYER_SIZE,
        PLAYER_SIZE,
    ))?;
    canvas.draw_line(
        center,
        Point::new(
            center.x + (player.direction.cos() * DIRECTION_LENGTH).round() as i32,
            center.y + (player.direction.sin() * DIRECTION_LENGTH).round() as i32,
        ),
    )?;
    Ok(())
}
