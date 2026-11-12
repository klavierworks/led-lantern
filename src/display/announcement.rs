use std::time::Duration;

use super::font::Font;
use super::frame::{Frame, PANEL_WIDTH, Rgb};
use super::repeated_label::repeated_label_frame;
use super::{SCROLL_PIXELS_PER_SECOND, TEXT_TOP};

const INTRO: Duration = Duration::from_secs(5);
const TICKER: Duration = Duration::from_secs(295);

pub struct Announcement {
    intro: Frame,
    ticker: Option<Ticker>,
}

struct Ticker {
    unit: String,
    unit_width: i64,
}

impl Announcement {
    pub fn new(font: &Font, message: &str) -> Self {
        let ticker = (!message.is_empty()).then(|| {
            let unit = format!("{message}  •  ");
            Ticker {
                unit_width: i64::from(font.text_width(&unit)),
                unit,
            }
        });
        Self {
            intro: repeated_label_frame(font, "ANNOUNCEMENT", Rgb::BLACK, Rgb::WHITE),
            ticker,
        }
    }

    pub fn frame_at(&self, font: &Font, elapsed: Duration) -> Frame {
        let Some(ticker) = &self.ticker else {
            return self.intro.clone();
        };
        let cycle = (INTRO + TICKER).as_secs_f64();
        let elapsed = elapsed.as_secs_f64();
        let completed_cycles = (elapsed / cycle).floor();
        let into_cycle = elapsed - completed_cycles * cycle;
        if into_cycle < INTRO.as_secs_f64() {
            return self.intro.clone();
        }
        let scrolled_seconds =
            completed_cycles * TICKER.as_secs_f64() + into_cycle - INTRO.as_secs_f64();
        ticker.frame_at(font, scrolled_seconds)
    }
}

impl Ticker {
    fn frame_at(&self, font: &Font, scrolled_seconds: f64) -> Frame {
        let offset = (scrolled_seconds * SCROLL_PIXELS_PER_SECOND) as i64;
        let panel_width = PANEL_WIDTH as i64;
        let mut x = panel_width - offset;
        if x <= 0 {
            x = -((offset - panel_width) % self.unit_width);
        }
        let mut frame = Frame::filled(Rgb::BLACK);
        let baseline = TEXT_TOP + font.baseline();
        while x < panel_width {
            font.draw(&mut frame, x as i32, baseline, Rgb::WHITE, &self.unit);
            x += self.unit_width;
        }
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::LANTERN_FONT;

    #[test]
    fn starts_with_intro_then_ticker() {
        let font = Font::parse(LANTERN_FONT).unwrap();
        let announcement = Announcement::new(&font, "hello");
        let intro = announcement.frame_at(&font, Duration::from_secs(1));
        assert_eq!(intro.pixel(0, 0), Rgb::WHITE);
        let ticker = announcement.frame_at(&font, Duration::from_secs(6));
        assert_eq!(ticker.pixel(0, 0), Rgb::BLACK);
    }

    #[test]
    fn intro_returns_every_five_minutes() {
        let font = Font::parse(LANTERN_FONT).unwrap();
        let announcement = Announcement::new(&font, "hello");
        let frame = announcement.frame_at(&font, Duration::from_secs(301));
        assert_eq!(frame.pixel(0, 0), Rgb::WHITE);
    }

    #[test]
    fn empty_message_keeps_intro() {
        let font = Font::parse(LANTERN_FONT).unwrap();
        let announcement = Announcement::new(&font, "");
        let frame = announcement.frame_at(&font, Duration::from_secs(60));
        assert_eq!(frame.pixel(0, 0), Rgb::WHITE);
    }
}
