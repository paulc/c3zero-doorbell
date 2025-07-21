#![feature(lock_value_accessors)]

use esp_idf_hal::gpio::OutputPin;
use esp_idf_hal::task::watchdog::{TWDTConfig, TWDTDriver};
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::prelude::*;
use esp_idf_svc::http::Method;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::EspWifi;

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use doorbell::nvs::NVStore;
use doorbell::ota::Ota;
use doorbell::web::{BuildInfo, HomePage, NavBar, NavLink, WebServer};
use doorbell::wifi::{APConfig, APStore, WifiManager, WifiState};
use doorbell::ws2812::{colour, RgbLayout, Ws2812RmtSingle};

mod adc;
mod led_task;
mod stats;

const AP_SSID: &str = "ESP32C3-AP";
const AP_PASSWORD: &str = "password";

const NVS_NAMESPACE: &str = "DOORBELL";

const WATCHDOG_TIMEOUT: u64 = 60;

const BUILD_INFO: BuildInfo = BuildInfo {
    build_ts: env!("BUILD_TS"),
    build_branch: env!("BUILD_BRANCH"),
    build_hash: env!("BUILD_HASH"),
    build_profile: env!("BUILD_PROFILE"),
};

// Static NavBar
pub const NAVBAR: NavBar = NavBar {
    title: "Doorbell",
    links: &[
        NavLink {
            url: "/wifi",
            label: "Wifi Configuration",
        },
        NavLink {
            url: "/alert",
            label: "Alert Configuration",
        },
        NavLink {
            url: "/ota_page",
            label: "OTA Update",
        },
        NavLink {
            url: "/reset_page",
            label: "Reset",
        },
    ],
};

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

    // Hardware Watchdog
    let twdt_config = TWDTConfig {
        duration: Duration::from_secs(WATCHDOG_TIMEOUT),
        panic_on_trigger: true,
        subscribed_idle_tasks: enumset::enum_set!(esp_idf_hal::cpu::Core::Core0),
    };
    let mut twdt_driver = TWDTDriver::new(peripherals.twdt, &twdt_config)?;

    // NVStore
    let nvs = NVStore::init(nvs_default_partition.clone(), NVS_NAMESPACE)?;

    // WiFi
    let mut wifi = WifiManager::new(EspWifi::new(
        peripherals.modem,
        sys_loop.clone(),
        Some(nvs_default_partition.clone()),
    )?)?;

    let mut wifi_state = WifiState::NotConnected;

    // Onboard WS2812 (GPIO10)
    let ws2812 = peripherals.pins.gpio10.downgrade_output();
    let channel = peripherals.rmt.channel0;
    let mut led = Ws2812RmtSingle::new(ws2812, channel, RgbLayout::Grb)?;
    led.set(colour::OFF)?;

    // LED task
    let (led_tx, led_rx) = mpsc::channel::<led_task::LedMessage>();
    let _led_task = thread::spawn(move || led_task::led_task(led, led_rx));

    // Start web server
    let mut web = WebServer::new(NAVBAR)?;

    // OTA
    let ota = Ota::new();

    // Add module handlers
    nvs.add_handlers(&mut web, NAVBAR)?;
    wifi.add_handlers(&mut web, NAVBAR)?;
    ota.add_handlers(&mut web, NAVBAR)?;

    // Home Page
    let home_page = HomePage::new(NAVBAR.title, BUILD_INFO.display_fields(), NAVBAR);
    home_page.set_status(wifi_state.display_fields())?;
    web.add_handler("/", Method::Get, home_page.make_handler())?;

    // ADC Task
    let (adc_tx, adc_rx) = mpsc::channel();

    let adc_task = adc::AdcTask::new(
        peripherals.timer00,
        peripherals.adc1,
        peripherals.pins.gpio4,
        adc_tx,
    )?;
    adc_task.run()?;

    // Start ADC task
    // Need to expand stack size as we allocate ADC & FP buffers on stack
    /*
    let _adc_task = thread::Builder::new()
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
    */

    // Start watchdog after initialisation
    let mut watchdog = twdt_driver.watch_current_task()?;
    let mut count = 0_usize;

    loop {
        match (&wifi_state, wifi.is_connected()?) {
            (WifiState::NotConnected, _) => {
                // NotConnected - try to connect to known AP (or start local AP)
                wifi.scan()?;
                wifi_state = wifi.try_connect(
                    &APStore::get_aps()?,
                    Some(APConfig::new(AP_SSID, AP_PASSWORD)?),
                    30_000,
                )?;
                log::info!("WifiState: {wifi_state:?}");

                // If we have connected start services
                if let WifiState::Station(_, _) = wifi_state {
                    // Start services
                }

                // Update home page status
                home_page.set_status(wifi_state.display_fields())?;
            }
            (WifiState::Station(ref ap, _), false) => {
                // WiFi disconnected - try to reconnect (every 30 secs)
                if count % 30 == 0 {
                    log::error!("WIFi Disconnected: Attempting to reconnect");
                    match wifi.connect_sta(ap, 30_000) {
                        Ok(WifiState::Station(config, ip_info)) => {
                            log::info!("WIFi Reconnected: {wifi_state}");
                            wifi_state = WifiState::Station(config, ip_info);
                            // Update home page status
                            home_page.set_status(wifi_state.display_fields())?;
                        }
                        Ok(_) => {
                            log::info!("WiFi Failed to Reconnect");
                        }
                        Err(e) => {
                            // Something went wrong - possibly reboot?
                            log::info!("WiFi Error Reconnecting: {e}");
                        }
                    }
                }
                // Flush adc_rx buffer
                while adc_rx.try_recv().is_ok() {}
                thread::sleep(Duration::from_millis(1000));
            }
            (WifiState::Station(_, _), true) => {
                // WiFi Online
                match adc_rx.recv_timeout(Duration::from_millis(1000)) {
                    Ok(msg) => match msg {
                        adc::RingMessage::RingStart(ref _s) => {
                            log::info!("adc_rx :: {msg:?}");
                        }
                        adc::RingMessage::RingStop => {
                            log::info!("adc_rx :: {msg:?}");
                        }
                    },
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(e) => log::error!("ERROR :: adc_rx :: {e:?}"),
                }
            }
            (WifiState::AP(_, _), _) => {
                // AP Mode
                led_tx.send(led_task::LedMessage::Flash(colour::GREEN))?;
                // Flush adc_rx buffer
                while adc_rx.try_recv().is_ok() {}
                thread::sleep(Duration::from_millis(1000));
            }
        }

        // Update watchdog
        watchdog.feed()?;

        // Update counter
        count += 1;
    }
}
