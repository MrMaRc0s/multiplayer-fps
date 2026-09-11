use crate::client::{Connection, WorldSnapshot};
use crate::protocol::CombatEventKind;
use crate::server::combat;
use crate::world::{Maze, Position};
use sdl2::event::Event;
use sdl2::keyboard::{Keycode, Scancode};
use sdl2::mouse::MouseButton;
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub mod hud;
pub mod minimap;
pub mod view;

const WINDOW_WIDTH: u32 = 960;
const WINDOW_HEIGHT: u32 = 600;
const INPUT_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / 60);

pub fn run(username: &str, mut connection: Connection) -> Result<(), String> {
    let sdl = sdl2::init().map_err(|error| format!("could not initialize SDL: {error}"))?;
    let video = sdl
        .video()
        .map_err(|error| format!("could not initialize video: {error}"))?;
    let title = format!(
        "Maze Wars - {username} - Player {} - Level {} - W/S move, A/D turn, Space shoot",
        connection.player_id(),
        connection.level()
    );
    let window = video
        .window(&title, WINDOW_WIDTH, WINDOW_HEIGHT)
        .position_centered()
        .resizable()
        .build()
        .map_err(|error| format!("could not create the game window: {error}"))?;
    let mut canvas = window
        .into_canvas()
        .present_vsync()
        .build()
        .map_err(|error| format!("could not create the renderer: {error}"))?;
    let mut events = sdl
        .event_pump()
        .map_err(|error| format!("could not read input events: {error}"))?;
    let mut maze = Maze::for_level(connection.level()).unwrap_or_else(Maze::level_one);
    let mut latest_snapshot: Option<WorldSnapshot> = None;
    let mut input_sequence = 0_u32;
    let mut fire_sequence = 0_u32;
    let mut last_input = Instant::now();
    let mut fps_tracker = hud::FpsTracker::default();
    let mut roster: HashMap<u32, String> = HashMap::new();
    let mut shot_tracers: Vec<view::ShotTracer> = Vec::with_capacity(32);
    let mut hit_confirmation_until: Option<Instant> = None;
    let mut damage_feedback_until: Option<Instant> = None;

    'game: loop {
        let now = Instant::now();
        fps_tracker.record_frame(now);

        let own_snapshot = latest_snapshot.as_ref().and_then(|s| {
            s.players
                .iter()
                .find(|p| p.id == connection.player_id())
                .copied()
        });

        for event in events.poll_iter() {
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => break 'game,
                Event::KeyDown {
                    keycode: Some(Keycode::Space),
                    repeat: false,
                    ..
                }
                | Event::MouseButtonDown {
                    mouse_btn: MouseButton::Left,
                    ..
                } => {
                    fire_sequence = fire_sequence.wrapping_add(1);
                    connection.send_fire(fire_sequence)?;
                    if let Some(player) = own_snapshot {
                        let to_x = player.x + player.direction.cos() * 50.0;
                        let to_y = player.y + player.direction.sin() * 50.0;
                        let start = Position {
                            x: player.x,
                            y: player.y,
                        };
                        let max_target = Position { x: to_x, y: to_y };
                        let hit_end = if let Some((_, hit)) =
                            combat::swept_wall_collision(&maze, start, max_target)
                        {
                            (hit.x, hit.y)
                        } else {
                            (to_x, to_y)
                        };
                        shot_tracers.push(view::ShotTracer::new(
                            connection.player_id(),
                            player.x,
                            player.y,
                            hit_end.0,
                            hit_end.1,
                            now,
                        ));
                    }
                }
                _ => {}
            }
        }

        let keyboard = events.keyboard_state();
        let forward = axis(
            keyboard.is_scancode_pressed(Scancode::W) || keyboard.is_scancode_pressed(Scancode::Up),
            keyboard.is_scancode_pressed(Scancode::S)
                || keyboard.is_scancode_pressed(Scancode::Down),
        );
        let turn = axis(
            keyboard.is_scancode_pressed(Scancode::D)
                || keyboard.is_scancode_pressed(Scancode::Right),
            keyboard.is_scancode_pressed(Scancode::A)
                || keyboard.is_scancode_pressed(Scancode::Left),
        );

        if last_input.elapsed() >= INPUT_INTERVAL {
            input_sequence = input_sequence.wrapping_add(1);
            connection.send_input(input_sequence, forward, turn)?;
            last_input = Instant::now();
        }

        let updates = connection.receive_updates()?;
        if let Some(roster_players) = updates.roster {
            for (id, name) in roster_players {
                roster.insert(id, name);
            }
        }

        for event in updates.combat_events {
            match event {
                CombatEventKind::ShotFired {
                    shooter_id,
                    from_x,
                    from_y,
                    to_x,
                    to_y,
                } => {
                    let already_has = shooter_id == connection.player_id()
                        && shot_tracers
                            .iter()
                            .any(|t| t.shooter_id == shooter_id && t.is_active(now));
                    if !already_has {
                        shot_tracers.push(view::ShotTracer::new(
                            shooter_id, from_x, from_y, to_x, to_y, now,
                        ));
                    }
                }
                CombatEventKind::PlayerHit {
                    shooter_id,
                    target_id,
                } => {
                    if shooter_id == connection.player_id() {
                        hit_confirmation_until = Some(now + view::HIT_CONFIRMATION_LIFETIME);
                    }
                    if target_id == connection.player_id() {
                        damage_feedback_until = Some(now + view::DAMAGE_FEEDBACK_LIFETIME);
                    }
                }
            }
        }

        if let Some(snapshot) = updates.snapshot {
            if snapshot.level != maze.level()
                && let Some(new_maze) = Maze::for_level(snapshot.level)
            {
                maze = new_maze;
            }
            latest_snapshot = Some(snapshot);
        }

        shot_tracers.retain(|tracer| tracer.is_active(now));

        let players = latest_snapshot
            .as_ref()
            .map_or(&[][..], |snapshot| snapshot.players.as_slice());
        let own_player = players
            .iter()
            .find(|player| player.id == connection.player_id());
        let mut scoreboard_storage = [hud::ScoreboardEntry::new(0, None, 0); 64];
        let (scoreboard, count) = if players.is_empty() {
            scoreboard_storage[0] =
                hud::ScoreboardEntry::new(connection.player_id(), Some(username), 0);
            (&scoreboard_storage[..1], 1)
        } else {
            let count = players.len().min(64);
            for (i, player) in players.iter().take(count).enumerate() {
                let name = if player.id == connection.player_id() {
                    Some(username)
                } else {
                    roster.get(&player.id).map(|s| s.as_str())
                };
                scoreboard_storage[i] = hud::ScoreboardEntry::new(player.id, name, player.score);
            }
            (&scoreboard_storage[..count], count)
        };

        let combat_visuals = view::CombatVisuals {
            tracers: &shot_tracers,
            hit_confirmation_until,
            damage_feedback_until,
            now: Some(now),
        };

        view::draw_with_effects(&mut canvas, &maze, own_player, players, &combat_visuals)?;
        minimap::draw_with_effects(
            &mut canvas,
            &maze,
            connection.player_id(),
            players,
            &shot_tracers,
            Some(now),
        )?;
        hud::draw(
            &mut canvas,
            fps_tracker.fps(),
            Some(maze.level()),
            Some(connection.player_id()),
            &scoreboard[..count],
        )?;
        canvas.present();
    }

    Ok(())
}

fn axis(positive: bool, negative: bool) -> i8 {
    match (positive, negative) {
        (true, false) => 1,
        (false, true) => -1,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opposing_keys_cancel_each_other() {
        assert_eq!(axis(true, false), 1);
        assert_eq!(axis(false, true), -1);
        assert_eq!(axis(true, true), 0);
        assert_eq!(axis(false, false), 0);
    }
}
