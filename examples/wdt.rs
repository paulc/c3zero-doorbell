#![feature(lock_value_accessors)]

use esp_idf_hal::gpio::OutputPin;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::prelude::*;
use esp_idf_svc::http::Method;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::EspWifi;

use std::thread;
use std::time::Duration;

use doorbell::nvs::NVStore;
use doorbell::ota::Ota;
use doorbell::web::{BuildInfo, HomePage, NavBar, NavLink, WebServer};
use doorbell::wifi::{APConfig, APStore, WifiManager, WifiState};
use doorbell::ws2812::{colour, RgbLayout, Ws2812RmtSingle};

const AP_SSID: &str = "ESP32C3-AP";
const AP_PASSWORD: &str = "password";

const NVS_NAMESPACE: &str = "DOORBELL";

const BUILD_INFO: BuildInfo = BuildInfo {
    build_ts: env!("BUILD_TS"),
    build_branch: env!("BUILD_BRANCH"),
    build_hash: env!("BUILD_HASH"),
    build_profile: env!("BUILD_PROFILE"),
};

mod watchdog {

    use esp_idf_hal::task::watchdog::{TWDTConfig, TWDTDriver, TWDT};
    use std::time::Duration;

    pub fn init(p: TWDT, duration: Duration) -> anyhow::Result<()> {
        let twdt_config = TWDTConfig {
            duration,
            panic_on_trigger: true,
            subscribed_idle_tasks: enumset::enum_set!(esp_idf_hal::cpu::Core::Core0),
        };
        TWDTDriver::new(p, &twdt_config)?;
        Ok(())
    }
    pub fn subscribe() -> anyhow::Result<()> {
        esp_idf_sys::esp!(unsafe { esp_idf_sys::esp_task_wdt_add(core::ptr::null_mut()) })
            .map_err(|e| anyhow::anyhow!("esp_task_wdt_add: {e}"))
    }
    pub fn feed() -> anyhow::Result<()> {
        esp_idf_sys::esp!(unsafe { esp_idf_sys::esp_task_wdt_reset() })
            .map_err(|e| anyhow::anyhow!("esp_task_wdt_reset: {e}"))
    }
    /*
    use esp_idf_hal::task::watchdog::{TWDTConfig, TWDTDriver, WatchdogSubscription, TWDT};
    use std::sync::{Mutex, OnceLock};
    use std::time::Duration;

    pub static WATCHDOG_MANAGER: OnceLock<Mutex<WatchdogManager>> = OnceLock::new();

    pub struct WatchdogManager {
        driver: TWDTDriver<'static>,
    }

    impl WatchdogManager {
        pub fn init_static(p: TWDT, duration: Duration) -> anyhow::Result<()> {
            WATCHDOG_MANAGER
                .set(Mutex::new(Self::new(p, duration)?))
                .map_err(|_| anyhow::anyhow!("WATCHDOG_MANAGER: Invalid State"))
        }
        pub fn new(p: TWDT, duration: Duration) -> anyhow::Result<Self> {
            let twdt_config = TWDTConfig {
                duration,
                panic_on_trigger: true,
                subscribed_idle_tasks: enumset::enum_set!(esp_idf_hal::cpu::Core::Core0),
            };
            let driver = TWDTDriver::new(p, &twdt_config)?;
            Ok(Self { driver })
        }
        pub fn subscribe(&mut self) -> anyhow::Result<()> {
            esp_idf_sys::esp!(unsafe { esp_idf_sys::esp_task_wdt_add(core::ptr::null_mut()) })
                .map_err(|e| anyhow::anyhow!("esp_task_wdt_add: {e}"))
        }
        pub fn feed(&self) -> anyhow::Result<()> {
            esp_idf_sys::esp!(unsafe { esp_idf_sys::esp_task_wdt_reset() })
                .map_err(|e| anyhow::anyhow!("esp_task_wdt_reset: {e}"))
        }
    }
    */
}

// Static NavBar
pub const NAVBAR: NavBar = NavBar {
    title: "WDT Test",
    links: &[
        NavLink {
            url: "/wifi",
            label: "Wifi Configuration",
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

    // Watchdog
    watchdog::init(peripherals.twdt, Duration::from_secs(10))?;

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

    let mut count = 0_usize;

    loop {
        match wifi_state {
            WifiState::NotConnected => {
                // Try to connect to known AP (or start local AP)
                wifi.scan()?;
                wifi_state = wifi.try_connect(
                    &APStore::get_aps()?,
                    Some(APConfig::new(AP_SSID, AP_PASSWORD)?),
                    20_000,
                )?;
                log::info!("WifiState: {wifi_state:?}");
                // Update home page status
                home_page.set_status(wifi_state.display_fields())?;

                // Create task
                let _ = thread::spawn(|| {
                    watchdog::subscribe().unwrap();
                    for count in 0..10 {
                        log::info!(">> Thread [{count}]");
                        thread::sleep(Duration::from_millis(1000));
                        watchdog::feed().unwrap();
                    }
                });
                // Start watchdog
                watchdog::subscribe()?;
            }
            WifiState::Station(ref ap, _) => {
                if wifi.is_connected()? {
                    log::info!("app_main: {count}");
                } else {
                    // Only try to reconnect every 30 secs
                    if count.is_multiple_of(30) {
                        log::error!("WIFi Disconnected: Attempting to reconnect");
                        match wifi.connect_sta(ap, 30000) {
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
                }
            }
            WifiState::AP(_, _) => {
                // Run until restart
            }
        }

        led.set(colour::BLUE)?;
        led.set(colour::OFF)?;

        // Update counter
        count += 1;

        // Feed watchdog
        watchdog::feed()?;

        // Sleep
        thread::sleep(Duration::from_millis(1000));
    }
}

fn _err() -> anyhow::Result<()> {
    Err(anyhow::anyhow!("ERROR"))
}
