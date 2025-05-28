use esp_idf_hal::adc::attenuation::DB_11;
use esp_idf_hal::adc::oneshot::config::AdcChannelConfig;
use esp_idf_hal::adc::oneshot::{AdcChannelDriver, AdcDriver};
use esp_idf_hal::delay::TickType;
use esp_idf_hal::gpio::{OutputPin, PinDriver};
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_hal::task::notification::Notification;
use esp_idf_hal::timer::config::Config as TimerConfig;
use esp_idf_hal::timer::TimerDriver;
use esp_idf_svc::hal::adc::{AdcContConfig, AdcContDriver, AdcMeasurement, Attenuated};

use std::num::NonZeroU32;

use doorbell::rgb::{self, RgbLayout};
use doorbell::ws2812_rmt::Ws2812RmtSingle;

const ADC_SAMPLE_RATE: u32 = 1000; // 1kHz sample rate
const ADC_BUFFER_LEN: usize = 100; // 100ms sample buffer
const ADC_MIN_THRESHOLD: f64 = 0.1; // If Hall-Effect sensor is on we should see Vcc/2
                                    // when bell is off - if this is below threshold
                                    // we assume that sensor is powered off
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

fn main() -> anyhow::Result<()> {
    esp_idf_hal::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;

    // Status LED (C3-Zero onboard RGB LED pin = GPIO10)
    let ws2812 = peripherals.pins.gpio10.downgrade_output();
    let channel = peripherals.rmt.channel0;
    let mut status = Ws2812RmtSingle::new(ws2812, channel, RgbLayout::Rgb)?;
    status.set(rgb::BLUE)?;

    let mut led = PinDriver::output(peripherals.pins.gpio5)?;
    led.set_low()?;
    esp_idf_hal::delay::FreeRtos::delay_ms(1000);
    led.set_high()?;
    esp_idf_hal::delay::FreeRtos::delay_ms(1000);
    led.set_low()?;

    adc_cont(
        peripherals.timer00,
        peripherals.adc1,
        peripherals.pins.gpio2,
        &mut status,
    )?;

    Ok(())
}

fn adc_cont(
    timer: esp_idf_hal::timer::TIMER00,
    adc: esp_idf_hal::adc::ADC1,
    adc_pin: esp_idf_hal::gpio::Gpio2,
    status: &mut Ws2812RmtSingle<'_>,
) -> anyhow::Result<()> {
    let mut config = AdcContConfig::default();
    config.sample_freq = esp_idf_hal::units::Hertz(ADC_SAMPLE_RATE);
    config.frame_measurements = ADC_BUFFER_LEN;
    config.frames_count = 1;

    let adc_pin = Attenuated::db11(adc_pin);
    let mut adc = AdcContDriver::new(adc, &config, adc_pin)?;

    let mut timer = TimerDriver::new(timer, &Default::default())?;
    timer.enable(true)?;
    println!("=== Timer: {} Hz", timer.tick_hz());
    let mut prev_t = timer.counter()?;

    println!(
        "=== ADC Samples - Sample Rate: {} / Samples: {} / ADC Config: {:?}",
        ADC_SAMPLE_RATE, ADC_BUFFER_LEN, config
    );

    let mut samples = [AdcMeasurement::default(); ADC_BUFFER_LEN];
    let mut samples_f64 = [0_f64; ADC_BUFFER_LEN];
    let mut count = 0_usize;
    let mut prev = [1.0_f64; THRESHOLD_BUFFER];

    adc.start()?;

    loop {
        match adc.read(&mut samples, TickType::new_millis(200).ticks()) {
            Ok(_n) => {
                let now = timer.counter()?;
                for (i, s) in samples.iter().enumerate() {
                    samples_f64[i] = s.data() as f64 / 4096_f64;
                }
                let ring = check_ring(count, now - prev_t, &samples_f64, &mut prev);
                match ring {
                    true => status.set(rgb::RED)?,
                    false => status.set(rgb::GREEN)?,
                };
                count += 1;
                prev_t = now;
            }
            Err(e) => println!("{:?}", e),
        }
    }
}

fn _adc_timer(
    timer: esp_idf_hal::timer::TIMER00,
    adc: esp_idf_hal::adc::ADC1,
    adc_pin: esp_idf_hal::gpio::Gpio2,
) -> anyhow::Result<()> {
    // Setup Timer
    let timer_conf = TimerConfig::new().auto_reload(true);
    let mut timer = TimerDriver::new(timer, &timer_conf)?;

    // Setup ADC Timer
    timer.set_alarm(timer.tick_hz() / ADC_SAMPLE_RATE as u64)?;

    // Notification handler
    let notification = Notification::new();
    let notifier = notification.notifier();

    // Safety: make sure the `Notification` object is not dropped while the subscription is active
    unsafe {
        timer.subscribe(move || {
            let bitset = 0b10001010101;
            notifier.notify_and_yield(NonZeroU32::new(bitset).unwrap());
        })?;
    }

    // Setup ADC
    let adc = AdcDriver::new(adc)?;
    let config = AdcChannelConfig {
        attenuation: DB_11,
        ..Default::default()
    };
    let mut adc_pin = AdcChannelDriver::new(&adc, adc_pin, &config)?;

    // Enable timer
    timer.enable_interrupt()?;
    timer.enable_alarm(true)?;
    timer.enable(true)?;

    let mut count = 0_usize;
    let mut buf = [0_f64; ADC_BUFFER_LEN];
    let mut prev = [1.0_f64; THRESHOLD_BUFFER];

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

        check_ring(count, 0, &buf, &mut prev);
        count += 1;
    }
}

fn check_ring(
    count: usize,
    elapsed: u64,
    buf: &[f64; ADC_BUFFER_LEN],
    prev: &mut [f64; THRESHOLD_BUFFER],
) -> bool {
    let (mean, stddev) = stats(&buf);
    let threshold = prev.iter().sum::<f64>() / prev.len() as f64;

    // Trigger if stddev > 2.5 * threshold
    let ring = stddev > threshold * 2.5_f64;

    if mean < ADC_MIN_THRESHOLD {
        // Hall-effect sensor probably off - ignore readings
        // status.set(rgb::WHITE)?;
    } else {
        if ring {
            // status.set(rgb::RED)?;
        } else {
            // status.set(rgb::BLUE)?;
            // Update threshold buffer
            for i in 0..(THRESHOLD_BUFFER - 1) {
                prev[i] = prev[i + 1];
            }
            prev[THRESHOLD_BUFFER - 1] = stddev;
        }
    }
    println!("[{count}/{elapsed:06}] Mean: {mean:.4} :: Std Dev: {stddev:.4}/{threshold:.4} :: Ring: {ring}");
    ring
}
