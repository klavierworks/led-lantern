pub const MATRIX_ROWS: usize = 32;
pub const MATRIX_COLUMNS: usize = 64;
pub const MATRIX_CHAIN_LENGTH: usize = 6;
pub const PANEL_WIDTH: usize = MATRIX_COLUMNS * MATRIX_CHAIN_LENGTH;
pub const PANEL_HEIGHT: usize = MATRIX_ROWS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Rgb {
    pub const BLACK: Rgb = Rgb::new(0, 0, 0);
    pub const WHITE: Rgb = Rgb::new(255, 255, 255);

    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pixels: Vec<Rgb>,
}

impl Frame {
    pub fn filled(color: Rgb) -> Self {
        Self {
            pixels: vec![color; PANEL_WIDTH * PANEL_HEIGHT],
        }
    }

    pub fn set(&mut self, x: i32, y: i32, color: Rgb) {
        let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) else {
            return;
        };
        if x >= PANEL_WIDTH || y >= PANEL_HEIGHT {
            return;
        }
        self.pixels[y * PANEL_WIDTH + x] = color;
    }

    pub fn pixel(&self, x: usize, y: usize) -> Rgb {
        self.pixels[y * PANEL_WIDTH + x]
    }
}
