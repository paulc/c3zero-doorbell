use esp_idf_hal::delay::TickType;
use esp_idf_hal::prelude::Peripherals;
use esp_idf_hal::sys::EspError;
use esp_idf_hal::timer::TimerDriver;
use esp_idf_svc::hal::adc::{AdcContConfig, AdcContDriver, AdcMeasurement, Attenuated};

#[allow(dead_code)]
enum Mode {
    Stats,
    Samples,
}

// Need to be careful of FreeRTOS watchdog crashes with sample buffers > 200 samples (??)
const SAMPLES: usize = 100;
// Min sample rate for continuous driver is 1kHz
const SAMPLE_RATE: u32 = 1000;
const MODE: Mode = Mode::Samples;

fn stats(buf: &[f64; SAMPLES]) -> (f64, f64) {
    let mean = buf.iter().sum::<f64>() / SAMPLES as f64;
    let var = buf
        .iter()
        .map(|v| {
            let diff = mean - *v;
            diff * diff
        })
        .sum::<f64>();
    let var = var / SAMPLES as f64;
    (mean, var.sqrt())
}

fn main() -> Result<(), EspError> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_hal::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;

    let mut config = AdcContConfig::default();
    config.sample_freq = esp_idf_hal::units::Hertz(SAMPLE_RATE);
    config.frame_measurements = SAMPLES;
    config.frames_count = 1;

    let adc_pin = Attenuated::db11(peripherals.pins.gpio2);
    let mut adc = AdcContDriver::new(peripherals.adc1, &config, adc_pin)?;

    let mut samples = [AdcMeasurement::default(); SAMPLES];
    let mut samples_f64 = [0_f64; SAMPLES];

    adc.start()?;

    let mut timer = TimerDriver::new(peripherals.timer00, &Default::default())?;
    timer.enable(true)?;
    println!("=== Timer: {} Hz", timer.tick_hz());
    let mut prev = timer.counter()?;

    println!(
        "=== ADC Samples - Sample Rate: {} / Samples: {} / ADC Config: {:?}",
        SAMPLE_RATE, SAMPLES, config
    );

    let mut count = 0_usize;

    loop {
        match adc.read(&mut samples, TickType::new_millis(2000).ticks()) {
            Ok(n) => {
                let now = timer.counter()?;
                for (i, s) in samples.iter().enumerate() {
                    samples_f64[i] = s.data() as f64 / 4096_f64;
                }
                match MODE {
                    Mode::Stats => {
                        let ticks = prev - now;
                        let (mean, sd) = stats(&samples_f64);
                        println!("{count} [{n}/{ticks}] : Mean = {mean:.3} / Std Dev = {sd:.3}",);
                    }
                    Mode::Samples => {
                        println!(
                            "{}",
                            samples_f64
                                .iter()
                                .map(|v| format!("{:.3}", v))
                                .collect::<Vec<_>>()
                                .join(","),
                        );
                    }
                }
                count += 1;
                prev = now;
            }
            Err(e) => println!("{:?}", e),
        }
    }
}
