use esp_idf_hal::gpio::OutputPin;
use esp_idf_svc::hal::prelude::*;

use std::time::Duration;

use doorbell::rgb;
use doorbell::ws2812_quad;

fn rotate<T: Copy, const N: usize>(a: &mut [T; N]) {
    let start = a[0];
    for i in 0..(N - 1) {
        a[i] = a[i + 1];
    }
    a[N - 1] = start;
}

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    let peripherals = Peripherals::take()?;

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();
    log::info!("Starting...");

    // ws2812_quad display on gpio0
    let pin = peripherals.pins.gpio0.downgrade_output();
    let channel = peripherals.rmt.channel0;
    let mut quad = ws2812_quad::Ws2812RmtQuad::new(pin, channel, rgb::RgbLayout::Grb)?;
    quad.set(&[rgb::OFF; 4])?;

    let mut status = [rgb::RED, rgb::GREEN, rgb::BLUE, rgb::OFF];

    loop {
        log::info!(">> {:?}", status);
        quad.set(&status)?;
        rotate(&mut status);
        std::thread::sleep(Duration::from_millis(200));
    }
}
