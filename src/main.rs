use esp_idf_hal::gpio::{OutputPin, PinDriver};
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::EspWifi;

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

// use anyhow::Context;

use doorbell::httpd;
use doorbell::nvs;
use doorbell::rgb;
use doorbell::wifi;
use doorbell::ws2812;

mod adc;
mod alert;
mod stats;

pub const ADC_SAMPLE_RATE: u32 = 1000; // 1kHz sample rate
pub const ADC_BUFFER_LEN: usize = 50; // 50ms sample buffer
pub const ADC_MIN_THRESHOLD: f64 = 0.1; // If Hall-Effect sensor is on we should see Vcc/2
                                        // when bell is off - if this is below threshold
                                        // we assume that sensor is powered off
pub const THRESHOLD_BUFFER: usize = 5; // Average std-dev threshold over this number of frames

fn main() -> anyhow::Result<()> {
    esp_idf_hal::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();
    log::info!("Started...");

    // Initialise peripherals
    let peripherals = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs_default_partition = EspDefaultNvsPartition::take()?;

    // Status display (C3-Zero onboard WS2812 LED pin = GPIO10)
    let ws2812 = peripherals.pins.gpio10.downgrade_output();
    let channel = peripherals.rmt.channel0;
    let mut status = ws2812::Ws2812RmtSingle::new(ws2812, channel, rgb::RgbLayout::Rgb)?;
    status.set(rgb::OFF)?;

    // Ring led
    let mut ring_led = PinDriver::output(peripherals.pins.gpio6)?;
    ring_led.set_high()?;

    // Initialise WiFi
    let mut wifi: EspWifi<'_> = EspWifi::new(
        peripherals.modem,
        sys_loop.clone(),
        Some(nvs_default_partition.clone()),
    )?;
    wifi::wifi_init(&mut wifi)?;

    // Initial scan
    wifi::wifi_scan(&mut wifi)?;

    // Initislise NVS APStore
    nvs::NVStore::init(nvs_default_partition.clone(), "DOORBELL")?;

    let mut wifi_config: Option<wifi::APConfig> = None;

    for config in wifi::find_known_aps() {
        log::info!("Trying network: {}", config.ssid);
        match wifi::connect_wifi(&mut wifi, &config, 10000) {
            Ok(true) => {
                log::info!("Connected to Wifi: {}", config.ssid);
                wifi_config = Some(config);
                break;
            }
            Ok(false) => {
                log::info!("Failed to connect to Wifi: {}", config.ssid);
            }
            Err(e) => {
                log::info!("Wifi Error: {} [{}]", config.ssid, e);
            }
        }
    }

    log::info!("WiFi Config: {wifi_config:?}");

    let mut _server = if let Some(config) = wifi_config {
        log::info!("Connected to SSID: {}", config.ssid);
        log::info!("IP: {}", wifi.sta_netif().get_ip_info()?.ip);
        httpd::start_http_server()?
    } else {
        log::info!("No valid config found - starting AP");
        wifi::start_access_point(&mut wifi)?;
        log::info!("AP Mode - {:?}", wifi.ap_netif());
        httpd::start_http_server()?
    };

    // ADC Channel
    let (adc_tx, adc_rx) = mpsc::channel();

    // Need to expand stack size as we allocate ADC & FP buffers on stack
    let adc_task = thread::Builder::new()
        .stack_size(8192)
        .spawn(move || {
            adc::adc_task(
                peripherals.timer00,
                peripherals.adc1,
                peripherals.pins.gpio4,
                adc_tx,
                false,
            )
        })
        .expect("Error starting adc_task:");

    // Alert Channel
    let (alert_tx, alert_rx) = mpsc::channel();

    let alert_task = thread::Builder::new()
        .stack_size(8192)
        .spawn(move || alert::alert_task(alert_rx))
        .expect("Error starting alert_task:");

    loop {
        // Check tasks still running - restart if not
        if adc_task.is_finished() || alert_task.is_finished() {
            log::error!("Task Failed - Restarting");
            esp_idf_hal::reset::restart();
        }
        match adc_rx.recv_timeout(Duration::from_millis(500)) {
            Ok(msg) => match msg {
                adc::RingMessage::RingStart => {
                    log::info!("adc_rx :: {msg:?}");
                    ring_led.set_low()?;
                    alert_tx.send(alert::AlertMessage::RingStart)?;
                }
                adc::RingMessage::RingStop => {
                    log::info!("adc_rx :: {msg:?}");
                    ring_led.set_high()?;
                }
                adc::RingMessage::Stats(s) => {
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
            },
            Err(mpsc::RecvTimeoutError::Timeout) => {
                log::info!("adc_rx :: tick");
                status.set(rgb::BLUE)?;
                status.set(rgb::OFF)?;
            }
            Err(e) => log::error!("ERROR :: adc_rx :: {e:?}"),
        }
    }
}
