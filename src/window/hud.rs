use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;
use std::time::{Duration, Instant};

pub const GLYPH_WIDTH: i32 = 3;
pub const GLYPH_HEIGHT: i32 = 5;
pub const CHAR_SPACING: i32 = 1;

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
    /// The first call only arms the timestamp, so [`Self::fps`] stays `0` until
    /// the first measurement window completes (startup shows `FPS: 0` by design).
    /// Out-of-order timestamps never panic (saturating, zero elapsed).
    pub fn record_frame(&mut self, now: Instant) {
        if let Some(previous) = self.last_update {
            let elapsed = now.saturating_duration_since(previous);
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
            // Carry the overshoot remainder into the next window instead of
            // discarding it, keeping the long-term rate stable across boundaries.
            self.elapsed_accumulated -= self.window_duration;
        }
    }

    /// Returns the last completed window's FPS.
    /// Returns `0` until the first measurement window completes (see [`Self::record_frame`]).
    pub fn fps(&self) -> u32 {
        self.displayed_fps
    }
}

impl Default for FpsTracker {
    fn default() -> Self {
        Self::new(Duration::from_millis(500))
    }
}

/// A player entry displayed on the multiplayer scoreboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScoreboardEntry<'a> {
    pub id: u32,
    pub username: Option<&'a str>,
    pub score: i32,
}

impl<'a> ScoreboardEntry<'a> {
    pub const fn new(id: u32, username: Option<&'a str>, score: i32) -> Self {
        Self {
            id,
            username,
            score,
        }
    }
}

/// Sorts scoreboard entries descending by score, breaking ties by player ID (ascending).
/// Uses in-place unstable sorting to guarantee zero heap allocations.
pub fn sort_scoreboard(entries: &mut [ScoreboardEntry<'_>]) {
    entries.sort_unstable_by(|a, b| b.score.cmp(&a.score).then_with(|| a.id.cmp(&b.id)));
}

/// Decimal digit count of an `i32` (including `-` sign), without allocation.
fn decimal_len_i32(value: i32) -> usize {
    if value == 0 {
        return 1;
    }
    let mut len = 0_usize;
    let mut n = value as i64;
    if n < 0 {
        len += 1;
        n = -n;
    }
    while n > 0 {
        len += 1;
        n /= 10;
    }
    len
}

/// Formats a scoreboard line as `USERNAME: SCORE` (or `P<id>: SCORE` if unknown),
/// highlighting the local player with a `* ` prefix when `is_local` is true.
/// Overlong names are truncated with a `...` ellipsis so the result always fits
/// `buffer` instead of being silently cut mid-score. Stack buffer only, no allocation.
pub fn format_scoreboard_entry<'a>(
    buffer: &'a mut [u8],
    entry: &ScoreboardEntry<'_>,
    is_local: bool,
) -> &'a str {
    use std::io::Write;
    let capacity = buffer.len();
    let mut cursor = std::io::Cursor::new(&mut buffer[..]);
    let prefix = if is_local { "* " } else { "" };
    match entry.username {
        Some(name) if !name.trim().is_empty() => {
            let name = name.trim();
            let prefix_len = prefix.len();
            let overhead = prefix_len + 2 + decimal_len_i32(entry.score);
            if name.len() + overhead <= capacity {
                let _ = write!(cursor, "{prefix}{}: {}", name, entry.score);
            } else if let Some(avail) = capacity.checked_sub(overhead)
                && avail > 3
            {
                // Truncate the name (not the score) and mark it with an ellipsis.
                let mut trunc = avail - 3;
                while trunc > 0 && !name.is_char_boundary(trunc) {
                    trunc -= 1;
                }
                let short = &name[..trunc];
                let _ = write!(cursor, "{prefix}{short}...: {}", entry.score);
            } else {
                // Buffer too small even for an ellipsis; best-effort truncated write.
                let _ = write!(cursor, "{prefix}{}: {}", name, entry.score);
            }
        }
        _ => {
            let _ = write!(cursor, "{prefix}P{}: {}", entry.id, entry.score);
        }
    }
    let len = cursor.position() as usize;
    std::str::from_utf8(&buffer[..len]).unwrap_or("")
}

/// Formats the top-right HUD text: FPS and optional LEVEL.
pub fn format_hud_string(buffer: &mut [u8], fps: u32, level: Option<u8>) -> &str {
    use std::io::Write;
    let mut cursor = std::io::Cursor::new(&mut buffer[..]);
    if let Some(lvl) = level {
        let _ = write!(cursor, "FPS: {fps}  LEVEL: {lvl}");
    } else {
        let _ = write!(cursor, "FPS: {fps}");
    }
    let len = cursor.position() as usize;
    std::str::from_utf8(&buffer[..len]).unwrap_or("FPS: 0")
}

/// Draws an on-screen HUD with the FPS/level counter and the multiplayer scoreboard.
pub fn draw(
    canvas: &mut Canvas<Window>,
    fps: u32,
    level: Option<u8>,
    local_player_id: Option<u32>,
    scoreboard: &[ScoreboardEntry<'_>],
) -> Result<(), String> {
    draw_fps_and_level(canvas, fps, level)?;
    draw_scoreboard(canvas, local_player_id, scoreboard)?;
    Ok(())
}

/// Draws the FPS and level counter in the top-right corner.
pub fn draw_fps_and_level(
    canvas: &mut Canvas<Window>,
    fps: u32,
    level: Option<u8>,
) -> Result<(), String> {
    let (window_width, _) = canvas.output_size()?;
    let scale = 3;
    let margin = 16;

    let mut text_buf = [0_u8; 48];
    let text = format_hud_string(&mut text_buf, fps, level);

    let text_width = (text.len() as i32) * (GLYPH_WIDTH + CHAR_SPACING) * scale;
    let text_height = GLYPH_HEIGHT * scale;
    let padding = 8;

    // Clamp the card to the window so narrow windows never overflow.
    let max_box_width = window_width.saturating_sub((margin * 2) as u32).max(1);
    let box_width = ((text_width + padding * 2) as u32).min(max_box_width);
    let box_height = (text_height + padding * 2) as u32;
    let box_x = ((window_width as i32) - (box_width as i32) - margin).max(0);
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

/// Maximum number of players displayed on the compact HUD scoreboard panel.
pub const MAX_DISPLAYED_SCOREBOARD_PLAYERS: usize = 10;

/// Draws a compact multiplayer scoreboard panel in the top-center of the screen.
pub fn draw_scoreboard(
    canvas: &mut Canvas<Window>,
    local_player_id: Option<u32>,
    scoreboard: &[ScoreboardEntry<'_>],
) -> Result<(), String> {
    if scoreboard.is_empty() {
        return Ok(());
    }

    let (window_width, _) = canvas.output_size()?;
    let scale = 2;
    let margin = 16;
    let padding = 8;
    let line_spacing = 3;
    let line_height = GLYPH_HEIGHT * scale + line_spacing;

    // Copy up to 64 entries onto the stack and sort without heap allocation
    let mut entries = [ScoreboardEntry::new(0, None, 0); 64];
    let count = scoreboard.len().min(64);
    entries[..count].copy_from_slice(&scoreboard[..count]);
    let active = &mut entries[..count];
    sort_scoreboard(active);

    // Limit display to at most 10 players
    let display_count = active.len().min(MAX_DISPLAYED_SCOREBOARD_PLAYERS);
    let mut display_entries = [ScoreboardEntry::new(0, None, 0); MAX_DISPLAYED_SCOREBOARD_PLAYERS];
    display_entries[..display_count].copy_from_slice(&active[..display_count]);

    // If local player is connected and ranked beyond 10, pin local player to the last slot
    if let Some(local_id) = local_player_id {
        let is_in_display = display_entries[..display_count]
            .iter()
            .any(|entry| entry.id == local_id);
        if !is_in_display
            && let Some(local_entry) = active.iter().find(|entry| entry.id == local_id)
            && display_count > 0
        {
            display_entries[display_count - 1] = *local_entry;
        }
    }

    // Format lines to determine card dimensions without heap allocations
    let mut line_buffers = [[0_u8; 48]; MAX_DISPLAYED_SCOREBOARD_PLAYERS];
    let mut line_lens = [0_usize; MAX_DISPLAYED_SCOREBOARD_PLAYERS];
    let mut max_chars = 0_usize;

    for i in 0..display_count {
        let entry = &display_entries[i];
        let is_local = local_player_id == Some(entry.id);
        let text = format_scoreboard_entry(&mut line_buffers[i], entry, is_local);
        line_lens[i] = text.len();
        if text.len() > max_chars {
            max_chars = text.len();
        }
    }

    let text_width = (max_chars as i32) * (GLYPH_WIDTH + CHAR_SPACING) * scale;
    // Clip the card to the window so overlong entries can never overflow it.
    let max_box_width = window_width.saturating_sub((margin * 2) as u32).max(1);
    let box_width = ((text_width + padding * 2) as u32).min(max_box_width);
    let content_height = (display_count as i32) * line_height - line_spacing;
    let box_height = (content_height + padding * 2) as u32;

    // Centered at the top, leaving central first-person view completely unobstructed.
    // Overlap guard: on narrow windows the centered scoreboard would run into the
    // top-right FPS card, so shift it left to stay clear, and if horizontal room
    // is exhausted, drop it below the FPS card instead of overlapping it.
    let centered_x = ((window_width as i32 - box_width as i32) / 2).max(margin);
    // Conservative width reserved for the top-right FPS/level card plus a gap.
    let fps_reserve = 16 * (GLYPH_WIDTH + CHAR_SPACING) * 3 + 16 + margin + 8;
    let rightmost_x = (window_width as i32 - margin - fps_reserve - box_width as i32).max(margin);
    let box_x = centered_x.min(rightmost_x);
    let mut box_y = margin;
    let fps_card_height = GLYPH_HEIGHT * 3 + padding * 2;
    if box_x + box_width as i32 > window_width as i32 - margin - fps_reserve
        && rightmost_x <= margin
    {
        // No horizontal room left even when left-aligned: stack below FPS card.
        box_y = margin + fps_card_height + 8;
    }

    // Semi-transparent dark background card
    canvas.set_draw_color(Color::RGBA(10, 15, 20, 210));
    canvas.fill_rect(Rect::new(box_x, box_y, box_width, box_height))?;

    // Subtle border
    canvas.set_draw_color(Color::RGBA(60, 85, 100, 255));
    canvas.draw_rect(Rect::new(box_x, box_y, box_width, box_height))?;

    // Render each player row
    let mut cursor_y = box_y + padding;
    for i in 0..display_count {
        let entry = &display_entries[i];
        let is_local = local_player_id == Some(entry.id);
        let len = line_lens[i];
        if let Ok(text) = std::str::from_utf8(&line_buffers[i][..len]) {
            let text_color = if is_local {
                Color::RGB(255, 230, 80) // Distinct warm gold highlight for local player
            } else {
                Color::RGB(200, 220, 230) // Clean light cyan-white for opponents
            };
            draw_bitmap_text(canvas, text, box_x + padding, cursor_y, scale, text_color)?;
        }
        cursor_y += line_height;
    }

    Ok(())
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
        for (row, &row_bits) in glyph.iter().enumerate() {
            for col in 0..GLYPH_WIDTH {
                let pixel_on = (row_bits & (1 << (GLYPH_WIDTH - 1 - col))) != 0;
                if pixel_on {
                    canvas.fill_rect(Rect::new(
                        cursor_x + col * scale,
                        start_y + (row as i32) * scale,
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
pub const fn glyph_bitmap(c: u8) -> [u8; 5] {
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

        b'A' => [0b010, 0b101, 0b111, 0b101, 0b101],
        b'B' => [0b110, 0b101, 0b110, 0b101, 0b110],
        b'C' => [0b111, 0b100, 0b100, 0b100, 0b111],
        b'D' => [0b110, 0b101, 0b101, 0b101, 0b110],
        b'E' => [0b111, 0b100, 0b110, 0b100, 0b111],
        b'F' => [0b111, 0b100, 0b110, 0b100, 0b100],
        b'G' => [0b111, 0b100, 0b101, 0b101, 0b111],
        b'H' => [0b101, 0b101, 0b111, 0b101, 0b101],
        b'I' => [0b111, 0b010, 0b010, 0b010, 0b111],
        b'J' => [0b001, 0b001, 0b001, 0b101, 0b010],
        b'K' => [0b101, 0b110, 0b100, 0b110, 0b101],
        b'L' => [0b100, 0b100, 0b100, 0b100, 0b111],
        b'M' => [0b101, 0b111, 0b101, 0b101, 0b101],
        b'N' => [0b110, 0b101, 0b101, 0b101, 0b101],
        b'O' => [0b111, 0b101, 0b101, 0b101, 0b111],
        b'P' => [0b111, 0b101, 0b111, 0b100, 0b100],
        b'Q' => [0b111, 0b101, 0b101, 0b111, 0b001],
        b'R' => [0b111, 0b101, 0b110, 0b101, 0b101],
        b'S' => [0b111, 0b100, 0b111, 0b001, 0b111],
        b'T' => [0b111, 0b010, 0b010, 0b010, 0b010],
        b'U' => [0b101, 0b101, 0b101, 0b101, 0b111],
        b'V' => [0b101, 0b101, 0b101, 0b101, 0b010],
        b'W' => [0b101, 0b101, 0b101, 0b111, 0b101],
        b'X' => [0b101, 0b101, 0b010, 0b101, 0b101],
        b'Y' => [0b101, 0b101, 0b010, 0b010, 0b010],
        b'Z' => [0b111, 0b001, 0b010, 0b100, 0b111],

        b':' => [0b000, 0b010, 0b000, 0b010, 0b000],
        b'-' => [0b000, 0b000, 0b111, 0b000, 0b000],
        b'+' => [0b000, 0b010, 0b111, 0b010, 0b000],
        b' ' => [0b000, 0b000, 0b000, 0b000, 0b000],
        b'*' => [0b000, 0b101, 0b111, 0b101, 0b000],
        b'[' => [0b110, 0b100, 0b100, 0b100, 0b110],
        b']' => [0b011, 0b001, 0b001, 0b001, 0b011],
        // Legal username punctuation: trivial 3x5 glyphs so names stay readable.
        b'_' => [0b000, 0b000, 0b000, 0b000, 0b111],
        b'.' => [0b000, 0b000, 0b000, 0b000, 0b010],
        b'/' => [0b001, 0b001, 0b010, 0b100, 0b100],
        b',' => [0b000, 0b000, 0b000, 0b010, 0b100],
        b'!' => [0b010, 0b010, 0b010, 0b000, 0b010],
        b'?' => [0b111, 0b001, 0b010, 0b000, 0b010],
        b'>' => [0b100, 0b010, 0b001, 0b010, 0b100],
        b'<' => [0b001, 0b010, 0b100, 0b010, 0b001],
        b'=' => [0b000, 0b111, 0b000, 0b111, 0b000],

        // Visible placeholder (never blank) so any other legal character
        // still renders instead of silently advancing the cursor.
        _ => [0b111, 0b001, 0b010, 0b000, 0b010],
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
    fn fps_tracker_updates_across_multiple_windows() {
        let mut tracker = FpsTracker::new(Duration::from_millis(500));
        let step_60 = Duration::from_nanos(500_000_000 / 30 + 1);
        for _ in 0..30 {
            tracker.advance(step_60);
        }
        assert_eq!(tracker.fps(), 60);

        let step_120 = Duration::from_nanos(500_000_000 / 60 + 1);
        for _ in 0..60 {
            tracker.advance(step_120);
        }
        assert_eq!(tracker.fps(), 120);
    }

    #[test]
    fn hud_string_formats_properly() {
        let mut buf = [0_u8; 48];
        assert_eq!(format_hud_string(&mut buf, 60, None), "FPS: 60");
        assert_eq!(
            format_hud_string(&mut buf, 120, Some(2)),
            "FPS: 120  LEVEL: 2"
        );
    }

    #[test]
    fn scoreboard_string_formats_with_name_and_id_fallback() {
        let mut buf = [0_u8; 48];

        let entry_with_name = ScoreboardEntry::new(1, Some("ALICE"), 10);
        assert_eq!(
            format_scoreboard_entry(&mut buf, &entry_with_name, false),
            "ALICE: 10"
        );

        let entry_unnamed = ScoreboardEntry::new(4, None, -5);
        assert_eq!(
            format_scoreboard_entry(&mut buf, &entry_unnamed, false),
            "P4: -5"
        );

        let entry_empty_name = ScoreboardEntry::new(2, Some("   "), 0);
        assert_eq!(
            format_scoreboard_entry(&mut buf, &entry_empty_name, false),
            "P2: 0"
        );
    }

    #[test]
    fn scoreboard_string_highlights_local_player() {
        let mut buf = [0_u8; 48];

        let entry = ScoreboardEntry::new(1, Some("BOB"), 20);
        let formatted = format_scoreboard_entry(&mut buf, &entry, true);
        assert_eq!(formatted, "* BOB: 20");
        assert!(formatted.starts_with('*'));

        let unnamed_local = ScoreboardEntry::new(7, None, 15);
        let formatted_unnamed = format_scoreboard_entry(&mut buf, &unnamed_local, true);
        assert_eq!(formatted_unnamed, "* P7: 15");
        assert!(formatted_unnamed.starts_with('*'));
    }

    #[test]
    fn scoreboard_sorting_orders_by_score_descending_and_breaks_ties_by_id() {
        let mut entries = [
            ScoreboardEntry::new(5, Some("EVE"), 0),
            ScoreboardEntry::new(2, Some("BOB"), 10),
            ScoreboardEntry::new(1, Some("ALICE"), 10),
            ScoreboardEntry::new(4, Some("DAVE"), -5),
            ScoreboardEntry::new(3, Some("CHARLIE"), 25),
        ];

        sort_scoreboard(&mut entries);

        // Highest score first: 25 (id 3)
        assert_eq!(entries[0].id, 3);
        assert_eq!(entries[0].score, 25);

        // Tie at score 10: lower id first -> id 1, then id 2
        assert_eq!(entries[1].id, 1);
        assert_eq!(entries[1].score, 10);
        assert_eq!(entries[2].id, 2);
        assert_eq!(entries[2].score, 10);

        // Score 0 (id 5)
        assert_eq!(entries[3].id, 5);
        assert_eq!(entries[3].score, 0);

        // Negative score -5 (id 4)
        assert_eq!(entries[4].id, 4);
        assert_eq!(entries[4].score, -5);
    }

    #[test]
    fn glyph_coverage_has_all_digits_letters_and_symbols() {
        // Digits 0-9
        for digit in b'0'..=b'9' {
            let glyph = glyph_bitmap(digit);
            assert!(
                glyph.iter().any(|&row| row != 0),
                "glyph for digit '{}' is empty",
                digit as char
            );
        }

        // Letters A-Z
        for letter in b'A'..=b'Z' {
            let glyph = glyph_bitmap(letter);
            assert!(
                glyph.iter().any(|&row| row != 0),
                "glyph for letter '{}' is empty",
                letter as char
            );
        }

        // Symbols: colon, hyphen, plus, asterisk, brackets []
        for &sym in b":-+*[]" {
            let glyph = glyph_bitmap(sym);
            assert!(
                glyph.iter().any(|&row| row != 0),
                "glyph for symbol '{}' is empty",
                sym as char
            );
        }

        // Space should be completely empty (all rows 0)
        let space_glyph = glyph_bitmap(b' ');
        assert!(space_glyph.iter().all(|&row| row == 0));
    }
}
