use super::frame::{Frame, PANEL_HEIGHT, PANEL_WIDTH, Rgb};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileSize {
    pub width: usize,
    pub height: usize,
}

impl TileSize {
    pub fn byte_count(self) -> usize {
        self.width * self.height * 3
    }
}

pub fn fit_tile(source_width: usize, source_height: usize) -> TileSize {
    let scale = (PANEL_WIDTH as f64 / source_width as f64)
        .min(PANEL_HEIGHT as f64 / source_height as f64)
        .min(1.0);
    TileSize {
        width: ((source_width as f64 * scale).round() as usize).max(1),
        height: ((source_height as f64 * scale).round() as usize).max(1),
    }
}

pub fn tile_across_panel(rgb24: &[u8], size: TileSize) -> Frame {
    let mut frame = Frame::filled(Rgb::BLACK);
    for y in 0..PANEL_HEIGHT {
        for x in 0..PANEL_WIDTH {
            let index = ((y % size.height) * size.width + x % size.width) * 3;
            let color = Rgb::new(rgb24[index], rgb24[index + 1], rgb24[index + 2]);
            frame.set(x as i32, y as i32, color);
        }
    }
    frame
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widescreen_video_fits_panel_height() {
        assert_eq!(
            fit_tile(1920, 1080),
            TileSize {
                width: 57,
                height: 32
            }
        );
    }

    #[test]
    fn very_wide_video_fits_panel_width() {
        assert_eq!(
            fit_tile(3840, 160),
            TileSize {
                width: 384,
                height: 16
            }
        );
    }

    #[test]
    fn small_video_is_not_scaled_up() {
        assert_eq!(
            fit_tile(20, 10),
            TileSize {
                width: 20,
                height: 10
            }
        );
    }

    #[test]
    fn tile_repeats_across_panel() {
        let size = TileSize {
            width: 2,
            height: 1,
        };
        let pixels = [255, 0, 0, 0, 0, 255];
        let frame = tile_across_panel(&pixels, size);
        assert_eq!(frame.pixel(0, 0), Rgb::new(255, 0, 0));
        assert_eq!(frame.pixel(1, 5), Rgb::new(0, 0, 255));
        assert_eq!(frame.pixel(382, 31), Rgb::new(255, 0, 0));
    }
}
