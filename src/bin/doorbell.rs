#![feature(lock_value_accessors)]

use esp_idf_hal::gpio::{OutputPin, PinDriver};
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_hal::task::watchdog::{TWDTConfig, TWDTDriver};
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::http;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::EspWifi;

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use doorbell::adc;
use doorbell::alert;
use doorbell::httpd;
use doorbell::nvs;
use doorbell::rgb;
use doorbell::wifi;
use doorbell::ws2812;

fn main() -> anyhow::Result<()> {
    esp_idf_hal::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();
    log::info!("Started...");

    // Initialise peripherals
    let peripherals = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs_default_partition = EspDefaultNvsPartition::take()?;

    // Hardware Watchdog
    let twdt_config = TWDTConfig {
        duration: Duration::from_secs(5),
        panic_on_trigger: true,
        subscribed_idle_tasks: enumset::enum_set!(esp_idf_hal::cpu::Core::Core0),
    };
    let mut twdt_driver = TWDTDriver::new(peripherals.twdt, &twdt_config)?;

    // Status display (C3-Zero onboard WS2812 LED pin = GPIO10)
    let ws2812 = peripherals.pins.gpio10.downgrade_output();
    let channel = peripherals.rmt.channel0;
    let mut status = ws2812::Ws2812RmtSingle::new(ws2812, channel, rgb::RgbLayout::Rgb)?;
    status.set(rgb::RED)?;

    // Ring led
    let mut ring_led = PinDriver::output(peripherals.pins.gpio6)?;
    ring_led.set_high()?;

    // Initialise NVStore
    nvs::NVStore::init(nvs_default_partition.clone(), "DOORBELL")?;

    // Initialise WiFi
    let mut wifi: EspWifi<'_> = EspWifi::new(
        peripherals.modem,
        sys_loop.clone(),
        Some(nvs_default_partition.clone()),
    )?;
    wifi::wifi_init(&mut wifi)?;

    // Initial scan
    wifi::wifi_scan(&mut wifi)?;

    let mut wifi_config: Option<wifi::APConfig> = None;

    // Try to connect to known APs
    for config in wifi::find_known_aps() {
        log::info!("Trying network: {}", config.ssid);
        match wifi::connect_wifi(&mut wifi, &config, 10000) {
            Ok(true) => {
                log::info!("Connected to Wifi: {}", config.ssid);
                wifi_config = Some(config);
                break;
            }
            Ok(false) => {
                log::error!("Failed to connect to Wifi: {}", config.ssid);
            }
            Err(e) => {
                log::error!("Wifi Error: {} [{}]", config.ssid, e);
            }
        }
    }

    log::info!("WiFi Config: {wifi_config:?}");

    let mut server = if let Some(config) = wifi_config {
        log::info!("Connected to SSID: {}", config.ssid);
        log::info!("IP: {}", wifi.sta_netif().get_ip_info()?.ip);
        httpd::start_http_server()?
    } else {
        log::info!("No valid config found - starting AP");
        wifi::start_access_point(&mut wifi)?;
        log::info!("AP Mode - {:?}", wifi.ap_netif());
        httpd::start_http_server()?
    };

    // Add adc debug handlers
    server.fn_handler("/adc_debug/on", http::Method::Get, |r| {
        adc::ADC_DEBUG
            .replace(true)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let mut response = r.into_ok_response()?;
        response.write("ADC_DEBUG: On\n".as_bytes())?;
        Ok::<(), anyhow::Error>(())
    })?;

    server.fn_handler("/adc_debug/off", http::Method::Get, |r| {
        adc::ADC_DEBUG
            .replace(false)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let mut response = r.into_ok_response()?;
        response.write("ADC_DEBUG: Off\n".as_bytes())?;
        Ok::<(), anyhow::Error>(())
    })?;

    // ADC Channel
    let (adc_tx, adc_rx) = mpsc::channel();

    // Start ADC task
    // Need to expand stack size as we allocate ADC & FP buffers on stack
    let adc_task = thread::Builder::new()
        .stack_size(8192)
        .spawn(move || {
            adc::adc_task(
                peripherals.timer00,
                peripherals.adc1,
                peripherals.pins.gpio4,
                adc_tx,
            )
        })
        .expect("Error starting adc_task:");

    // Alert Channel
    let (alert_tx, alert_rx) = mpsc::channel();

    let alert_task = thread::Builder::new()
        .stack_size(8192)
        .spawn(move || alert::alert_task(alert_rx))
        .expect("Error starting alert_task:");

    // Dont configure watchdog until we have setup background tasks
    let mut watchdog = twdt_driver.watch_current_task()?;
    let mut count: u64 = 0;

    loop {
        // Check tasks still running - restart if not
        if adc_task.is_finished() || alert_task.is_finished() {
            log::error!("Task Failed - Restarting");
            esp_idf_hal::reset::restart();
        }
        match adc_rx.recv_timeout(Duration::from_millis(1000)) {
            Ok(msg) => match msg {
                adc::RingMessage::RingStart(ref s) => {
                    log::info!("adc_rx :: {msg:?}");
                    ring_led.set_low()?;
                    alert_tx.send(alert::AlertMessage::RingStart(s.clone()))?;
                }
                adc::RingMessage::RingStop => {
                    log::info!("adc_rx :: {msg:?}");
                    ring_led.set_high()?;
                    alert_tx.send(alert::AlertMessage::RingStop)?;
                }
            },
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(e) => log::error!("ERROR :: adc_rx :: {e:?}"),
        }
        // Send status message every 30 secs
        if count % 30 == 0 {
            alert_tx.send(alert::AlertMessage::Status)?;
        }

        status.set(rgb::BLUE)?;
        status.set(rgb::OFF)?;

        count += 1;
        // Update watchdog
        watchdog.feed()?
    }
}
