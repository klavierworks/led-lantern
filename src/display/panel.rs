use anyhow::{Result, anyhow};
use rpi_led_matrix::{LedCanvas, LedColor, LedMatrix, LedMatrixOptions, LedRuntimeOptions};

use super::frame::{
    Frame, MATRIX_CHAIN_LENGTH, MATRIX_COLUMNS, MATRIX_ROWS, PANEL_HEIGHT, PANEL_WIDTH,
};

pub struct Panel {
    matrix: LedMatrix,
    canvas: Option<LedCanvas>,
}

impl Panel {
    pub fn open() -> Result<Self> {
        let mut options = LedMatrixOptions::new();
        options.set_rows(MATRIX_ROWS as u32);
        options.set_cols(MATRIX_COLUMNS as u32);
        options.set_chain_length(MATRIX_CHAIN_LENGTH as u32);
        options.set_hardware_mapping("adafruit-hat-pwm");

        let mut runtime_options = LedRuntimeOptions::new();
        runtime_options.set_drop_privileges(false);

        let matrix = LedMatrix::new(Some(options), Some(runtime_options))
            .map_err(|error| anyhow!("opening the LED matrix: {error}"))?;
        let canvas = matrix.offscreen_canvas();
        Ok(Self {
            matrix,
            canvas: Some(canvas),
        })
    }

    pub fn show(&mut self, frame: &Frame) {
        let Some(mut canvas) = self.canvas.take() else {
            return;
        };
        for y in 0..PANEL_HEIGHT {
            for x in 0..PANEL_WIDTH {
                let pixel = frame.pixel(x, y);
                let color = LedColor {
                    red: pixel.red,
                    green: pixel.green,
                    blue: pixel.blue,
                };
                canvas.set(x as i32, y as i32, &color);
            }
        }
        self.canvas = Some(self.matrix.swap(canvas));
    }
}
