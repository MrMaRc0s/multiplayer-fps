use super::view::{ShotTracer, should_blink};
use crate::protocol::PlayerSnapshot;
use crate::world::{Maze, Tile};
use sdl2::pixels::Color;
use sdl2::rect::{Point, Rect};
use sdl2::render::Canvas;
use sdl2::video::Window;
use std::time::Instant;

#[allow(dead_code)]
pub const PANEL_MAX_WIDTH: i32 = 260;
#[allow(dead_code)]
pub const PANEL_MAX_HEIGHT: i32 = 220;
#[allow(dead_code)]
pub const MARGIN_X: i32 = 24;
#[allow(dead_code)]
pub const MARGIN_Y: i32 = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MinimapLayout {
    pub origin_x: i32,
    pub origin_y: i32,
    pub tile_pixels: i32,
    pub map_width_px: u32,
    pub map_height_px: u32,
    pub player_size: u32,
}

pub fn compute_layout(maze_width: usize, maze_height: usize) -> MinimapLayout {
    let mw = (maze_width as i32).max(1);
    let mh = (maze_height as i32).max(1);

    let max_tile_x = PANEL_MAX_WIDTH / mw;
    let max_tile_y = PANEL_MAX_HEIGHT / mh;
    let tile_pixels = max_tile_x.min(max_tile_y).clamp(8, 16);

    let map_width_px = (mw * tile_pixels) as u32;
    let map_height_px = (mh * tile_pixels) as u32;

    let origin_x = MARGIN_X + (PANEL_MAX_WIDTH - map_width_px as i32) / 2;
    let origin_y = MARGIN_Y + (PANEL_MAX_HEIGHT - map_height_px as i32) / 2;

    let player_size = (tile_pixels as u32 / 2).max(4);

    MinimapLayout {
        origin_x,
        origin_y,
        tile_pixels,
        map_width_px,
        map_height_px,
        player_size,
    }
}

#[allow(dead_code)]
pub(super) fn draw(
    canvas: &mut Canvas<Window>,
    maze: &Maze,
    own_player_id: u32,
    players: &[PlayerSnapshot],
) -> Result<(), String> {
    draw_with_effects(canvas, maze, own_player_id, players, &[], None)
}

#[allow(dead_code)]
pub(super) fn draw_with_effects(
    canvas: &mut Canvas<Window>,
    maze: &Maze,
    own_player_id: u32,
    players: &[PlayerSnapshot],
    tracers: &[ShotTracer],
    now: Option<Instant>,
) -> Result<(), String> {
    let layout = compute_layout(maze.width(), maze.height());
    let current_time = now.unwrap_or_else(Instant::now);
    let blink_phase = should_blink(current_time);

    // Draw maze tiles
    for y in 0..maze.height() {
        for x in 0..maze.width() {
            let color = match maze.tile(x as i32, y as i32) {
                Tile::Wall => Color::RGB(55, 88, 105),
                Tile::Floor => Color::RGB(13, 22, 29),
                Tile::Exit => Color::RGB(240, 200, 50),
            };
            canvas.set_draw_color(color);
            canvas.fill_rect(Rect::new(
                layout.origin_x + x as i32 * layout.tile_pixels,
                layout.origin_y + y as i32 * layout.tile_pixels,
                layout.tile_pixels as u32,
                layout.tile_pixels as u32,
            ))?;
        }
    }

    // Outer border
    canvas.set_draw_color(Color::RGB(120, 145, 155));
    canvas.draw_rect(Rect::new(
        layout.origin_x,
        layout.origin_y,
        layout.map_width_px,
        layout.map_height_px,
    ))?;

    // Draw active shot tracers (monochrome lines)
    for tracer in tracers {
        if tracer.progress(current_time).is_some() {
            draw_shot_tracer(canvas, tracer, &layout)?;
        }
    }

    // Draw other players
    for player in players.iter().filter(|p| p.id != own_player_id) {
        draw_player(
            canvas,
            player,
            Color::RGB(225, 73, 75),
            &layout,
            blink_phase,
        )?;
    }

    // Draw local player
    if let Some(player) = players.iter().find(|p| p.id == own_player_id) {
        draw_player(
            canvas,
            player,
            Color::RGB(70, 220, 125),
            &layout,
            blink_phase,
        )?;
    }

    Ok(())
}

fn draw_shot_tracer(
    canvas: &mut Canvas<Window>,
    tracer: &ShotTracer,
    layout: &MinimapLayout,
) -> Result<(), String> {
    let x1 = layout.origin_x + (tracer.from_x * layout.tile_pixels as f32).round() as i32;
    let y1 = layout.origin_y + (tracer.from_y * layout.tile_pixels as f32).round() as i32;
    let x2 = layout.origin_x + (tracer.to_x * layout.tile_pixels as f32).round() as i32;
    let y2 = layout.origin_y + (tracer.to_y * layout.tile_pixels as f32).round() as i32;

    // Monochrome high-visibility white line
    canvas.set_draw_color(Color::RGB(255, 255, 255));
    canvas.draw_line(Point::new(x1, y1), Point::new(x2, y2))?;
    Ok(())
}

fn draw_player(
    canvas: &mut Canvas<Window>,
    player: &PlayerSnapshot,
    color: Color,
    layout: &MinimapLayout,
    blink_phase: bool,
) -> Result<(), String> {
    let center = Point::new(
        layout.origin_x + (player.x * layout.tile_pixels as f32).round() as i32,
        layout.origin_y + (player.y * layout.tile_pixels as f32).round() as i32,
    );
    let half_size = layout.player_size as i32 / 2;

    let is_protected = player.is_protected;

    let draw_color = if is_protected && blink_phase {
        // Bright flashing shield color
        Color::RGB(255, 255, 255)
    } else {
        color
    };

    canvas.set_draw_color(draw_color);

    if is_protected && blink_phase {
        // Draw slightly larger flashing shield marker
        canvas.fill_rect(Rect::new(
            center.x - half_size - 1,
            center.y - half_size - 1,
            layout.player_size + 2,
            layout.player_size + 2,
        ))?;
    } else {
        canvas.fill_rect(Rect::new(
            center.x - half_size,
            center.y - half_size,
            layout.player_size,
            layout.player_size,
        ))?;
    }

    let direction_length = layout.tile_pixels as f32 * 0.7;
    canvas.draw_line(
        center,
        Point::new(
            center.x + (player.direction.cos() * direction_length).round() as i32,
            center.y + (player.direction.sin() * direction_length).round() as i32,
        ),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_layout_level_one() {
        let layout = compute_layout(15, 11);

        assert_eq!(layout.tile_pixels, 16);
        assert_eq!(layout.map_width_px, 240);
        assert_eq!(layout.map_height_px, 176);
        assert_eq!(layout.origin_x, MARGIN_X + (PANEL_MAX_WIDTH - 240) / 2);
        assert_eq!(layout.origin_y, MARGIN_Y + (PANEL_MAX_HEIGHT - 176) / 2);
    }

    #[test]
    fn compute_layout_level_two() {
        let layout = compute_layout(21, 15);

        assert_eq!(layout.tile_pixels, 12);
        assert_eq!(layout.map_width_px, 252);
        assert_eq!(layout.map_height_px, 180);
        assert_eq!(layout.origin_x, MARGIN_X + (PANEL_MAX_WIDTH - 252) / 2);
        assert_eq!(layout.origin_y, MARGIN_Y + (PANEL_MAX_HEIGHT - 180) / 2);
    }

    #[test]
    fn compute_layout_level_three() {
        let layout = compute_layout(25, 19);

        assert_eq!(layout.tile_pixels, 10);
        assert_eq!(layout.map_width_px, 250);
        assert_eq!(layout.map_height_px, 190);
        assert_eq!(layout.origin_x, MARGIN_X + (PANEL_MAX_WIDTH - 250) / 2);
        assert_eq!(layout.origin_y, MARGIN_Y + (PANEL_MAX_HEIGHT - 190) / 2);
    }

    #[test]
    fn all_levels_fit_within_panel_bounds() {
        for (w, h) in [(15, 11), (21, 15), (25, 19)] {
            let layout = compute_layout(w, h);
            assert!(layout.map_width_px as i32 <= PANEL_MAX_WIDTH);
            assert!(layout.map_height_px as i32 <= PANEL_MAX_HEIGHT);
            assert!(layout.origin_x >= MARGIN_X);
            assert!(layout.origin_y >= MARGIN_Y);
        }
    }

    #[test]
    fn player_marker_size_is_proportional_to_tile() {
        let l1 = compute_layout(15, 11);
        let l3 = compute_layout(25, 19);

        assert!(l1.player_size >= l3.player_size);
        assert!(l1.player_size >= 4);
        assert!(l3.player_size >= 4);
    }
}
