use std::num::NonZeroU32;

use esp_idf_hal::peripherals::*;
use esp_idf_hal::sys::EspError;
use esp_idf_hal::task::notification::Notification;
use esp_idf_hal::timer::*;

use esp_idf_hal::adc::attenuation::DB_11;
use esp_idf_hal::adc::oneshot::config::AdcChannelConfig;
use esp_idf_hal::adc::oneshot::{AdcChannelDriver, AdcDriver};

const SAMPLES: usize = 200;
const SAMPLE_RATE: u64 = 1000;

fn main() -> Result<(), EspError> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_hal::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;

    // A safer abstraction over FreeRTOS/ESP-IDF task notifications.
    let notification = Notification::new();

    // BaseClock for the Timer is the APB_CLK that is running on 80MHz at default
    // The default clock-divider is -> 80
    // default APB clk is available with the APB_CLK_FREQ constant
    let timer_conf = config::Config::new().auto_reload(true);
    let mut timer = TimerDriver::new(peripherals.timer00, &timer_conf)?;

    // 1kHz Timer
    timer.set_alarm(timer.tick_hz() / SAMPLE_RATE)?;

    let notifier = notification.notifier();

    // Safety: make sure the `Notification` object is not dropped while the subscription is active
    unsafe {
        timer.subscribe(move || {
            let bitset = 0b10001010101;
            notifier.notify_and_yield(NonZeroU32::new(bitset).unwrap());
        })?;
    }

    timer.enable_interrupt()?;
    timer.enable_alarm(true)?;
    timer.enable(true)?;

    // Configure ADC
    let adc = AdcDriver::new(peripherals.adc1)?;

    let config = AdcChannelConfig {
        attenuation: DB_11,
        ..Default::default()
    };

    let mut adc_pin = AdcChannelDriver::new(&adc, peripherals.pins.gpio2, &config)?;
    let mut buf = [0_f64; SAMPLES];

    println!(
        "=== ADC Samples - Sample Rate: {} / Samples: {}",
        SAMPLE_RATE, SAMPLES
    );

    loop {
        // Ignore annoying clippy warning
        #[allow(clippy::needless_range_loop)]
        for i in 0..buf.len() {
            // Wait for notification
            let bitset = notification.wait(esp_idf_hal::delay::BLOCK);
            if let Some(_) = bitset {
                buf[i] = adc.read(&mut adc_pin)? as f64 / 4096_f64;
            }
        }
        println!(
            "{}",
            buf.iter()
                .enumerate()
                .map(|(_i, v)| format!("{:.3}", v))
                .collect::<Vec<_>>()
                .join(","),
        );
    }
}
