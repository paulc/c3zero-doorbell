use esp_idf_hal::gpio::OutputPin;
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_hal::task::watchdog::{TWDTConfig, TWDTDriver};

use std::time::Duration;

use doorbell::rgb;
use doorbell::ws2812;

fn main() -> anyhow::Result<()> {
    esp_idf_hal::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();
    log::info!("Started...");

    // Initialise peripherals
    let peripherals = Peripherals::take()?;

    // Hardware Watchdog
    let twdt_config = TWDTConfig {
        duration: Duration::from_secs(10),
        panic_on_trigger: true,
        subscribed_idle_tasks: enumset::enum_set!(esp_idf_hal::cpu::Core::Core0),
    };
    let mut twdt_driver = TWDTDriver::new(peripherals.twdt, &twdt_config)?;

    // Status display (C3-Zero onboard WS2812 LED pin = GPIO10)
    let ws2812 = peripherals.pins.gpio10.downgrade_output();
    let channel = peripherals.rmt.channel0;
    let mut status = ws2812::Ws2812RmtSingle::new(ws2812, channel, rgb::RgbLayout::Rgb)?;
    status.set(rgb::OFF)?;

    // Dont configure watchdog until we have setup background tasks
    let mut watchdog = twdt_driver.watch_current_task()?;
    let mut count = 0_usize;

    loop {
        std::thread::sleep(Duration::from_secs(1));
        status.set(rgb::BLUE)?;
        status.set(rgb::OFF)?;
        println!("Counter: {count}",);
        count += 1;
        // Update watchdog
        watchdog.feed()?
    }
}
