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
pub mod menu;
pub mod minimap;
pub mod view;

const WINDOW_WIDTH: u32 = 960;
const WINDOW_HEIGHT: u32 = 600;
const INPUT_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / 60);
const MAX_SHOT_TRACERS: usize = 32;

/// Push a tracer while keeping the buffer bounded at [`MAX_SHOT_TRACERS`].
/// Drops the oldest tracer when full so rapid fire stays visible but bounded.
fn push_shot_tracer(tracers: &mut Vec<view::ShotTracer>, tracer: view::ShotTracer) {
    while tracers.len() >= MAX_SHOT_TRACERS {
        tracers.remove(0);
    }
    tracers.push(tracer);
}

/// Reconcile an authoritative `ShotFired` with the local prediction.
///
/// Keeps immediate local feedback (predictive tracers are pushed on fire),
/// but the authoritative echo corrects the display: for the local shooter,
/// expire one matching active predictive tracer (oldest first, FIFO so N
/// rapid clicks reconcile to N authoritative segments) and push the
/// authoritative segment. Remote shooters are pushed directly.
#[allow(clippy::too_many_arguments)]
fn apply_authoritative_shot(
    tracers: &mut Vec<view::ShotTracer>,
    shooter_id: u32,
    local_player_id: u32,
    from_x: f32,
    from_y: f32,
    to_x: f32,
    to_y: f32,
    now: Instant,
) {
    let mut final_to_x = to_x;
    let mut final_to_y = to_y;
    if shooter_id == local_player_id
        && let Some(index) = tracers
            .iter()
            .position(|t| t.shooter_id == shooter_id && t.is_active(now))
    {
        let old = tracers.remove(index);
        let server_dist_sq = (to_x - from_x).powi(2) + (to_y - from_y).powi(2);
        let old_dist_sq = (old.to_x - old.from_x).powi(2) + (old.to_y - old.from_y).powi(2);
        if old_dist_sq < server_dist_sq - 0.01 {
            final_to_x = old.to_x;
            final_to_y = old.to_y;
        }
    }
    push_shot_tracer(
        tracers,
        view::ShotTracer::new(shooter_id, from_x, from_y, final_to_x, final_to_y, now),
    );
}

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
    let mut last_frame_time = Instant::now();
    let mut fps_tracker = hud::FpsTracker::default();
    let mut roster: HashMap<u32, String> = HashMap::new();
    let mut shot_tracers: Vec<view::ShotTracer> = Vec::with_capacity(MAX_SHOT_TRACERS);
    let mut hit_confirmation_until: Option<Instant> = None;
    let mut damage_feedback_until: Option<Instant> = None;
    let mut enemy_manager = crate::enemy::EnemyManager::for_maze(&maze, None);
    let mut crosshair_enabled = true;
    let mut cleared_banner_until: Option<Instant> = None;
    let mut game_state = menu::GameState::Playing;

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
                Event::Quit { .. } => break 'game,
                Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    repeat: false,
                    ..
                } => match game_state {
                    menu::GameState::Playing => {
                        game_state = menu::GameState::Paused { selected_item: 0 };
                        input_sequence = input_sequence.wrapping_add(1);
                        let _ = connection.send_input(input_sequence, 0, 0);
                    }
                    menu::GameState::Paused { .. } => {
                        game_state = menu::GameState::Playing;
                    }
                    menu::GameState::GameOver => {
                        game_state = menu::GameState::Paused { selected_item: 0 };
                    }
                },
                Event::KeyDown {
                    keycode: Some(Keycode::C),
                    repeat: false,
                    ..
                } if game_state.is_playing() => {
                    crosshair_enabled = !crosshair_enabled;
                }
                Event::KeyDown {
                    keycode: Some(Keycode::E),
                    repeat: false,
                    ..
                } if game_state.is_playing() => {
                    enemy_manager.toggle_enabled();
                    cleared_banner_until = None;
                }
                Event::KeyDown {
                    keycode: Some(Keycode::E),
                    repeat: false,
                    ..
                } if game_state.is_game_over() => {
                    enemy_manager.set_enabled(false);
                    enemy_manager.clear_player_lost();
                    cleared_banner_until = None;
                    game_state = menu::GameState::Playing;
                }
                Event::KeyDown {
                    keycode: Some(Keycode::R),
                    repeat: false,
                    ..
                } if game_state.is_game_over() => {
                    let spawn_pos = maze.spawn();
                    if let Some(snapshot) = &mut latest_snapshot {
                        for player in &mut snapshot.players {
                            if player.id == connection.player_id() {
                                player.x = spawn_pos.x;
                                player.y = spawn_pos.y;
                            }
                        }
                    }
                    enemy_manager.reset(&maze, Some(spawn_pos));
                    cleared_banner_until = None;
                    game_state = menu::GameState::Playing;
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Q),
                    repeat: false,
                    ..
                } if !game_state.is_playing() => {
                    break 'game;
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Up | Keycode::W),
                    repeat: false,
                    ..
                } if game_state.is_paused() => {
                    game_state.move_selection_up();
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Down | Keycode::S),
                    repeat: false,
                    ..
                } if game_state.is_paused() => {
                    game_state.move_selection_down();
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Return | Keycode::Space),
                    repeat: false,
                    ..
                } if game_state.is_paused() => {
                    match game_state.activate_selection() {
                        menu::MenuAction::Resume => {}
                        menu::MenuAction::ToggleCrosshair => {
                            crosshair_enabled = !crosshair_enabled;
                        }
                        menu::MenuAction::ToggleEnemies => {
                            enemy_manager.toggle_enabled();
                            cleared_banner_until = None;
                        }
                        menu::MenuAction::ResetEnemies => {
                            let own_pos = own_snapshot.map(|p| Position { x: p.x, y: p.y });
                            enemy_manager.reset(&maze, own_pos);
                            cleared_banner_until = None;
                        }
                        menu::MenuAction::Quit => break 'game,
                        menu::MenuAction::None => {}
                    }
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Space),
                    repeat: false,
                    ..
                }
                | Event::MouseButtonDown {
                    mouse_btn: MouseButton::Left,
                    ..
                } if game_state.is_playing() => {
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
                        let (wall_end_x, wall_end_y) = if let Some((_, hit)) =
                            combat::swept_wall_collision(&maze, start, max_target)
                        {
                            (hit.x, hit.y)
                        } else {
                            (to_x, to_y)
                        };

                        let wall_end_pos = Position {
                            x: wall_end_x,
                            y: wall_end_y,
                        };

                        let (tracer_to_x, tracer_to_y) =
                            if let Some((_id, hit_pos)) = enemy_manager.check_shot_hit(start, wall_end_pos) {
                                hit_confirmation_until = Some(now + view::HIT_CONFIRMATION_LIFETIME);
                                if enemy_manager.is_cleared() {
                                    cleared_banner_until = Some(now + Duration::from_secs(3));
                                }
                                (hit_pos.x, hit_pos.y)
                            } else {
                                (wall_end_x, wall_end_y)
                            };

                        push_shot_tracer(
                            &mut shot_tracers,
                            view::ShotTracer::new(
                                connection.player_id(),
                                player.x,
                                player.y,
                                tracer_to_x,
                                tracer_to_y,
                                now,
                            ),
                        );
                    }
                }
                _ => {}
            }
        }

        let (forward, turn) = if game_state.is_playing() {
            let keyboard = events.keyboard_state();
            (
                axis(
                    keyboard.is_scancode_pressed(Scancode::W)
                        || keyboard.is_scancode_pressed(Scancode::Up),
                    keyboard.is_scancode_pressed(Scancode::S)
                        || keyboard.is_scancode_pressed(Scancode::Down),
                ),
                axis(
                    keyboard.is_scancode_pressed(Scancode::D)
                        || keyboard.is_scancode_pressed(Scancode::Right),
                    keyboard.is_scancode_pressed(Scancode::A)
                        || keyboard.is_scancode_pressed(Scancode::Left),
                ),
            )
        } else {
            (0, 0)
        };

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
                    apply_authoritative_shot(
                        &mut shot_tracers,
                        shooter_id,
                        connection.player_id(),
                        from_x,
                        from_y,
                        to_x,
                        to_y,
                        now,
                    );
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

        let dt = now.duration_since(last_frame_time).as_secs_f32();
        last_frame_time = now;

        if game_state.is_playing()
            && let Some(player) = own_snapshot
        {
            let player_pos = Position {
                x: player.x,
                y: player.y,
            };
            let lost = enemy_manager.update(dt, &maze, player_pos);
            if lost {
                game_state = menu::GameState::GameOver;
                input_sequence = input_sequence.wrapping_add(1);
                let _ = connection.send_input(input_sequence, 0, 0);
            }
        }

        if let Some(snapshot) = updates.snapshot {
            if snapshot.level != maze.level()
                && let Some(new_maze) = Maze::for_level(snapshot.level)
            {
                maze = new_maze;
                cleared_banner_until = None;
                let own_pos = own_snapshot.map(|p| Position { x: p.x, y: p.y });
                let was_enabled = enemy_manager.is_enabled();
                enemy_manager.spawn_enemies_for_maze(&maze, own_pos);
                enemy_manager.set_enabled(was_enabled);
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

        let active_enemies = if enemy_manager.is_enabled() {
            enemy_manager.enemies()
        } else {
            &[][..]
        };

        let combat_visuals = view::CombatVisuals {
            tracers: &shot_tracers,
            hit_confirmation_until,
            damage_feedback_until,
            now: Some(now),
            enemies: active_enemies,
            show_crosshair: crosshair_enabled,
        };

        view::draw_with_effects(&mut canvas, &maze, own_player, players, &combat_visuals)?;
        minimap::draw_with_effects(
            &mut canvas,
            &maze,
            connection.player_id(),
            players,
            &shot_tracers,
            active_enemies,
            Some(now),
        )?;
        hud::draw(
            &mut canvas,
            fps_tracker.fps(),
            Some(maze.level()),
            Some(connection.player_id()),
            &scoreboard[..count],
        )?;

        if cleared_banner_until.is_some_and(|until| now < until) && !game_state.is_game_over() {
            menu::draw_maze_cleared_banner(&mut canvas)?;
        }

        match game_state {
            menu::GameState::Paused { selected_item } => {
                menu::draw_pause_menu(
                    &mut canvas,
                    selected_item,
                    crosshair_enabled,
                    enemy_manager.is_enabled(),
                )?;
            }
            menu::GameState::GameOver => {
                menu::draw_game_over_dialog(&mut canvas)?;
            }
            menu::GameState::Playing => {}
        }

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

    #[test]
    fn test_apply_authoritative_shot_preserves_truncated_enemy_endpoint() {
        let mut tracers = Vec::new();
        let now = Instant::now();
        let local_id = 1;

        // Simulate local predictive shot truncated at enemy hit (e.g., x=4.0) instead of wall (x=10.0)
        push_shot_tracer(
            &mut tracers,
            view::ShotTracer::new(local_id, 1.0, 1.0, 4.0, 1.0, now),
        );
        assert_eq!(tracers.len(), 1);
        assert_eq!(tracers[0].to_x, 4.0);

        // Server echoes ShotFired with wall endpoint (x=10.0)
        apply_authoritative_shot(&mut tracers, local_id, local_id, 1.0, 1.0, 10.0, 1.0, now);

        // Reconciled tracer must retain the truncated endpoint (x=4.0)
        assert_eq!(tracers.len(), 1);
        assert_eq!(tracers[0].to_x, 4.0);
        assert_eq!(tracers[0].to_y, 1.0);
    }

    #[test]
    fn test_apply_authoritative_shot_normal_reconciliation() {
        let mut tracers = Vec::new();
        let now = Instant::now();
        let local_id = 1;

        // Predictive tracer without enemy truncation (both around x=10.0)
        push_shot_tracer(
            &mut tracers,
            view::ShotTracer::new(local_id, 1.0, 1.0, 10.0, 1.0, now),
        );

        // Authoritative server shot arrives with authoritative endpoint
        apply_authoritative_shot(&mut tracers, local_id, local_id, 1.0, 1.0, 10.0, 1.0, now);

        assert_eq!(tracers.len(), 1);
        assert_eq!(tracers[0].to_x, 10.0);
    }

    #[test]
    fn test_apply_authoritative_shot_remote_player() {
        let mut tracers = Vec::new();
        let now = Instant::now();
        let local_id = 1;
        let remote_id = 2;

        apply_authoritative_shot(&mut tracers, remote_id, local_id, 2.0, 2.0, 8.0, 2.0, now);

        assert_eq!(tracers.len(), 1);
        assert_eq!(tracers[0].shooter_id, remote_id);
        assert_eq!(tracers[0].to_x, 8.0);
    }
}
