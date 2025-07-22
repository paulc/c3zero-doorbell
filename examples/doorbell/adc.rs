use esp_idf_hal::delay::TickType;
use esp_idf_hal::timer::TimerDriver;
use esp_idf_svc::hal::adc::{AdcContConfig, AdcContDriver, AdcMeasurement, Attenuated};
use esp_idf_svc::http::server::{EspHttpConnection, Request};

use std::sync::{mpsc, Mutex};
use std::thread;

use crate::stats;

pub const ADC_SAMPLE_RATE: u32 = 1000; // 1kHz sample rate
pub const ADC_BUFFER_LEN: usize = 50; // 50ms sample buffer
pub const ADC_MIN_THRESHOLD: f64 = 0.1; // If Hall-Effect sensor is on we should see Vcc/2
                                        // when bell is off - if this is below threshold
                                        // we assume that sensor is powered off
pub const THRESHOLD_BUFFER: usize = 5; // Average std-dev threshold over this number of frames

pub static ADC_STATS: Mutex<Option<Stats>> = Mutex::new(None);
pub static ADC_DEBUG: Mutex<bool> = Mutex::new(false);

pub fn adc_task(
    timer: esp_idf_hal::timer::TIMER00,
    adc: esp_idf_hal::adc::ADC1,
    adc_pin: esp_idf_hal::gpio::Gpio4,
    tx: mpsc::Sender<RingMessage>,
) -> anyhow::Result<thread::JoinHandle<anyhow::Result<()>>> {
    thread::Builder::new()
        .stack_size(8192)
        .spawn(move || {
            let mut adc = AdcTask::new(timer, adc, adc_pin, tx)?;
            loop {
                adc.get_frame();
                adc.process_frame()?;
            }
        })
        .map_err(|e| anyhow::anyhow!("adc_task: {e}"))
}

pub fn adc_debug_on_handler(request: Request<&mut EspHttpConnection>) -> anyhow::Result<()> {
    ADC_DEBUG
        .replace(true)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut response = request.into_ok_response()?;
    response.write("ADC_DEBUG: On\n".as_bytes())?;
    Ok::<(), anyhow::Error>(())
}

pub fn adc_debug_off_handler(request: Request<&mut EspHttpConnection>) -> anyhow::Result<()> {
    ADC_DEBUG
        .replace(false)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut response = request.into_ok_response()?;
    response.write("ADC_DEBUG: Off\n".as_bytes())?;
    Ok::<(), anyhow::Error>(())
}

#[derive(Debug)]
pub enum RingMessage {
    RingStart(Stats),
    RingStop,
}

#[derive(Debug, Clone)]
pub struct Stats {
    pub count: usize,
    pub elapsed: u64,
    pub mean: f64,
    pub stddev: f64,
    pub threshold: f64,
    pub ring: bool,
}

impl std::fmt::Display for Stats {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "[{}/{:06}] Mean: {:.4} :: Std Dev: {:.4}/{:.4} :: Ring: {}",
            self.count, self.elapsed, self.mean, self.stddev, self.threshold, self.ring
        )
    }
}

// --- IMPLEMENTATION ---

struct AdcState {
    samples: [f64; ADC_BUFFER_LEN],
    ring_state: bool,
    debounce: [bool; 3],
    prev: [f64; THRESHOLD_BUFFER],
    ticks: u64,
    count: usize,
}

impl AdcState {
    fn new() -> Self {
        AdcState {
            samples: [0_f64; ADC_BUFFER_LEN],
            ring_state: false,
            debounce: [false; 3],
            prev: [1.0_f64; THRESHOLD_BUFFER],
            ticks: 0_u64,
            count: 0_usize,
        }
    }
}

struct AdcTask<'a> {
    timer: TimerDriver<'a>,
    adc: AdcContDriver<'a>,
    tx: mpsc::Sender<RingMessage>,
    state: AdcState,
}

impl<'a> AdcTask<'a> {
    fn new(
        timer: esp_idf_hal::timer::TIMER00,
        adc: esp_idf_hal::adc::ADC1,
        adc_pin: esp_idf_hal::gpio::Gpio4,
        tx: mpsc::Sender<RingMessage>,
    ) -> anyhow::Result<Self> {
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
        log::info!( "=== ADC Samples - Sample Rate: {ADC_SAMPLE_RATE} / Samples: {ADC_BUFFER_LEN} / ADC Config: {adc_config:?}");

        Ok(Self {
            timer,
            adc,
            tx,
            state: AdcState::new(),
        })
    }
    fn get_frame(&mut self) {
        let mut frame_len = 0_usize;
        let mut samples = [AdcMeasurement::default(); ADC_BUFFER_LEN];
        while frame_len < ADC_BUFFER_LEN {
            match self
                .adc
                .read(&mut samples, TickType::new_millis(200).ticks())
            {
                Ok(n) => {
                    // We dont always get a full frame from the ADC so fill up
                    // samples_f64 with the data we do have

                    // Make sure we dont overrun samples_f64 array
                    let n = if frame_len + n > ADC_BUFFER_LEN {
                        ADC_BUFFER_LEN - frame_len
                    } else {
                        n
                    };

                    // Append frame to output (ignore annoying clippy warning)
                    #[allow(clippy::needless_range_loop)]
                    for i in 0..n {
                        self.state.samples[frame_len + i] = samples[i].data() as f64 / 4096_f64;
                    }
                    frame_len += n;
                }
                Err(_) => {
                    // Ignore ADC errors ?
                }
            }
        }
    }
    fn process_frame(&mut self) -> anyhow::Result<()> {
        let (mean, stddev) = stats::stats(&self.state.samples);
        let (ring, threshold) = stats::check_ring(mean, stddev, &mut self.state.prev);

        let now = self.timer.counter()?;
        let elapsed = now - self.state.ticks;

        let s = Stats {
            count: self.state.count,
            elapsed,
            mean,
            stddev,
            threshold,
            ring,
        };

        ADC_STATS.replace(Some(s.clone()))?;
        if ADC_DEBUG.get_cloned().unwrap_or(false) {
            log::info!("{s}");
        };

        self.state.debounce = [self.state.debounce[1], self.state.debounce[2], ring];
        match self.state.ring_state {
            true => {
                if !ring {
                    log::info!("Ring: {ring} Debounce: {:?}", self.state.debounce)
                }
                if self.state.debounce == [false, false, false] {
                    self.state.ring_state = false;
                    self.tx.send(RingMessage::RingStop).unwrap();
                }
            }
            false => {
                if ring {
                    log::info!("Ring: {ring} Debounce: {:?}", self.state.debounce)
                }
                if self.state.debounce == [true, true, true] {
                    self.state.ring_state = true;
                    self.tx.send(RingMessage::RingStart(s)).unwrap();
                }
            }
        }

        self.state.count += 1;
        self.state.ticks = now;

        Ok(())
    }
}
