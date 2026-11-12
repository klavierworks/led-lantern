use std::ffi::c_int;
use std::ptr::{self, NonNull};

use anyhow::{Context, Result, bail};
use rpi_led_matrix_sys::{
    CLedCanvas, CLedMatrix, CLedMatrixOptions, CLedRuntimeOptions, led_canvas_set_pixel,
    led_matrix_create_from_options_and_rt_options, led_matrix_create_offscreen_canvas,
    led_matrix_delete, led_matrix_swap_on_vsync,
};

use super::frame::{
    Frame, MATRIX_CHAIN_LENGTH, MATRIX_COLUMNS, MATRIX_ROWS, PANEL_HEIGHT, PANEL_WIDTH,
};

pub struct Panel {
    matrix: NonNull<CLedMatrix>,
    canvas: NonNull<CLedCanvas>,
}

impl Panel {
    pub fn open() -> Result<Self> {
        // The C wrapper only copies options that are non-zero, so every zero here means "library default".
        let mut options = CLedMatrixOptions {
            hardware_mapping: c"adafruit-hat-pwm".as_ptr().cast_mut(),
            rows: MATRIX_ROWS as c_int,
            cols: MATRIX_COLUMNS as c_int,
            chain_length: MATRIX_CHAIN_LENGTH as c_int,
            parallel: 0,
            pwm_bits: 0,
            pwm_lsb_nanoseconds: 0,
            pwm_dither_bits: 0,
            brightness: 0,
            scan_mode: 0,
            row_address_type: 0,
            multiplexing: 0,
            led_rgb_sequence: ptr::null_mut(),
            pixel_mapper_config: ptr::null_mut(),
            panel_type: ptr::null_mut(),
            disable_hardware_pulsing: 0,
            show_refresh_rate: 0,
            inverse_colors: 0,
            limit_refresh_rate_hz: 0,
        };
        // -1 is the library's "never drop root privileges"; 0 would be ignored like any other zero.
        let mut runtime_options = CLedRuntimeOptions {
            gpio_slowdown: 0,
            daemon: 0,
            drop_privileges: -1,
            do_gpio_init: true,
        };

        let matrix = unsafe {
            led_matrix_create_from_options_and_rt_options(&mut options, &mut runtime_options)
        };
        let matrix = NonNull::new(matrix).context("opening the LED matrix")?;
        let canvas = unsafe { led_matrix_create_offscreen_canvas(matrix.as_ptr()) };
        let Some(canvas) = NonNull::new(canvas) else {
            unsafe { led_matrix_delete(matrix.as_ptr()) };
            bail!("creating the LED canvas");
        };
        Ok(Self { matrix, canvas })
    }

    pub fn show(&mut self, frame: &Frame) {
        for y in 0..PANEL_HEIGHT {
            for x in 0..PANEL_WIDTH {
                let pixel = frame.pixel(x, y);
                unsafe {
                    led_canvas_set_pixel(
                        self.canvas.as_ptr(),
                        x as c_int,
                        y as c_int,
                        pixel.red,
                        pixel.green,
                        pixel.blue,
                    );
                }
            }
        }
        let previous =
            unsafe { led_matrix_swap_on_vsync(self.matrix.as_ptr(), self.canvas.as_ptr()) };
        if let Some(previous) = NonNull::new(previous) {
            self.canvas = previous;
        }
    }
}

impl Drop for Panel {
    fn drop(&mut self) {
        unsafe { led_matrix_delete(self.matrix.as_ptr()) };
    }
}
