use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::Color;

const WINDOW_WIDTH: u32 = 960;
const WINDOW_HEIGHT: u32 = 600;

pub fn run(username: &str, player_id: u32, level: u8) -> Result<(), String> {
    let sdl = sdl2::init().map_err(|error| format!("could not initialize SDL: {error}"))?;
    let video = sdl
        .video()
        .map_err(|error| format!("could not initialize video: {error}"))?;
    let title = format!("Maze Wars - {username} - Player {player_id} - Level {level}");
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

        canvas.set_draw_color(Color::RGB(7, 12, 18));
        canvas.clear();
        canvas.present();
    }

    Ok(())
}
