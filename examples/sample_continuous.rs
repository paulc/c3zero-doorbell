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
const ADC_BUFFER_LEN: usize = 100;
// Min sample rate for continuous driver is 1kHz
const ADC_SAMPLE_RATE: u32 = 1000;
const MODE: Mode = Mode::Stats;

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

    let peripherals = Peripherals::take()?;

    // Setup ADC
    let adc_config = AdcContConfig {
        sample_freq: esp_idf_hal::units::Hertz(ADC_SAMPLE_RATE),
        frame_measurements: ADC_BUFFER_LEN,
        frames_count: 2, // Need 2 buffers as frames can be unaligned (?)
    };

    let adc_pin = Attenuated::db11(peripherals.pins.gpio4);
    let mut adc = AdcContDriver::new(peripherals.adc1, &adc_config, adc_pin)?;

    let mut samples = [AdcMeasurement::default(); ADC_BUFFER_LEN];
    let mut samples_f64 = [0_f64; ADC_BUFFER_LEN];

    adc.start()?;

    let mut timer = TimerDriver::new(peripherals.timer00, &Default::default())?;
    timer.enable(true)?;
    println!("=== Timer: {} Hz", timer.tick_hz());
    let mut prev = timer.counter()?;

    println!(
        "=== ADC Samples - Sample Rate: {ADC_SAMPLE_RATE} / Samples: {ADC_BUFFER_LEN} / ADC Config: {adc_config:?}"
    );

    let mut count = 0_usize;
    let mut frame = 0_usize;

    // Discard the first read after reset as this can cause alignment problems
    let _ = adc.read(&mut samples, TickType::new_millis(2000).ticks());

    loop {
        match adc.read(&mut samples, TickType::new_millis(2000).ticks()) {
            Ok(n) => {
                let now = timer.counter()?;

                // We dont always get a full frame from the ADC so fill up
                // samples_f64 with the data we do have

                // Make sure we dont overrun samples_f64 array
                let n = if frame + n > ADC_BUFFER_LEN {
                    ADC_BUFFER_LEN - frame
                } else {
                    n
                };

                // Append frame to output
                for i in 0..n {
                    samples_f64[frame + i] = samples[i].data() as f64 / 4096_f64;
                }
                frame += n;

                // When it's full process
                if frame == ADC_BUFFER_LEN {
                    for (i, s) in samples.iter().enumerate() {
                        samples_f64[i] = s.data() as f64 / 4096_f64;
                    }
                    match MODE {
                        Mode::Stats => {
                            let ticks = now - prev;
                            let (mean, sd) = stats(&samples_f64);
                            println!("{count} [{ticks}] : Mean = {mean:.3} / Std Dev = {sd:.3}",);
                        }
                        Mode::Samples => {
                            println!(
                                "{}",
                                samples_f64
                                    .iter()
                                    .map(|v| format!("{v:.3}"))
                                    .collect::<Vec<_>>()
                                    .join(","),
                            );
                        }
                    }
                    prev = now;
                    count += 1;
                    frame = 0;
                }
            }
            Err(e) => println!("{e:?}"),
        }
    }
}
