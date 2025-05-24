use std::num::NonZeroU32;

use esp_idf_hal::peripherals::*;
use esp_idf_hal::sys::EspError;
use esp_idf_hal::task::notification::Notification;
use esp_idf_hal::timer::*;

use esp_idf_hal::adc::attenuation::DB_11;
use esp_idf_hal::adc::oneshot::config::AdcChannelConfig;
use esp_idf_hal::adc::oneshot::{AdcChannelDriver, AdcDriver};

const ADC_BUFFER_LEN: usize = 100;
const THRESHOLD_BUFFER: usize = 5;

fn stats(buf: &[f64; ADC_BUFFER_LEN]) -> (f64, f64) {
    let mean = buf.iter().sum::<f64>() / ADC_BUFFER_LEN as f64;
    let var = buf
        .iter()
        .map(|v| {
            let diff = mean - *v;
            diff * diff
        })
        .sum::<f64>();
    let var = var / ADC_BUFFER_LEN as f64;
    (mean, var.sqrt())
}

fn main() -> Result<(), EspError> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_hal::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();
    log::info!("Starting...");

    let peripherals = Peripherals::take()?;

    // A safer abstraction over FreeRTOS/ESP-IDF task notifications.
    let notification = Notification::new();

    // BaseClock for the Timer is the APB_CLK that is running on 80MHz at default
    // The default clock-divider is -> 80
    // default APB clk is available with the APB_CLK_FREQ constant
    let timer_conf = config::Config::new().auto_reload(true);
    let mut timer = TimerDriver::new(peripherals.timer00, &timer_conf)?;

    // 1kHz Timer
    timer.set_alarm(timer.tick_hz() / 1000)?;

    let notifier = notification.notifier();

    // Saftey: make sure the `Notification` object is not dropped while the subscription is active
    unsafe {
        timer.subscribe(move || {
            let bitset = 0b10001010101;
            notifier.notify_and_yield(NonZeroU32::new(bitset).unwrap());
        })?;
    }

    timer.enable_interrupt()?;
    timer.enable_alarm(true)?;
    timer.enable(true)?;

    println!("APB_CLK_FREQ: {:?}", esp_idf_hal::sys::APB_CLK_FREQ);
    println!("Timer: {:?}", timer.tick_hz());

    // Configure ADC
    let adc = AdcDriver::new(peripherals.adc1)?;

    let config = AdcChannelConfig {
        attenuation: DB_11,
        ..Default::default()
    };

    let mut adc_pin = AdcChannelDriver::new(&adc, peripherals.pins.gpio2, &config)?;
    let mut buf = [0_f64; ADC_BUFFER_LEN];
    let mut prev = [1.0_f64; THRESHOLD_BUFFER];
    let mut count = 0_usize;

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
        let (mean, stddev) = stats(&buf);
        let threshold = prev.iter().sum::<f64>() / prev.len() as f64;

        // Trigger if stddev > 2.5 * threshold
        let ring = stddev > threshold * 2.5_f64;

        if !ring {
            for i in 0..(THRESHOLD_BUFFER - 1) {
                prev[i] = prev[i + 1];
            }
            prev[THRESHOLD_BUFFER - 1] = stddev;
        }
        println!(
            "[{count}] Mean: {mean:.4} :: Std Dev: {stddev:.4}/{threshold:.4} :: Ring: {ring}"
        );
        count += 1;
    }
}
