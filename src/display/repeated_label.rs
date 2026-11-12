use super::TEXT_TOP;
use super::font::Font;
use super::frame::{Frame, PANEL_WIDTH, Rgb};

const DOT: &str = "•";
const MINIMUM_GAP: i32 = 30;

pub fn repeated_label_frame(font: &Font, label: &str, foreground: Rgb, background: Rgb) -> Frame {
    let label_width = font.text_width(label);
    let dot_width = font.text_width(DOT);
    let panel_width = PANEL_WIDTH as i32;
    let repeats = (panel_width / (label_width + MINIMUM_GAP)).max(1);
    let period = panel_width / repeats;
    let baseline = TEXT_TOP + font.baseline();

    let mut frame = Frame::filled(background);
    for repeat in 0..repeats {
        let x = repeat * period;
        font.draw(&mut frame, x, baseline, foreground, label);
        let dot_x = x + label_width + (period - label_width - dot_width) / 2;
        font.draw(&mut frame, dot_x, baseline, foreground, DOT);
    }
    frame
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::LANTERN_FONT;

    #[test]
    fn repeats_label_around_the_panel() {
        let font = Font::parse(LANTERN_FONT).unwrap();
        let frame = repeated_label_frame(&font, " 42%", Rgb::WHITE, Rgb::BLACK);
        let lit_columns = (0..PANEL_WIDTH)
            .filter(|&x| (0..32).any(|y| frame.pixel(x, y) == Rgb::WHITE))
            .count();
        assert!(lit_columns > 100);
    }
}
