use esp_idf_hal::delay::TickType;
use esp_idf_hal::gpio::PinDriver;
use esp_idf_hal::timer::TimerDriver;
use esp_idf_svc::hal::adc::{AdcContConfig, AdcContDriver, AdcMeasurement, Attenuated};

use crate::ADC_BUFFER_LEN;
use crate::ADC_SAMPLE_RATE;
use crate::THRESHOLD_BUFFER;

pub fn adc_continuous(
    timer: esp_idf_hal::timer::TIMER00,
    adc: esp_idf_hal::adc::ADC1,
    adc_pin: esp_idf_hal::gpio::Gpio4,
    ring_led: &mut PinDriver<'_, esp_idf_hal::gpio::Gpio6, esp_idf_hal::gpio::Output>,
) -> anyhow::Result<()> {
    // Setup Timer
    let mut timer = TimerDriver::new(timer, &Default::default())?;
    timer.enable(true)?;
    println!("=== Timer: {} Hz", timer.tick_hz());

    // Setup ADC
    let adc_config = AdcContConfig {
        sample_freq: esp_idf_hal::units::Hertz(ADC_SAMPLE_RATE),
        frame_measurements: ADC_BUFFER_LEN,
        frames_count: 2, // Need 2 buffers as frames can be unaligned (?)
    };

    let adc_pin = Attenuated::db11(adc_pin);
    let mut adc = AdcContDriver::new(adc, &adc_config, adc_pin)?;
    adc.start()?;
    println!(
        "=== ADC Samples - Sample Rate: {ADC_SAMPLE_RATE} / Samples: {ADC_BUFFER_LEN} / ADC Config: {adc_config:?}"
    );

    // State variables
    let mut samples = [AdcMeasurement::default(); ADC_BUFFER_LEN];
    let mut samples_f64 = [0_f64; ADC_BUFFER_LEN];
    let mut ring_state = false;
    let mut debounce = [false; 2];
    let mut count = 0_usize;
    let mut frame = 0_usize;
    let mut prev = [1.0_f64; THRESHOLD_BUFFER];
    let mut ticks = timer.counter()?;

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

                // When it's full process
                if frame == ADC_BUFFER_LEN {
                    let ring =
                        crate::stats::check_ring(count, now - ticks, &samples_f64, &mut prev);
                    debounce = [debounce[1], ring];
                    match ring_state {
                        true => {
                            if debounce == [false, false] {
                                ring_state = false;
                                ring_led.set_high()?;
                                println!(">> RING STOP",);
                            }
                        }
                        false => {
                            if debounce == [true, true] {
                                ring_state = true;
                                ring_led.set_low()?;
                                println!(">> RING START",);
                                // let _ = std::thread::spawn(|| {
                                //     log::info!(">> Calling start webhook");
                                crate::pushover::send_pushover_alert()?;
                                //});
                            }
                        }
                    };
                    count += 1;
                    frame = 0;
                    ticks = now;
                }
            }
            Err(e) => println!("{e:?}"),
        }
    }
}
