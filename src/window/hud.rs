use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;
use std::time::{Duration, Instant};

const GLYPH_WIDTH: i32 = 3;
const GLYPH_HEIGHT: i32 = 5;
const CHAR_SPACING: i32 = 1;

/// Rolling frame rate tracker that updates its displayed FPS once every measurement window.
#[derive(Debug, Clone)]
pub struct FpsTracker {
    window_duration: Duration,
    elapsed_accumulated: Duration,
    frame_count: u32,
    displayed_fps: u32,
    last_update: Option<Instant>,
}

impl FpsTracker {
    pub fn new(window_duration: Duration) -> Self {
        Self {
            window_duration,
            elapsed_accumulated: Duration::ZERO,
            frame_count: 0,
            displayed_fps: 0,
            last_update: None,
        }
    }

    /// Records a rendered frame with the elapsed real time since the previous tick.
    pub fn record_frame(&mut self, now: Instant) {
        if let Some(previous) = self.last_update {
            let elapsed = now.duration_since(previous);
            self.advance(elapsed);
        }
        self.last_update = Some(now);
    }

    /// Pure deterministic update logic taking an elapsed duration.
    pub fn advance(&mut self, elapsed: Duration) {
        self.frame_count = self.frame_count.saturating_add(1);
        self.elapsed_accumulated += elapsed;

        if self.elapsed_accumulated >= self.window_duration {
            let seconds = self.elapsed_accumulated.as_secs_f64();
            if seconds > 0.0 {
                self.displayed_fps = (self.frame_count as f64 / seconds).round() as u32;
            }
            self.frame_count = 0;
            self.elapsed_accumulated = Duration::ZERO;
        }
    }

    pub fn fps(&self) -> u32 {
        self.displayed_fps
    }
}

impl Default for FpsTracker {
    fn default() -> Self {
        Self::new(Duration::from_millis(500))
    }
}

/// Draws an on-screen HUD in the top-right corner showing the current FPS and optional level.
pub fn draw(
    canvas: &mut Canvas<Window>,
    fps: u32,
    level: Option<u8>,
) -> Result<(), String> {
    let (window_width, _) = canvas.output_size()?;
    let scale = 3;
    let margin = 16;

    let mut text_buf = [0_u8; 32];
    let text = format_hud_string(&mut text_buf, fps, level);

    let text_width = (text.len() as i32) * (GLYPH_WIDTH + CHAR_SPACING) * scale;
    let text_height = GLYPH_HEIGHT * scale;
    let padding = 8;

    let box_width = (text_width + padding * 2) as u32;
    let box_height = (text_height + padding * 2) as u32;
    let box_x = (window_width as i32) - (box_width as i32) - margin;
    let box_y = margin;

    // Semi-transparent dark background card
    canvas.set_draw_color(Color::RGBA(10, 15, 20, 210));
    canvas.fill_rect(Rect::new(box_x, box_y, box_width, box_height))?;

    // Subtle border
    canvas.set_draw_color(Color::RGBA(60, 85, 100, 255));
    canvas.draw_rect(Rect::new(box_x, box_y, box_width, box_height))?;

    // High-visibility retro phosphor green text
    let text_color = Color::RGB(80, 240, 120);
    draw_bitmap_text(
        canvas,
        text,
        box_x + padding,
        box_y + padding,
        scale,
        text_color,
    )?;

    Ok(())
}

fn format_hud_string(buffer: &mut [u8; 32], fps: u32, level: Option<u8>) -> &str {
    use std::io::Write;
    let mut cursor = std::io::Cursor::new(&mut buffer[..]);
    if let Some(lvl) = level {
        let _ = write!(cursor, "FPS: {fps}  LVL: {lvl}");
    } else {
        let _ = write!(cursor, "FPS: {fps}");
    }
    let len = cursor.position() as usize;
    std::str::from_utf8(&buffer[..len]).unwrap_or("FPS: 0")
}

/// Renders a string using 3x5 bitmapped font glyphs.
pub fn draw_bitmap_text(
    canvas: &mut Canvas<Window>,
    text: &str,
    start_x: i32,
    start_y: i32,
    scale: i32,
    color: Color,
) -> Result<(), String> {
    canvas.set_draw_color(color);
    let mut cursor_x = start_x;

    for byte in text.bytes() {
        let glyph = glyph_bitmap(byte.to_ascii_uppercase());
        for row in 0..GLYPH_HEIGHT {
            for col in 0..GLYPH_WIDTH {
                let pixel_on = (glyph[row as usize] & (1 << (GLYPH_WIDTH - 1 - col))) != 0;
                if pixel_on {
                    canvas.fill_rect(Rect::new(
                        cursor_x + col * scale,
                        start_y + row * scale,
                        scale as u32,
                        scale as u32,
                    ))?;
                }
            }
        }
        cursor_x += (GLYPH_WIDTH + CHAR_SPACING) * scale;
    }

    Ok(())
}

/// Returns a 5-row bitmask for each character. Bit 2 is left, Bit 0 is right.
const fn glyph_bitmap(c: u8) -> [u8; 5] {
    match c {
        b'0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        b'1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        b'2' => [0b111, 0b001, 0b111, 0b100, 0b111],
        b'3' => [0b111, 0b001, 0b111, 0b001, 0b111],
        b'4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        b'5' => [0b111, 0b100, 0b111, 0b001, 0b111],
        b'6' => [0b111, 0b100, 0b111, 0b101, 0b111],
        b'7' => [0b111, 0b001, 0b001, 0b010, 0b010],
        b'8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        b'9' => [0b111, 0b101, 0b111, 0b001, 0b111],
        b':' => [0b000, 0b010, 0b000, 0b010, 0b000],
        b'F' => [0b111, 0b100, 0b110, 0b100, 0b100],
        b'P' => [0b111, 0b101, 0b111, 0b100, 0b100],
        b'S' => [0b111, 0b100, 0b111, 0b001, 0b111],
        b'L' => [0b100, 0b100, 0b100, 0b100, 0b111],
        b'E' => [0b111, 0b100, 0b110, 0b100, 0b111],
        b'V' => [0b101, 0b101, 0b101, 0b101, 0b010],
        b'-' => [0b000, 0b000, 0b111, 0b000, 0b000],
        _ => [0b000, 0b000, 0b000, 0b000, 0b000],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fps_tracker_calculates_correct_frequency() {
        let mut tracker = FpsTracker::new(Duration::from_millis(500));

        // 30 frames over 500ms -> 60 FPS
        let frame_duration = Duration::from_nanos(500_000_000 / 30 + 1);
        for _ in 0..30 {
            tracker.advance(frame_duration);
        }

        assert_eq!(tracker.fps(), 60);
    }

    #[test]
    fn fps_tracker_remains_zero_before_window_completes() {
        let mut tracker = FpsTracker::new(Duration::from_millis(500));
        tracker.advance(Duration::from_millis(200));
        assert_eq!(tracker.fps(), 0);
    }

    #[test]
    fn hud_string_formats_properly() {
        let mut buf = [0_u8; 32];
        assert_eq!(format_hud_string(&mut buf, 60, None), "FPS: 60");
        assert_eq!(format_hud_string(&mut buf, 120, Some(2)), "FPS: 120  LVL: 2");
    }

    #[test]
    fn glyph_coverage_has_all_digits_and_fps() {
        for digit in b'0'..=b'9' {
            let glyph = glyph_bitmap(digit);
            assert!(glyph.iter().any(|&row| row != 0));
        }
        for letter in [b'F', b'P', b'S', b':', b'L', b'E', b'V'] {
            let glyph = glyph_bitmap(letter);
            assert!(glyph.iter().any(|&row| row != 0));
        }
    }
}
