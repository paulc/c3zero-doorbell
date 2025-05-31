use esp_idf_hal::gpio::{OutputPin, PinDriver};
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::EspWifi;

use doorbell::httpd;
use doorbell::nvs;
use doorbell::rgb;
use doorbell::wifi;
use doorbell::ws2812;

mod adc;
mod pushover;
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
    nvs::APStore::init(nvs_default_partition.clone())?;

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

    adc::adc_continuous(
        peripherals.timer00,
        peripherals.adc1,
        peripherals.pins.gpio4,
        &mut ring_led,
    )
}
