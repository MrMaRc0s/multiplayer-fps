use crate::protocol::PlayerSnapshot;
use crate::world::{Maze, Position, Tile};
use sdl2::pixels::Color;
use sdl2::rect::{Point, Rect};
use sdl2::render::Canvas;
use sdl2::video::Window;
use std::f32::consts::PI;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const FIELD_OF_VIEW: f32 = PI / 3.0;
const COLUMN_WIDTH: usize = 2;
const MIN_DISTANCE: f32 = 0.001;
const NEAR_PLANE: f32 = 0.05;
const BEAM_WORLD_HEIGHT: f32 = 0.08;

#[allow(dead_code)]
pub const SHOT_TRACER_LIFETIME: Duration = Duration::from_millis(200);
#[allow(dead_code)]
pub const HIT_CONFIRMATION_LIFETIME: Duration = Duration::from_millis(150);
#[allow(dead_code)]
pub const DAMAGE_FEEDBACK_LIFETIME: Duration = Duration::from_millis(150);
pub const BLINK_CYCLE_MS: u128 = 150;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShotTracer {
    pub shooter_id: u32,
    pub from_x: f32,
    pub from_y: f32,
    pub to_x: f32,
    pub to_y: f32,
    pub start_time: Instant,
    pub duration: Duration,
}

#[allow(dead_code)]
impl ShotTracer {
    #[allow(dead_code)]
    pub fn new(
        shooter_id: u32,
        from_x: f32,
        from_y: f32,
        to_x: f32,
        to_y: f32,
        start_time: Instant,
    ) -> Self {
        Self {
            shooter_id,
            from_x,
            from_y,
            to_x,
            to_y,
            start_time,
            duration: SHOT_TRACER_LIFETIME,
        }
    }

    #[allow(dead_code)]
    pub fn with_duration(
        shooter_id: u32,
        from_x: f32,
        from_y: f32,
        to_x: f32,
        to_y: f32,
        start_time: Instant,
        duration: Duration,
    ) -> Self {
        Self {
            shooter_id,
            from_x,
            from_y,
            to_x,
            to_y,
            start_time,
            duration,
        }
    }

    #[allow(dead_code)]
    pub fn is_active(&self, now: Instant) -> bool {
        now.duration_since(self.start_time) < self.duration
    }

    /// Pure deterministic progress calculation taking elapsed duration.
    /// Returns Some(progress 0.0..1.0) while active, or None once expired.
    pub fn progress_from_elapsed(elapsed: Duration, duration: Duration) -> Option<f32> {
        if duration.is_zero() || elapsed >= duration {
            None
        } else {
            Some((elapsed.as_secs_f32() / duration.as_secs_f32()).clamp(0.0, 1.0))
        }
    }

    pub fn progress(&self, now: Instant) -> Option<f32> {
        if now < self.start_time {
            return Some(0.0);
        }
        let elapsed = now.duration_since(self.start_time);
        Self::progress_from_elapsed(elapsed, self.duration)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CombatVisuals<'a> {
    pub tracers: &'a [ShotTracer],
    pub hit_confirmation_until: Option<Instant>,
    pub damage_feedback_until: Option<Instant>,
    pub now: Option<Instant>,
}

#[allow(dead_code)]
impl<'a> CombatVisuals<'a> {
    pub fn is_hit_confirmed(&self, current: Instant) -> bool {
        self.hit_confirmation_until
            .is_some_and(|until| current < until)
    }

    pub fn is_damage_active(&self, current: Instant) -> bool {
        self.damage_feedback_until
            .is_some_and(|until| current < until)
    }

    pub fn is_effect_active_from_elapsed(elapsed: Duration, duration: Duration) -> bool {
        elapsed < duration
    }
}

#[derive(Debug, Clone, Copy)]
struct RayHit {
    distance: f32,
    hit_vertical_side: bool,
}

#[allow(dead_code)]
pub(super) fn draw(
    canvas: &mut Canvas<Window>,
    maze: &Maze,
    player: Option<&PlayerSnapshot>,
    players: &[PlayerSnapshot],
) -> Result<(), String> {
    draw_with_effects(canvas, maze, player, players, &CombatVisuals::default())
}

#[allow(dead_code)]
pub(super) fn draw_with_effects(
    canvas: &mut Canvas<Window>,
    maze: &Maze,
    player: Option<&PlayerSnapshot>,
    players: &[PlayerSnapshot],
    effects: &CombatVisuals,
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

    let now = effects.now.unwrap_or_else(Instant::now);
    let blink_phase = should_blink(now);

    draw_other_players(
        canvas,
        player,
        players,
        &wall_depth,
        projection_distance,
        width,
        height,
        blink_phase,
    )?;

    let view_ctx = ViewContext {
        observer: player,
        wall_depth: &wall_depth,
        projection_distance,
        screen_width: width,
        screen_height: height,
    };

    for tracer in effects.tracers {
        draw_shot_tracer(canvas, &view_ctx, tracer, now)?;
    }

    if effects.is_hit_confirmed(now) {
        draw_hit_confirmation(canvas, width, height)?;
    }

    if effects.is_damage_active(now) {
        draw_damage_feedback(canvas, width, height)?;
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpriteProjection {
    pub depth: f32,
    pub center_x: f32,
    pub size: f32,
}

#[allow(clippy::too_many_arguments)]
fn draw_other_players(
    canvas: &mut Canvas<Window>,
    observer: &PlayerSnapshot,
    players: &[PlayerSnapshot],
    wall_depth: &[f32],
    projection_distance: f32,
    screen_width: u32,
    screen_height: u32,
    blink_phase: bool,
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

    for (player, projection) in visible {
        draw_eye_sprite(
            canvas,
            player,
            projection,
            wall_depth,
            screen_height,
            blink_phase,
        )?;
    }
    Ok(())
}

pub fn project_player(
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
    if depth <= NEAR_PLANE {
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
    player: &PlayerSnapshot,
    projection: SpriteProjection,
    wall_depth: &[f32],
    screen_height: u32,
    blink_phase: bool,
) -> Result<(), String> {
    let half_width = projection.size / 2.0;
    let left = (projection.center_x - half_width).floor() as i32;
    let right = (projection.center_x + half_width).ceil() as i32;
    let center_y = screen_height as f32 / 2.0;

    let is_protected = player.is_protected;

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

        // Protected visual effect: alternate visibility / dashed flash colors
        if is_protected && !blink_phase && (screen_x / COLUMN_WIDTH as i32) % 2 == 0 {
            // Dashed gap during inactive blink phase
            continue;
        }

        let sclera_color = if is_protected && blink_phase {
            // Bright shield flash
            Color::RGB(255, 255, 255)
        } else if is_protected {
            // Inverted/dim protective glow
            Color::RGB(80, 160, 220)
        } else {
            Color::RGB(225, 225, 215)
        };

        canvas.set_draw_color(sclera_color);
        canvas.fill_rect(Rect::new(
            screen_x,
            (center_y - eye_half_height).round() as i32,
            COLUMN_WIDTH as u32,
            (eye_half_height * 2.0).round() as u32,
        ))?;

        if normalized_x.abs() < 0.23 {
            let pupil_color = if is_protected && blink_phase {
                Color::RGB(255, 220, 50)
            } else if is_protected {
                Color::RGB(240, 240, 240)
            } else {
                Color::RGB(190, 35, 35)
            };

            canvas.set_draw_color(pupil_color);
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClippedPoint {
    pub x: f32,
    pub y: f32,
    pub depth: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClippedSegment {
    pub start: ClippedPoint,
    pub end: ClippedPoint,
}

struct ViewContext<'a> {
    observer: &'a PlayerSnapshot,
    wall_depth: &'a [f32],
    projection_distance: f32,
    screen_width: u32,
    screen_height: u32,
}

/// Clips a 2D line segment against the camera's near viewing plane.
#[allow(clippy::type_complexity)]
pub fn clip_segment_near(
    observer_x: f32,
    observer_y: f32,
    observer_dir: f32,
    p1: (f32, f32),
    p2: (f32, f32),
    near: f32,
) -> Option<ClippedSegment> {
    let forward_x = observer_dir.cos();
    let forward_y = observer_dir.sin();
    let d1 = (p1.0 - observer_x) * forward_x + (p1.1 - observer_y) * forward_y;
    let d2 = (p2.0 - observer_x) * forward_x + (p2.1 - observer_y) * forward_y;

    if d1 < near && d2 < near {
        return None;
    }

    let mut start = ClippedPoint {
        x: p1.0,
        y: p1.1,
        depth: d1,
    };
    let mut end = ClippedPoint {
        x: p2.0,
        y: p2.1,
        depth: d2,
    };

    if d1 < near {
        let t = (near - d1) / (d2 - d1);
        start.x = p1.0 + t * (p2.0 - p1.0);
        start.y = p1.1 + t * (p2.1 - p1.1);
        start.depth = near;
    } else if d2 < near {
        let t = (near - d2) / (d1 - d2);
        end.x = p2.0 + t * (p1.0 - p2.0);
        end.y = p2.1 + t * (p1.1 - p2.1);
        end.depth = near;
    }

    Some(ClippedSegment { start, end })
}

fn draw_shot_tracer(
    canvas: &mut Canvas<Window>,
    ctx: &ViewContext<'_>,
    tracer: &ShotTracer,
    now: Instant,
) -> Result<(), String> {
    let Some(progress) = tracer.progress(now) else {
        return Ok(());
    };

    let p1 = (tracer.from_x, tracer.from_y);
    let p2 = (tracer.to_x, tracer.to_y);

    let Some(clipped) = clip_segment_near(
        ctx.observer.x,
        ctx.observer.y,
        ctx.observer.direction,
        p1,
        p2,
        NEAR_PLANE,
    ) else {
        return Ok(());
    };

    let forward_y = ctx.observer.direction.sin();
    let forward_x = ctx.observer.direction.cos();

    let sideways1 = -(clipped.start.x - ctx.observer.x) * forward_y
        + (clipped.start.y - ctx.observer.y) * forward_x;
    let sx1 =
        ctx.screen_width as f32 / 2.0 + (sideways1 / clipped.start.depth) * ctx.projection_distance;

    let sideways2 = -(clipped.end.x - ctx.observer.x) * forward_y
        + (clipped.end.y - ctx.observer.y) * forward_x;
    let sx2 =
        ctx.screen_width as f32 / 2.0 + (sideways2 / clipped.end.depth) * ctx.projection_distance;

    let min_sx = sx1.min(sx2);
    let max_sx = sx1.max(sx2);

    if max_sx < 0.0 || min_sx >= ctx.screen_width as f32 {
        return Ok(());
    }

    let start_col = (min_sx.floor() as i32).max(0);
    let end_col = (max_sx.ceil() as i32).min(ctx.screen_width as i32 - 1);
    let horizon = ctx.screen_height as f32 / 2.0;

    // Fading monochrome brightness along the beam lifetime
    let brightness = ((1.0 - progress) * 255.0).clamp(110.0, 255.0).round() as u8;
    canvas.set_draw_color(Color::RGB(brightness, brightness, brightness));

    let inv_d1 = 1.0 / clipped.start.depth;
    let inv_d2 = 1.0 / clipped.end.depth;
    let dx = sx2 - sx1;

    for col in (start_col..=end_col).step_by(COLUMN_WIDTH) {
        if col as usize >= ctx.wall_depth.len() {
            continue;
        }

        let alpha = if dx.abs() > 0.001 {
            ((col as f32 - sx1) / dx).clamp(0.0, 1.0)
        } else {
            0.5
        };

        let inv_d = inv_d1 + alpha * (inv_d2 - inv_d1);
        let col_depth = 1.0 / inv_d.max(0.0001);

        if col_depth >= ctx.wall_depth[col as usize] {
            continue;
        }

        let half_h =
            ((ctx.projection_distance / col_depth) * (BEAM_WORLD_HEIGHT / 2.0)).clamp(1.0, 15.0);
        canvas.fill_rect(Rect::new(
            col,
            (horizon - half_h).round() as i32,
            COLUMN_WIDTH as u32,
            (half_h * 2.0).round() as u32,
        ))?;
    }

    Ok(())
}

fn draw_hit_confirmation(
    canvas: &mut Canvas<Window>,
    screen_width: u32,
    screen_height: u32,
) -> Result<(), String> {
    let cx = (screen_width / 2) as i32;
    let cy = (screen_height / 2) as i32;

    // Subtle retro monochrome hit crosshair
    canvas.set_draw_color(Color::RGB(255, 255, 255));
    canvas.draw_line(Point::new(cx, cy - 8), Point::new(cx, cy - 3))?;
    canvas.draw_line(Point::new(cx, cy + 3), Point::new(cx, cy + 8))?;
    canvas.draw_line(Point::new(cx - 8, cy), Point::new(cx - 3, cy))?;
    canvas.draw_line(Point::new(cx + 3, cy), Point::new(cx + 8, cy))?;

    // Subtle diagonal corner ticks
    canvas.draw_line(Point::new(cx - 5, cy - 5), Point::new(cx - 2, cy - 2))?;
    canvas.draw_line(Point::new(cx + 2, cy + 2), Point::new(cx + 5, cy + 5))?;
    canvas.draw_line(Point::new(cx + 2, cy - 2), Point::new(cx + 5, cy - 5))?;
    canvas.draw_line(Point::new(cx - 5, cy + 2), Point::new(cx - 2, cy + 5))?;

    Ok(())
}

fn draw_damage_feedback(
    canvas: &mut Canvas<Window>,
    screen_width: u32,
    screen_height: u32,
) -> Result<(), String> {
    // Subtle border flash for damage indicator
    canvas.set_draw_color(Color::RGB(180, 180, 180));
    let thickness = 4;
    canvas.fill_rect(Rect::new(0, 0, screen_width, thickness))?;
    canvas.fill_rect(Rect::new(
        0,
        screen_height as i32 - thickness as i32,
        screen_width,
        thickness,
    ))?;
    canvas.fill_rect(Rect::new(0, 0, thickness, screen_height))?;
    canvas.fill_rect(Rect::new(
        screen_width as i32 - thickness as i32,
        0,
        thickness,
        screen_height,
    ))?;
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

pub fn is_blink_visible(elapsed: Duration, cycle_ms: u128) -> bool {
    if cycle_ms == 0 {
        return true;
    }
    (elapsed.as_millis() / cycle_ms).is_multiple_of(2)
}

pub fn should_blink(now: Instant) -> bool {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    let epoch = *EPOCH.get_or_init(Instant::now);
    let elapsed = now.duration_since(epoch);
    is_blink_visible(elapsed, BLINK_CYCLE_MS)
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
            score: 0,
            is_protected: false,
        };
        let target = PlayerSnapshot {
            id: 2,
            x: 3.5,
            y: 1.5,
            direction: 0.0,
            score: 0,
            is_protected: false,
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
            score: 0,
            is_protected: false,
        };
        let target = PlayerSnapshot {
            id: 2,
            x: 1.5,
            y: 1.5,
            direction: 0.0,
            score: 0,
            is_protected: false,
        };

        assert!(project_player(&observer, &target, 800.0, 800).is_none());
    }

    #[test]
    fn protected_player_projects_identically() {
        let observer = PlayerSnapshot {
            id: 1,
            x: 1.5,
            y: 1.5,
            direction: 0.0,
            score: 0,
            is_protected: false,
        };
        let target = PlayerSnapshot {
            id: 2,
            x: 3.5,
            y: 1.5,
            direction: 0.0,
            score: 0,
            is_protected: true,
        };

        let projection = project_player(&observer, &target, 800.0, 800).unwrap();
        assert_eq!(projection.depth, 2.0);
    }

    #[test]
    fn clip_segment_both_behind_camera_returns_none() {
        let p1 = (-5.0, 0.0);
        let p2 = (-2.0, 0.0);
        let result = clip_segment_near(0.0, 0.0, 0.0, p1, p2, 0.05);
        assert!(result.is_none());
    }

    #[test]
    fn clip_segment_one_behind_camera_is_clipped() {
        let p1 = (-1.0, 0.0);
        let p2 = (3.0, 0.0);
        let result = clip_segment_near(0.0, 0.0, 0.0, p1, p2, 0.05).unwrap();
        assert!((result.start.depth - 0.05).abs() < 0.001);
        assert!((result.end.depth - 3.0).abs() < 0.001);
    }

    #[test]
    fn clip_segment_both_in_front_preserves_depths() {
        let p1 = (1.0, 0.0);
        let p2 = (4.0, 0.0);
        let result = clip_segment_near(0.0, 0.0, 0.0, p1, p2, 0.05).unwrap();
        assert!((result.start.depth - 1.0).abs() < 0.001);
        assert!((result.end.depth - 4.0).abs() < 0.001);
    }

    #[test]
    fn shot_tracer_progress_and_expiry() {
        let duration = Duration::from_millis(200);

        assert_eq!(
            ShotTracer::progress_from_elapsed(Duration::from_millis(0), duration),
            Some(0.0)
        );
        assert_eq!(
            ShotTracer::progress_from_elapsed(Duration::from_millis(100), duration),
            Some(0.5)
        );
        assert!(
            ShotTracer::progress_from_elapsed(Duration::from_millis(199), duration).unwrap() > 0.99
        );
        assert_eq!(
            ShotTracer::progress_from_elapsed(Duration::from_millis(200), duration),
            None
        );
        assert_eq!(
            ShotTracer::progress_from_elapsed(Duration::from_millis(250), duration),
            None
        );
    }

    #[test]
    fn hit_confirmation_and_damage_feedback_active_checks() {
        let now = Instant::now();
        let active_until = now + Duration::from_millis(150);
        let expired = now - Duration::from_millis(10);

        let visuals = CombatVisuals {
            tracers: &[],
            hit_confirmation_until: Some(active_until),
            damage_feedback_until: Some(expired),
            now: Some(now),
        };

        assert!(visuals.is_hit_confirmed(now));
        assert!(!visuals.is_damage_active(now));
        assert!(CombatVisuals::is_effect_active_from_elapsed(
            Duration::from_millis(100),
            HIT_CONFIRMATION_LIFETIME
        ));
        assert!(!CombatVisuals::is_effect_active_from_elapsed(
            Duration::from_millis(160),
            HIT_CONFIRMATION_LIFETIME
        ));
    }

    #[test]
    fn blink_cycle_alternates_deterministically() {
        assert!(is_blink_visible(Duration::from_millis(0), 150));
        assert!(is_blink_visible(Duration::from_millis(149), 150));
        assert!(!is_blink_visible(Duration::from_millis(150), 150));
        assert!(!is_blink_visible(Duration::from_millis(299), 150));
        assert!(is_blink_visible(Duration::from_millis(300), 150));
    }

    #[test]
    fn shot_tracer_constructor_and_active_check() {
        let start = Instant::now();
        let tracer = ShotTracer::new(1, 1.0, 2.0, 3.0, 4.0, start);
        assert_eq!(tracer.duration, SHOT_TRACER_LIFETIME);
        assert!(tracer.is_active(start + Duration::from_millis(50)));
        assert!(!tracer.is_active(start + SHOT_TRACER_LIFETIME + Duration::from_millis(1)));

        let custom =
            ShotTracer::with_duration(2, 0.0, 0.0, 1.0, 1.0, start, DAMAGE_FEEDBACK_LIFETIME);
        assert_eq!(custom.duration, DAMAGE_FEEDBACK_LIFETIME);
    }
}
