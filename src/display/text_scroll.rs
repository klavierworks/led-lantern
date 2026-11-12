use std::time::Duration;

use super::font::Font;
use super::frame::{Frame, PANEL_WIDTH, Rgb};
use super::{SCROLL_PIXELS_PER_SECOND, TEXT_TOP};

pub struct TextScroll {
    text: String,
    pass_length: i64,
}

impl TextScroll {
    pub fn new(font: &Font, text: &str) -> Self {
        let text = format!("{text} ");
        let pass_length = PANEL_WIDTH as i64 + i64::from(font.text_width(&text));
        Self { text, pass_length }
    }

    pub fn passes_completed(&self, elapsed: Duration) -> u64 {
        (scrolled_pixels(elapsed) / self.pass_length) as u64
    }

    pub fn frame_at(&self, font: &Font, elapsed: Duration) -> Frame {
        let offset = scrolled_pixels(elapsed) % self.pass_length;
        let x = PANEL_WIDTH as i64 - offset;
        let mut frame = Frame::filled(Rgb::BLACK);
        font.draw(
            &mut frame,
            x as i32,
            TEXT_TOP + font.baseline(),
            Rgb::new(255, 128, 0),
            &self.text,
        );
        frame
    }
}

fn scrolled_pixels(elapsed: Duration) -> i64 {
    (elapsed.as_secs_f64() * SCROLL_PIXELS_PER_SECOND) as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::LANTERN_FONT;

    #[test]
    fn one_pass_crosses_panel_and_text() {
        let font = Font::parse(LANTERN_FONT).unwrap();
        let scroll = TextScroll::new(&font, "hi");
        let pass_seconds = (PANEL_WIDTH as f64 + 30.0) / SCROLL_PIXELS_PER_SECOND;
        assert_eq!(
            scroll.passes_completed(Duration::from_secs_f64(pass_seconds - 0.1)),
            0
        );
        assert_eq!(
            scroll.passes_completed(Duration::from_secs_f64(pass_seconds + 0.1)),
            1
        );
    }

    #[test]
    fn text_starts_off_panel_to_the_right() {
        let font = Font::parse(LANTERN_FONT).unwrap();
        let scroll = TextScroll::new(&font, "hi");
        assert_eq!(
            scroll.frame_at(&font, Duration::ZERO),
            Frame::filled(Rgb::BLACK)
        );
    }
}
