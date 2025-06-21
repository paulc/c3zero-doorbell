use esp_idf_hal::gpio::{IOPin, OutputPin};
use esp_idf_svc::hal::prelude::*;

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use doorbell::button;
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
    let pin = peripherals.pins.gpio6.downgrade_output();
    let channel = peripherals.rmt.channel0;
    let mut quad = ws2812_quad::Ws2812RmtQuad::new(pin, channel, rgb::RgbLayout::Grb)?;
    quad.set(&[rgb::OFF; 4])?;

    // Spawn button task
    let button_pin = peripherals.pins.gpio21.downgrade();
    let (button_tx, button_rx) = mpsc::channel();
    let _t = thread::spawn(move || {
        button::button_task(button_pin, button_tx, Some(Duration::from_secs(2)))
    });

    let mut status = [rgb::OFF; 4];
    quad.set(&status)?;
    status = [rgb::RED, rgb::GREEN, rgb::BLUE, rgb::OFF];

    loop {
        if let Ok(msg) = button_rx.recv_timeout(Duration::from_secs(2)) {
            log::info!(">> button_rx: {msg:?}");
            match msg {
                button::ButtonMessage::Short => {
                    rotate(&mut status);
                    quad.set(&status)?;
                }
                button::ButtonMessage::Long => {
                    for _ in 0..20 {
                        rotate(&mut status);
                        quad.set(&status)?;
                        std::thread::sleep(Duration::from_millis(50));
                    }
                }
            }
        } else {
            log::info!("Waiting");
        }
    }
}
