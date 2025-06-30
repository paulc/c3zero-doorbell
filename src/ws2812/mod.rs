mod rgb;
mod ws2812_quad;
mod ws2812_single;

pub use rgb::{colour, Rgb, RgbLayout};
pub use ws2812_quad::Ws2812RmtQuad;
pub use ws2812_single::Ws2812RmtSingle;

// ws2812 timings
const T0H: u64 = 400;
const T0L: u64 = 850;
const T1H: u64 = 800;
const T1L: u64 = 450;
const _RESET: u64 = 50000;

type Ws2812RmtChannel = esp_idf_hal::rmt::CHANNEL0;
