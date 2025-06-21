use esp_idf_hal::gpio::{IOPin, OutputPin};
use esp_idf_svc::hal::prelude::*;

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use doorbell::button_closure;
use doorbell::rgb;
use doorbell::ws2812_quad;

fn rotate<T: Copy, const N: usize>(a: &[T; N]) -> [T; N] {
    let mut b = *a;
    b[..(N - 1)].copy_from_slice(&a[1..N]);
    b[N - 1] = a[0];
    b
}

struct DisplayState<'a> {
    display: ws2812_quad::Ws2812RmtQuad<'a>,
    status: [rgb::Rgb; 4],
}

impl<'a> DisplayState<'a> {
    fn show(&mut self) {
        self.display.set(&self.status).unwrap();
    }
    fn set(&mut self, c: [rgb::Rgb; 4]) {
        self.status = c;
    }
    fn rotate(&mut self) {
        self.status = rotate(&self.status);
    }
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

    let mut display = DisplayState {
        display: ws2812_quad::Ws2812RmtQuad::new(pin, channel, rgb::RgbLayout::Grb)?,
        status: [rgb::OFF; 4],
    };

    display.show();
    display.set([rgb::RED, rgb::GREEN, rgb::BLUE, rgb::OFF]);

    // Spawn button task
    let button_pin = peripherals.pins.gpio21.downgrade();
    let display = Arc::new(Mutex::new(display));

    let _t = {
        let d1 = display.clone();
        let d2 = display.clone();
        thread::spawn(move || {
            button_closure::button_task(
                button_pin,
                Some(|| {
                    log::info!("-- Button Press");
                    {
                        let mut d = d1.lock().unwrap();
                        d.rotate();
                        d.show();
                    }
                }),
                Some(|| {
                    log::info!("-- Long Button Press");
                    {
                        let mut d = d2.lock().unwrap();
                        for _ in 0..20 {
                            d.rotate();
                            d.show();
                            thread::sleep(Duration::from_millis(50))
                        }
                    }
                }),
                Some(Duration::from_secs(2)),
            )
        })
    };

    loop {
        std::thread::sleep(Duration::from_secs(5));
        log::info!("Waiting");
    }
}
