mod announcement;
mod font;
mod frame;
mod panel;
pub mod renderer;
mod repeated_label;
mod text_scroll;
mod tiling;
mod video;

const LANTERN_FONT: &str = include_str!("../assets/fonts/10x20.bdf");
const TEXT_TOP: i32 = 6;
const SCROLL_PIXELS_PER_SECOND: f64 = 44.0;
