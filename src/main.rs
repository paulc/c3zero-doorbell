use std::thread;
use std::time::Duration;

use esp_idf_hal::adc::attenuation::DB_6;
use esp_idf_hal::adc::oneshot::config::AdcChannelConfig;
use esp_idf_hal::adc::oneshot::*;
use esp_idf_hal::peripherals::Peripherals;

const ADC_BUFFER_LEN: usize = 100;
const THRESHOLD_BUFFER: usize = 5;
const SAMPLE_PERIOD_US: u64 = 1000;

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

fn main() -> anyhow::Result<()> {
    esp_idf_hal::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();
    log::info!("Starting...");

    let peripherals = Peripherals::take()?;
    let adc = AdcDriver::new(peripherals.adc1)?;

    let config = AdcChannelConfig {
        attenuation: DB_6,
        ..Default::default()
    };
    let mut adc_pin = AdcChannelDriver::new(&adc, peripherals.pins.gpio2, &config)?;

    let mut count = 0_u32;
    let mut buf = [0_f64; ADC_BUFFER_LEN];
    let mut prev = [1.0_f64; THRESHOLD_BUFFER];

    loop {
        // Ignore annoying clippy warning
        #[allow(clippy::needless_range_loop)]
        for i in 0..buf.len() {
            buf[i] = adc.read(&mut adc_pin)? as f64 / 4096_f64;
            thread::sleep(Duration::from_micros(SAMPLE_PERIOD_US));
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
        thread::sleep(Duration::from_millis(10));
    }
}
