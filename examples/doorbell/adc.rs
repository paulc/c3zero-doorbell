use esp_idf_hal::delay::TickType;
use esp_idf_hal::timer::TimerDriver;
use esp_idf_svc::hal::adc::{AdcContConfig, AdcContDriver, AdcMeasurement, Attenuated};

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
pub static ADC_DEBUG: Mutex<bool> = Mutex::new(true);

#[allow(dead_code)]
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

#[derive(Debug)]
pub enum RingMessage {
    RingStart(Stats),
    RingStop,
}

pub struct AdcState {
    samples: [AdcMeasurement; ADC_BUFFER_LEN],
    samples_f64: [f64; ADC_BUFFER_LEN],
    ring_state: bool,
    debounce: [bool; 3],
    frame: usize,
    prev: [f64; THRESHOLD_BUFFER],
    ticks: u64,
    count: usize,
}

impl AdcState {
    pub fn new() -> Self {
        AdcState {
            samples: [AdcMeasurement::default(); ADC_BUFFER_LEN],
            samples_f64: [0_f64; ADC_BUFFER_LEN],
            ring_state: false,
            debounce: [false; 3],
            frame: 0_usize,
            prev: [1.0_f64; THRESHOLD_BUFFER],
            ticks: 0_u64,
            count: 0_usize,
        }
    }
}

pub struct AdcTask {
    timer: TimerDriver<'static>,
    adc: AdcContDriver<'static>,
    adc_config: AdcContConfig,
    tx: mpsc::Sender<RingMessage>,
}

impl AdcTask {
    pub fn new(
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
        let adc = AdcContDriver::new(adc, &adc_config, adc_pin)?;
        Ok(Self {
            timer,
            adc,
            adc_config,
            tx,
        })
    }

    pub fn run(mut self) -> anyhow::Result<thread::JoinHandle<()>> {
        self.adc.start()?;
        log::info!( "=== ADC Samples - Sample Rate: {ADC_SAMPLE_RATE} / Samples: {ADC_BUFFER_LEN} / ADC Config: {:?}", self.adc_config);
        let mut state = AdcState::new();
        state.ticks = self.timer.counter()?;

        thread::Builder::new()
            .stack_size(8192)
            .spawn(move || {
                loop {
                    match self
                        .adc
                        .read(&mut state.samples, TickType::new_millis(200).ticks())
                    {
                        Ok(n) => {
                            let now = self.timer.counter().unwrap();

                            // We dont always get a full frame from the ADC so fill up
                            // samples_f64 with the data we do have

                            // Make sure we dont overrun samples_f64 array
                            let n = if state.frame + n > ADC_BUFFER_LEN {
                                ADC_BUFFER_LEN - state.frame
                            } else {
                                n
                            };

                            // Append frame to output
                            for i in 0..n {
                                state.samples_f64[state.frame + i] =
                                    state.samples[i].data() as f64 / 4096_f64;
                            }
                            state.frame += n;

                            // When it's full process frame
                            if state.frame == ADC_BUFFER_LEN {
                                let elapsed = now - state.ticks;
                                let (mean, stddev) = stats::stats(&state.samples_f64);
                                let (ring, threshold) =
                                    stats::check_ring(mean, stddev, &mut state.prev);

                                let s = Stats {
                                    count: state.count,
                                    elapsed,
                                    mean,
                                    stddev,
                                    threshold,
                                    ring,
                                };

                                ADC_STATS.replace(Some(s.clone())).unwrap();

                                if ADC_DEBUG.get_cloned().unwrap_or(false) {
                                    log::info!(
                            "[{}/{:06}] Mean: {:.4} :: Std Dev: {:.4}/{:.4} :: Ring: {}",
                            s.count,
                            s.elapsed,
                            s.mean,
                            s.stddev,
                            s.threshold,
                            s.ring
                        );
                                }

                                state.debounce = [state.debounce[1], state.debounce[2], ring];
                                match state.ring_state {
                                    true => {
                                        if !ring {
                                            log::info!(
                                                "Ring: {ring} Debounce: {:?}",
                                                state.debounce
                                            )
                                        }
                                        if state.debounce == [false, false, false] {
                                            state.ring_state = false;
                                            self.tx.send(RingMessage::RingStop).unwrap();
                                        }
                                    }
                                    false => {
                                        if ring {
                                            log::info!(
                                                "Ring: {ring} Debounce: {:?}",
                                                state.debounce
                                            )
                                        }
                                        if state.debounce == [true, true, true] {
                                            state.ring_state = true;
                                            self.tx.send(RingMessage::RingStart(s)).unwrap();
                                        }
                                    }
                                }

                                state.count += 1;
                                state.frame = 0;
                                state.ticks = now;
                            }
                        }
                        Err(_) => {}
                    }
                }
            })
            .map_err(|e| anyhow::anyhow!("adc_thread: {e}"))
    }
}

pub fn _adc_task(
    timer: esp_idf_hal::timer::TIMER00,
    adc: esp_idf_hal::adc::ADC1,
    adc_pin: esp_idf_hal::gpio::Gpio4,
    tx: mpsc::Sender<RingMessage>,
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
    let mut debounce = [false; 3];
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

                    let s = Stats {
                        count,
                        elapsed,
                        mean,
                        stddev,
                        threshold,
                        ring,
                    };

                    ADC_STATS.replace(Some(s.clone())).unwrap();

                    if ADC_DEBUG.get_cloned().unwrap_or(false) {
                        log::info!(
                            "[{}/{:06}] Mean: {:.4} :: Std Dev: {:.4}/{:.4} :: Ring: {}",
                            s.count,
                            s.elapsed,
                            s.mean,
                            s.stddev,
                            s.threshold,
                            s.ring
                        );
                    }

                    debounce = [debounce[1], debounce[2], ring];
                    match ring_state {
                        true => {
                            if !ring {
                                log::info!("Ring: {ring} Debounce: {debounce:?}")
                            }
                            if debounce == [false, false, false] {
                                ring_state = false;
                                tx.send(RingMessage::RingStop)?;
                            }
                        }
                        false => {
                            if ring {
                                log::info!("Ring: {ring} Debounce: {debounce:?}")
                            }
                            if debounce == [true, true, true] {
                                ring_state = true;
                                tx.send(RingMessage::RingStart(s))?;
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
