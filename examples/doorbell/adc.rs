use esp_idf_hal::delay::TickType;
use esp_idf_hal::timer::TimerDriver;
use esp_idf_svc::hal::adc::{AdcContConfig, AdcContDriver, AdcMeasurement, Attenuated};
use esp_idf_svc::http::server::{EspHttpConnection, Request};

use std::array;
use std::sync::{mpsc, Mutex};
use std::thread;

use askama::Template;
use serde::Serialize;

use doorbell::web::NavBar;

const ADC_SAMPLE_RATE: u32 = 1000; // 1kHz sample rate
const ADC_BUFFER_LEN: usize = 50; // 50ms sample buffer
const ADC_MIN_THRESHOLD: f32 = 0.1; // If Hall-Effect sensor is on we should see Vcc/2
                                    // when bell is off - if this is below threshold
                                    // we assume that sensor is powered off
const THRESHOLD_BUFFER: usize = 5; // Average std-dev threshold over this number of frames
const THRESHOLD_TRIGGER: f32 = 2.5; // What level above threshold to trigger ring
const DEBOUNCE: usize = 3; // Number of debounce steps

pub static ADC_STATS: Mutex<Option<Stats>> = Mutex::new(None);
pub static ADC_DEBUG: Mutex<bool> = Mutex::new(false);
pub static ADC_DATA: Mutex<Option<(Stats, [f32; ADC_BUFFER_LEN])>> = Mutex::new(None);

pub type AdcTimer = esp_idf_hal::timer::TIMER00;
pub type AdcDevice = esp_idf_hal::adc::ADC1;
pub type AdcPin = esp_idf_hal::gpio::Gpio4;

#[derive(Debug)]
pub enum RingMessage {
    RingStart(Stats),
    RingStop,
}

#[derive(Debug, Clone, Serialize)]
pub struct Stats {
    pub count: usize,
    pub elapsed: u64,
    pub mean: f32,
    pub stddev: f32,
    pub threshold: f32,
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

// --- IMPLEMENTATION ---

struct AdcState {
    samples: [f32; ADC_BUFFER_LEN],
    ring_state: bool,
    debounce: [bool; DEBOUNCE],
    prev: [f32; THRESHOLD_BUFFER],
    ticks: u64,
    count: usize,
}

impl AdcState {
    fn new() -> Self {
        AdcState {
            samples: [0_f32; ADC_BUFFER_LEN],
            ring_state: false,
            debounce: [false; 3],
            prev: [1.0_f32; THRESHOLD_BUFFER],
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
        timer: AdcTimer,
        adc: AdcDevice,
        adc_pin: AdcPin,
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
                    // samples with the data we do have

                    // Make sure we dont overrun samples array
                    let n = if frame_len + n > ADC_BUFFER_LEN {
                        ADC_BUFFER_LEN - frame_len
                    } else {
                        n
                    };

                    // Append frame to output (ignore annoying clippy warning)
                    #[allow(clippy::needless_range_loop)]
                    for i in 0..n {
                        self.state.samples[frame_len + i] = samples[i].data() as f32 / 4096_f32;
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
        let (mean, stddev) = stats(&self.state.samples);
        let (ring, threshold, updated) = check_ring(mean, stddev, &self.state.prev);
        self.state.prev = updated;

        let now = self.timer.counter()?;
        let elapsed = now - self.state.ticks;

        let stats = Stats {
            count: self.state.count,
            elapsed,
            mean,
            stddev,
            threshold,
            ring,
        };

        ADC_STATS.replace(Some(stats.clone()))?;
        if ADC_DEBUG.get_cloned().unwrap_or(false) {
            log::info!("{stats}");
        };

        self.state.debounce = shift_left(&self.state.debounce, ring);
        match self.state.ring_state {
            true => {
                if !ring {
                    log::info!("Ring: {ring} Debounce: {:?}", self.state.debounce)
                }
                if self.state.debounce.iter().all(|&v| !v) {
                    self.state.ring_state = false;
                    self.tx.send(RingMessage::RingStop).unwrap();
                }
            }
            false => {
                if ring {
                    log::info!("Ring: {ring} Debounce: {:?}", self.state.debounce)
                }
                if self.state.debounce.iter().all(|&v| v) {
                    self.state.ring_state = true;
                    self.tx.send(RingMessage::RingStart(stats.clone())).unwrap();
                }
            }
        }

        // Save frame in ADC_SAMPLES
        let _ = ADC_DATA.replace(Some((stats, self.state.samples)));

        self.state.count += 1;
        self.state.ticks = now;

        Ok(())
    }
}

fn shift_left<T: Copy, const N: usize>(a: &[T; N], b: T) -> [T; N] {
    array::from_fn(|i| if i == N - 1 { b } else { a[i + 1] })
}

pub fn stats(buf: &[f32; ADC_BUFFER_LEN]) -> (f32, f32) {
    let mean = buf.iter().sum::<f32>() / ADC_BUFFER_LEN as f32;
    let var = buf
        .iter()
        .map(|v| {
            let diff = mean - *v;
            diff * diff
        })
        .sum::<f32>();
    let var = var / ADC_BUFFER_LEN as f32;
    (mean, var.sqrt())
}

pub fn check_ring(
    mean: f32,
    stddev: f32,
    prev: &[f32; THRESHOLD_BUFFER],
) -> (bool, f32, [f32; THRESHOLD_BUFFER]) {
    let threshold = prev.iter().sum::<f32>() / prev.len() as f32;
    let ring = stddev > threshold * THRESHOLD_TRIGGER;
    let updated_threshold = if mean > ADC_MIN_THRESHOLD && !ring {
        // Update threshold buffer if above ADC_MIN_THRESHOLD and ring not deteced
        shift_left(prev, stddev)
    } else {
        *prev
    };
    (ring, threshold, updated_threshold)
}

// HTTP Handlers

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

pub fn adc_data(request: Request<&mut EspHttpConnection>) -> anyhow::Result<()> {
    let mut response = request.into_response(
        200,
        Some("OK"),
        &[
            ("Content-Type", "text/event-stream"),
            ("Connection", "keep-alive"),
            ("Access-Control-Allow-Origin", "*"),
        ],
    )?;
    loop {
        if let Some((stats, samples)) = ADC_DATA.replace(None)? {
            let stats_json = serde_json::to_string(&stats)?;
            let json = format!(
                "event: data\r\ndata: {{\"stats\":{stats_json},\"samples\":[{}]}}\r\n\r\n",
                samples
                    .iter()
                    .map(|s| format!("{s:.3}"))
                    .collect::<Vec<_>>()
                    .join(",")
            );
            response.write(json.as_bytes())?;
        }
        response.flush()?;
        std::thread::sleep(std::time::Duration::from_millis(
            (ADC_SAMPLE_RATE / ADC_BUFFER_LEN as u32) as u64,
        ));
    }
}

#[derive(Template)]
#[template(path = "adc_page.html")]
struct AdcPage {
    navbar: NavBar<'static>,
}

pub fn make_adc_page(
    navbar: NavBar<'static>,
) -> impl for<'r> Fn(Request<&mut EspHttpConnection<'r>>) -> anyhow::Result<()> + Send + 'static {
    move |request| {
        let sse_page = AdcPage {
            navbar: navbar.clone(),
        };
        let mut response = request.into_response(200, Some("OK"), &[])?;
        let html = sse_page.render()?;
        response.write(html.as_bytes())?;

        Ok::<(), anyhow::Error>(())
    }
}
