use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::prelude::*;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::EspWifi;

// Local imports
use doorbell::httpd;
use doorbell::nvs::APStore;
use doorbell::wifi::{self, APConfig};

fn main() -> anyhow::Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();
    log::info!("Starting...");

    // Initialise peripherals
    let peripherals = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs_default_partition = EspDefaultNvsPartition::take()?;

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
    APStore::init(nvs_default_partition.clone())?;

    let mut wifi_config: Option<APConfig> = None;
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

    let mut server = if let Some(config) = wifi_config {
        log::info!("Connected to SSID: {}", config.ssid);
        httpd::start_http_server()?
    } else {
        log::info!("No valid config found - starting AP");
        wifi::start_access_point(&mut wifi)?;
        httpd::start_http_server()?
    };

    server.fn_handler(
        "/",
        esp_idf_svc::http::Method::Get,
        |request| -> anyhow::Result<()> {
            let mut response = request.into_ok_response()?;
            response.write("[[--main--]]\n".as_bytes())?;
            Ok(())
        },
    )?;

    loop {
        FreeRtos::delay_ms(1000); // Delay for 100 milliseconds
    }
}
