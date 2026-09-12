use crate::window::hud::{CHAR_SPACING, GLYPH_HEIGHT, GLYPH_WIDTH, draw_bitmap_text};
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;

pub const PAUSE_MENU_ITEMS: [&str; 5] = [
    "RESUME GAME",
    "CROSSHAIR",
    "ENEMIES",
    "RESET ENEMIES",
    "QUIT GAME",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameState {
    Playing,
    Paused { selected_item: usize },
    GameOver,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    None,
    Resume,
    ToggleCrosshair,
    ToggleEnemies,
    ResetEnemies,
    Quit,
}

impl GameState {
    pub fn is_playing(&self) -> bool {
        matches!(self, Self::Playing)
    }

    pub fn is_paused(&self) -> bool {
        matches!(self, Self::Paused { .. })
    }

    pub fn is_game_over(&self) -> bool {
        matches!(self, Self::GameOver)
    }

    pub fn move_selection_up(&mut self) {
        if let Self::Paused { selected_item } = self {
            *selected_item = if *selected_item == 0 {
                PAUSE_MENU_ITEMS.len() - 1
            } else {
                *selected_item - 1
            };
        }
    }

    pub fn move_selection_down(&mut self) {
        if let Self::Paused { selected_item } = self {
            *selected_item = (*selected_item + 1) % PAUSE_MENU_ITEMS.len();
        }
    }

    pub fn activate_selection(&mut self) -> MenuAction {
        if let Self::Paused { selected_item } = *self {
            match selected_item {
                0 => {
                    *self = Self::Playing;
                    MenuAction::Resume
                }
                1 => MenuAction::ToggleCrosshair,
                2 => MenuAction::ToggleEnemies,
                3 => {
                    *self = Self::Playing;
                    MenuAction::ResetEnemies
                }
                4 => MenuAction::Quit,
                _ => MenuAction::None,
            }
        } else {
            MenuAction::None
        }
    }
}

pub fn format_menu_item<'a>(
    buf: &'a mut [u8],
    name: &str,
    index: usize,
    is_selected: bool,
    crosshair_on: bool,
    enemies_on: bool,
) -> &'a str {
    use std::io::Write;
    let mut cursor = std::io::Cursor::new(&mut buf[..]);
    let prefix = if is_selected { "> " } else { "  " };

    match index {
        1 => {
            let state = if crosshair_on { "[ON]" } else { "[OFF]" };
            let _ = write!(cursor, "{prefix}CROSSHAIR: {state}");
        }
        2 => {
            let state = if enemies_on { "[ON]" } else { "[OFF]" };
            let _ = write!(cursor, "{prefix}ENEMIES:   {state}");
        }
        _ => {
            let _ = write!(cursor, "{prefix}{name}");
        }
    }

    let len = cursor.position() as usize;
    std::str::from_utf8(&buf[..len]).unwrap_or("")
}

pub fn draw_pause_menu(
    canvas: &mut Canvas<Window>,
    selected_item: usize,
    crosshair_on: bool,
    enemies_on: bool,
) -> Result<(), String> {
    let (window_width, window_height) = canvas.output_size()?;

    let box_width = 460_u32.min(window_width.saturating_sub(32));
    let box_height = 290_u32.min(window_height.saturating_sub(32));
    let box_x = ((window_width - box_width) / 2) as i32;
    let box_y = ((window_height - box_height) / 2) as i32;

    canvas.set_draw_color(Color::RGBA(6, 10, 14, 230));
    canvas.fill_rect(Rect::new(box_x, box_y, box_width, box_height))?;

    canvas.set_draw_color(Color::RGB(50, 180, 100));
    canvas.draw_rect(Rect::new(box_x, box_y, box_width, box_height))?;
    canvas.set_draw_color(Color::RGB(30, 90, 60));
    canvas.draw_rect(Rect::new(box_x + 4, box_y + 4, box_width - 8, box_height - 8))?;

    let title = "=== PAUSE MENU ===";
    let title_scale = 3;
    let title_w = (title.len() as i32) * (GLYPH_WIDTH + CHAR_SPACING) * title_scale;
    let title_x = box_x + ((box_width as i32 - title_w) / 2).max(10);
    let title_y = box_y + 20;
    draw_bitmap_text(canvas, title, title_x, title_y, title_scale, Color::RGB(80, 255, 130))?;

    let item_scale = 2;
    let item_start_y = title_y + 42;
    let line_height = (GLYPH_HEIGHT + 3) * item_scale + 8;

    for (index, &name) in PAUSE_MENU_ITEMS.iter().enumerate() {
        let is_selected = index == selected_item;
        let item_y = item_start_y + (index as i32) * line_height;

        let mut line_buf = [0_u8; 40];
        let line = format_menu_item(&mut line_buf, name, index, is_selected, crosshair_on, enemies_on);

        let color = if is_selected {
            canvas.set_draw_color(Color::RGBA(40, 80, 50, 180));
            let _ = canvas.fill_rect(Rect::new(
                box_x + 16,
                item_y - 2,
                box_width.saturating_sub(32),
                line_height as u32 - 2,
            ));
            Color::RGB(255, 255, 255)
        } else {
            Color::RGB(160, 200, 170)
        };

        draw_bitmap_text(canvas, line, box_x + 30, item_y, item_scale, color)?;
    }

    let hint = "UP/DOWN: NAVIGATE  ENTER: SELECT  ESC: RESUME";
    let hint_scale = 1;
    let hint_w = (hint.len() as i32) * (GLYPH_WIDTH + CHAR_SPACING) * hint_scale;
    let hint_x = box_x + ((box_width as i32 - hint_w) / 2).max(8);
    let hint_y = box_y + box_height as i32 - 24;
    draw_bitmap_text(canvas, hint, hint_x, hint_y, hint_scale, Color::RGB(100, 150, 120))?;

    Ok(())
}

pub fn draw_game_over_dialog(canvas: &mut Canvas<Window>) -> Result<(), String> {
    let (window_width, window_height) = canvas.output_size()?;

    let box_width = 460_u32.min(window_width.saturating_sub(32));
    let box_height = 240_u32.min(window_height.saturating_sub(32));
    let box_x = ((window_width - box_width) / 2) as i32;
    let box_y = ((window_height - box_height) / 2) as i32;

    canvas.set_draw_color(Color::RGBA(24, 6, 6, 240));
    canvas.fill_rect(Rect::new(box_x, box_y, box_width, box_height))?;

    canvas.set_draw_color(Color::RGB(230, 40, 40));
    canvas.draw_rect(Rect::new(box_x, box_y, box_width, box_height))?;
    canvas.set_draw_color(Color::RGB(130, 20, 20));
    canvas.draw_rect(Rect::new(box_x + 4, box_y + 4, box_width - 8, box_height - 8))?;

    let title = "YOU LOSE!";
    let title_scale = 4;
    let title_w = (title.len() as i32) * (GLYPH_WIDTH + CHAR_SPACING) * title_scale;
    let title_x = box_x + ((box_width as i32 - title_w) / 2).max(10);
    let title_y = box_y + 24;
    draw_bitmap_text(canvas, title, title_x, title_y, title_scale, Color::RGB(255, 60, 60))?;

    let subtitle = "AN ENEMY CAUGHT YOU";
    let sub_scale = 2;
    let sub_w = (subtitle.len() as i32) * (GLYPH_WIDTH + CHAR_SPACING) * sub_scale;
    let sub_x = box_x + ((box_width as i32 - sub_w) / 2).max(10);
    let sub_y = title_y + 35;
    draw_bitmap_text(canvas, subtitle, sub_x, sub_y, sub_scale, Color::RGB(240, 180, 180))?;

    let opt_scale = 2;
    let opt1 = "[R] RETRY";
    let opt2 = "[E] DISABLE ENEMIES";
    let opt3 = "[ESC] MENU";

    let opt_start_y = sub_y + 32;
    let line_h = 24;

    draw_bitmap_text(canvas, opt1, box_x + 60, opt_start_y, opt_scale, Color::RGB(255, 255, 255))?;
    draw_bitmap_text(canvas, opt2, box_x + 60, opt_start_y + line_h, opt_scale, Color::RGB(255, 220, 140))?;
    draw_bitmap_text(canvas, opt3, box_x + 60, opt_start_y + line_h * 2, opt_scale, Color::RGB(180, 200, 220))?;

    Ok(())
}

pub fn draw_maze_cleared_banner(canvas: &mut Canvas<Window>) -> Result<(), String> {
    let (window_width, _) = canvas.output_size()?;

    let box_width = 380_u32.min(window_width.saturating_sub(32));
    let box_height = 54_u32;
    let box_x = ((window_width - box_width) / 2) as i32;
    let box_y = 60_i32;

    canvas.set_draw_color(Color::RGBA(10, 30, 15, 230));
    canvas.fill_rect(Rect::new(box_x, box_y, box_width, box_height))?;

    canvas.set_draw_color(Color::RGB(255, 215, 0));
    canvas.draw_rect(Rect::new(box_x, box_y, box_width, box_height))?;
    canvas.draw_rect(Rect::new(box_x + 2, box_y + 2, box_width - 4, box_height - 4))?;

    let text1 = "MAZE CLEARED!";
    let scale1 = 2;
    let w1 = (text1.len() as i32) * (GLYPH_WIDTH + CHAR_SPACING) * scale1;
    let x1 = box_x + ((box_width as i32 - w1) / 2).max(6);
    draw_bitmap_text(canvas, text1, x1, box_y + 8, scale1, Color::RGB(255, 230, 50))?;

    let text2 = "ALL ENEMIES DEFEATED";
    let scale2 = 1;
    let w2 = (text2.len() as i32) * (GLYPH_WIDTH + CHAR_SPACING) * scale2;
    let x2 = box_x + ((box_width as i32 - w2) / 2).max(6);
    draw_bitmap_text(canvas, text2, x2, box_y + 32, scale2, Color::RGB(180, 255, 200))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_menu_navigation_and_wrapping() {
        let mut state = GameState::Paused { selected_item: 0 };

        state.move_selection_up();
        assert_eq!(state, GameState::Paused { selected_item: 4 });

        state.move_selection_down();
        assert_eq!(state, GameState::Paused { selected_item: 0 });

        state.move_selection_down();
        assert_eq!(state, GameState::Paused { selected_item: 1 });

        state.move_selection_down();
        assert_eq!(state, GameState::Paused { selected_item: 2 });
    }

    #[test]
    fn test_menu_actions_and_activation() {
        let mut state = GameState::Paused { selected_item: 0 };
        assert_eq!(state.activate_selection(), MenuAction::Resume);
        assert_eq!(state, GameState::Playing);

        state = GameState::Paused { selected_item: 1 };
        assert_eq!(state.activate_selection(), MenuAction::ToggleCrosshair);
        assert!(state.is_paused());

        state = GameState::Paused { selected_item: 2 };
        assert_eq!(state.activate_selection(), MenuAction::ToggleEnemies);
        assert!(state.is_paused());

        state = GameState::Paused { selected_item: 3 };
        assert_eq!(state.activate_selection(), MenuAction::ResetEnemies);
        assert_eq!(state, GameState::Playing);

        state = GameState::Paused { selected_item: 4 };
        assert_eq!(state.activate_selection(), MenuAction::Quit);
    }

    #[test]
    fn test_format_menu_item() {
        let mut buf = [0_u8; 40];
        let s1 = format_menu_item(&mut buf, "RESUME GAME", 0, true, false, false);
        assert_eq!(s1, "> RESUME GAME");

        let s2 = format_menu_item(&mut buf, "CROSSHAIR", 1, false, true, false);
        assert_eq!(s2, "  CROSSHAIR: [ON]");

        let s3 = format_menu_item(&mut buf, "CROSSHAIR", 1, false, false, false);
        assert_eq!(s3, "  CROSSHAIR: [OFF]");

        let s4 = format_menu_item(&mut buf, "ENEMIES", 2, true, false, true);
        assert_eq!(s4, "> ENEMIES:   [ON]");

        let s5 = format_menu_item(&mut buf, "ENEMIES", 2, false, false, false);
        assert_eq!(s5, "  ENEMIES:   [OFF]");
    }

    #[test]
    fn test_game_state_predicates() {
        let playing = GameState::Playing;
        assert!(playing.is_playing());
        assert!(!playing.is_paused());
        assert!(!playing.is_game_over());

        let paused = GameState::Paused { selected_item: 1 };
        assert!(!paused.is_playing());
        assert!(paused.is_paused());
        assert!(!paused.is_game_over());

        let game_over = GameState::GameOver;
        assert!(!game_over.is_playing());
        assert!(!game_over.is_paused());
        assert!(game_over.is_game_over());
    }
}
