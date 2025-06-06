use esp_idf_hal::delay::TickType;
use esp_idf_hal::timer::TimerDriver;
use esp_idf_svc::hal::adc::{AdcContConfig, AdcContDriver, AdcMeasurement, Attenuated};
use std::sync::mpsc;

use crate::stats;

pub const ADC_SAMPLE_RATE: u32 = 1000; // 1kHz sample rate
pub const ADC_BUFFER_LEN: usize = 50; // 50ms sample buffer
pub const ADC_MIN_THRESHOLD: f64 = 0.1; // If Hall-Effect sensor is on we should see Vcc/2
                                        // when bell is off - if this is below threshold
                                        // we assume that sensor is powered off
pub const THRESHOLD_BUFFER: usize = 5; // Average std-dev threshold over this number of frames

#[allow(dead_code)]
#[derive(Debug)]
pub struct Stats {
    pub count: usize,
    pub elapsed: u64,
    pub mean: f64,
    pub stddev: f64,
    pub threshold: f64,
    pub ring: bool,
}

#[derive(Debug)]
pub enum RingMessage {
    RingStart,
    RingStop,
    Stats(Stats),
}

pub fn adc_task(
    timer: esp_idf_hal::timer::TIMER00,
    adc: esp_idf_hal::adc::ADC1,
    adc_pin: esp_idf_hal::gpio::Gpio4,
    tx: mpsc::Sender<RingMessage>,
    stats: bool,
) -> anyhow::Result<()> {
    // Setup Timer
    let mut timer = TimerDriver::new(timer, &Default::default())?;
    timer.enable(true)?;
    log::info!("=== Timer: {} Hz", timer.tick_hz());

    // Setup ADC
    let adc_config = AdcContConfig {
        sample_freq: esp_idf_hal::units::Hertz(ADC_SAMPLE_RATE),
        frame_measurements: ADC_BUFFER_LEN,
        frames_count: 2, // Need 2 buffers as frames can be unaligned (?)
    };

    let adc_pin = Attenuated::db11(adc_pin);
    let mut adc = AdcContDriver::new(adc, &adc_config, adc_pin)?;
    adc.start()?;
    log::info!(
        "=== ADC Samples - Sample Rate: {ADC_SAMPLE_RATE} / Samples: {ADC_BUFFER_LEN} / ADC Config: {adc_config:?}"
    );

    // State variables
    let mut samples = [AdcMeasurement::default(); ADC_BUFFER_LEN];
    let mut samples_f64 = [0_f64; ADC_BUFFER_LEN];
    let mut ring_state = false;
    let mut debounce = [false; 2];
    let mut frame = 0_usize;
    let mut prev = [1.0_f64; THRESHOLD_BUFFER];
    let mut ticks = timer.counter()?;
    let mut count = 0_usize;

    unsafe {
        let stack_remaining = esp_idf_sys::uxTaskGetStackHighWaterMark(std::ptr::null_mut());
        log::info!("Stack remaining: {stack_remaining} bytes");
    }

    loop {
        match adc.read(&mut samples, TickType::new_millis(200).ticks()) {
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

                // When it's full process frame
                if frame == ADC_BUFFER_LEN {
                    let elapsed = now - ticks;
                    let (mean, stddev) = stats::stats(&samples_f64);
                    let (ring, threshold) = stats::check_ring(mean, stddev, &mut prev);

                    if stats {
                        tx.send(RingMessage::Stats(Stats {
                            count,
                            elapsed,
                            mean,
                            stddev,
                            threshold,
                            ring,
                        }))?;
                    }

                    debounce = [debounce[1], ring];
                    match ring_state {
                        true => {
                            if debounce == [false, false] {
                                ring_state = false;
                                tx.send(RingMessage::RingStop)?;
                            }
                        }
                        false => {
                            if debounce == [true, true] {
                                ring_state = true;
                                tx.send(RingMessage::RingStart)?;
                            }
                        }
                    }
                    count += 1;
                    frame = 0;
                    ticks = now;
                }
            }
            Err(e) => log::error!("ERROR :: adc_task :: {e:?}"),
        }
    }
}
