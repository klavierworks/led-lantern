use std::collections::HashMap;

use anyhow::{Context, Result, bail};

use super::frame::{Frame, PANEL_WIDTH, Rgb};

const REPLACEMENT_CHARACTER: char = '\u{FFFD}';

struct Glyph {
    advance: i32,
    height: i32,
    x_offset: i32,
    y_offset: i32,
    row_bits: u32,
    rows: Vec<u32>,
}

pub struct Font {
    glyphs: HashMap<char, Glyph>,
    baseline: i32,
    widest_glyph: i32,
}

pub struct TextLayout {
    characters: Vec<char>,
    positions: Vec<i32>,
    width: i32,
}

impl TextLayout {
    pub fn width(&self) -> i32 {
        self.width
    }
}

#[derive(Default)]
struct GlyphBuilder {
    character: Option<char>,
    advance: i32,
    height: i32,
    x_offset: i32,
    y_offset: i32,
    row_bits: u32,
    rows: Vec<u32>,
    is_reading_bitmap: bool,
}

impl Font {
    pub fn parse(bdf: &str) -> Result<Self> {
        let mut glyphs = HashMap::new();
        let mut baseline = None;
        let mut glyph = GlyphBuilder::default();

        for line in bdf.lines() {
            let mut fields = line.split_whitespace();
            let Some(keyword) = fields.next() else {
                continue;
            };
            let numbers: Vec<i32> = fields.filter_map(|field| field.parse().ok()).collect();

            match keyword {
                "FONTBOUNDINGBOX" => {
                    let [_, height, _, y_offset] = numbers[..] else {
                        bail!("malformed FONTBOUNDINGBOX: {line}");
                    };
                    baseline = Some(height + y_offset);
                }
                "STARTCHAR" => glyph = GlyphBuilder::default(),
                "ENCODING" => {
                    glyph.character = numbers
                        .first()
                        .and_then(|&code| u32::try_from(code).ok())
                        .and_then(char::from_u32);
                }
                "DWIDTH" => glyph.advance = numbers.first().copied().unwrap_or_default(),
                "BBX" => {
                    let [_, height, x_offset, y_offset] = numbers[..] else {
                        bail!("malformed BBX: {line}");
                    };
                    glyph.height = height;
                    glyph.x_offset = x_offset;
                    glyph.y_offset = y_offset;
                }
                "BITMAP" => glyph.is_reading_bitmap = true,
                "ENDCHAR" => {
                    if let Some(character) = glyph.character {
                        glyphs.insert(character, glyph.build());
                    }
                    glyph = GlyphBuilder::default();
                }
                hex if glyph.is_reading_bitmap => {
                    let row = u32::from_str_radix(hex, 16)
                        .with_context(|| format!("malformed bitmap row: {line}"))?;
                    glyph.row_bits = hex.len() as u32 * 4;
                    glyph.rows.push(row);
                }
                _ => {}
            }
        }

        let baseline = baseline.context("font has no FONTBOUNDINGBOX")?;
        Ok(Self {
            widest_glyph: widest_glyph(&glyphs),
            glyphs,
            baseline,
        })
    }

    pub fn baseline(&self) -> i32 {
        self.baseline
    }

    pub fn text_width(&self, text: &str) -> i32 {
        text.chars()
            .filter_map(|character| self.glyph(character))
            .map(|glyph| glyph.advance)
            .sum()
    }

    pub fn layout(&self, text: &str) -> TextLayout {
        let mut characters = Vec::new();
        let mut positions = Vec::new();
        let mut cursor = 0;
        for character in text.chars() {
            let Some(glyph) = self.glyph(character) else {
                continue;
            };
            characters.push(character);
            positions.push(cursor);
            cursor += glyph.advance;
        }
        TextLayout {
            characters,
            positions,
            width: cursor,
        }
    }

    pub fn draw(&self, frame: &mut Frame, x: i32, baseline_y: i32, color: Rgb, text: &str) {
        self.draw_layout(frame, x, baseline_y, color, &self.layout(text));
    }

    pub fn draw_layout(
        &self,
        frame: &mut Frame,
        x: i32,
        baseline_y: i32,
        color: Rgb,
        layout: &TextLayout,
    ) {
        let first_visible = layout
            .positions
            .partition_point(|&position| x + position + self.widest_glyph <= 0);
        let visible = layout.characters[first_visible..]
            .iter()
            .zip(&layout.positions[first_visible..])
            .take_while(|&(_, &position)| x + position - self.widest_glyph < PANEL_WIDTH as i32);
        for (&character, &position) in visible {
            if let Some(glyph) = self.glyph(character) {
                draw_glyph(frame, glyph, x + position, baseline_y, color);
            }
        }
    }

    fn glyph(&self, character: char) -> Option<&Glyph> {
        self.glyphs
            .get(&character)
            .or_else(|| self.glyphs.get(&REPLACEMENT_CHARACTER))
    }
}

fn widest_glyph(glyphs: &HashMap<char, Glyph>) -> i32 {
    glyphs
        .values()
        .map(|glyph| {
            glyph
                .advance
                .max(glyph.x_offset.abs() + glyph.row_bits as i32)
        })
        .max()
        .unwrap_or_default()
}

fn draw_glyph(frame: &mut Frame, glyph: &Glyph, x: i32, baseline_y: i32, color: Rgb) {
    let top = baseline_y - glyph.height - glyph.y_offset;
    for (row_index, row) in glyph.rows.iter().enumerate() {
        for column in 0..glyph.row_bits {
            if row >> (glyph.row_bits - 1 - column) & 1 == 1 {
                frame.set(
                    x + glyph.x_offset + column as i32,
                    top + row_index as i32,
                    color,
                );
            }
        }
    }
}

impl GlyphBuilder {
    fn build(self) -> Glyph {
        Glyph {
            advance: self.advance,
            height: self.height,
            x_offset: self.x_offset,
            y_offset: self.y_offset,
            row_bits: self.row_bits,
            rows: self.rows,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::LANTERN_FONT;

    const BASELINE: i32 = 22;

    #[test]
    fn parses_lantern_font() {
        let font = Font::parse(LANTERN_FONT).unwrap();
        assert_eq!(font.baseline(), 16);
        assert_eq!(font.text_width("ANNOUNCEMENT"), 120);
        assert_eq!(font.text_width("•"), 10);
    }

    #[test]
    fn draws_pixels_for_text() {
        let font = Font::parse(LANTERN_FONT).unwrap();
        let mut frame = Frame::filled(Rgb::BLACK);
        font.draw(&mut frame, 0, BASELINE, Rgb::WHITE, "I");
        let lit = (0..10)
            .flat_map(|x| (0..32).map(move |y| (x, y)))
            .filter(|&(x, y)| frame.pixel(x, y) == Rgb::WHITE)
            .count();
        assert!(lit > 0);
    }

    #[test]
    fn layout_draws_the_same_pixels_as_every_glyph() {
        let font = Font::parse(LANTERN_FONT).unwrap();
        let text = "The quick brown fox jumps over the lazy dog. ".repeat(20);
        let layout = font.layout(&text);
        for x in [200, 0, -1234, -5000] {
            let mut expected = Frame::filled(Rgb::BLACK);
            let mut cursor = x;
            for character in text.chars() {
                let glyph = font.glyph(character).unwrap();
                draw_glyph(&mut expected, glyph, cursor, BASELINE, Rgb::WHITE);
                cursor += glyph.advance;
            }
            let mut actual = Frame::filled(Rgb::BLACK);
            font.draw_layout(&mut actual, x, BASELINE, Rgb::WHITE, &layout);
            assert_eq!(actual, expected);
        }
    }
}
