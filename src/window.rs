use crate::client::{Connection, WorldSnapshot};
use crate::world::Maze;
use sdl2::event::Event;
use sdl2::keyboard::{Keycode, Scancode};
use std::time::{Duration, Instant};

mod minimap;
mod view;

const WINDOW_WIDTH: u32 = 960;
const WINDOW_HEIGHT: u32 = 600;
const INPUT_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / 60);

pub fn run(username: &str, mut connection: Connection) -> Result<(), String> {
    let sdl = sdl2::init().map_err(|error| format!("could not initialize SDL: {error}"))?;
    let video = sdl
        .video()
        .map_err(|error| format!("could not initialize video: {error}"))?;
    let title = format!(
        "Maze Wars - {username} - Player {} - Level {} - W/S move, A/D turn",
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
    let maze = Maze::level_one();
    let mut latest_snapshot: Option<WorldSnapshot> = None;
    let mut input_sequence = 0_u32;
    let mut last_input = Instant::now();

    'game: loop {
        for event in events.poll_iter() {
            if matches!(
                event,
                Event::Quit { .. }
                    | Event::KeyDown {
                        keycode: Some(Keycode::Escape),
                        ..
                    }
            ) {
                break 'game;
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
        if let Some(snapshot) = connection.receive_latest_snapshot()? {
            latest_snapshot = Some(snapshot);
        }

        let players = latest_snapshot
            .as_ref()
            .map_or(&[][..], |snapshot| snapshot.players.as_slice());
        let own_player = players
            .iter()
            .find(|player| player.id == connection.player_id());
        view::draw(&mut canvas, &maze, own_player)?;
        minimap::draw(&mut canvas, &maze, connection.player_id(), players)?;
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
