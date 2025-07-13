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
use doorbell::web::{NavBar, NavLink, WebServer};
use doorbell::wifi::{APConfig, APStore, WifiManager};
use doorbell::ws2812::{colour, RgbLayout, Ws2812RmtSingle};

mod home_page;
mod led_task;
mod mqtt_task;

const AP_SSID: &str = "ESP32C3-AP";
const AP_PASSWORD: &str = "password";

const NVS_NAMESPACE: &str = "DOORBELL";

const WATCHDOG_TIMEOUT: u64 = 10;
const RESET_THRESHOLD: u64 = 5;

// Static NavBar
pub const NAVBAR: NavBar = NavBar {
    title: "MQTT Alarm",
    links: &[
        NavLink {
            url: "/wifi",
            label: "Wifi Configuration",
        },
        NavLink {
            url: "/mqtt",
            label: "MQTT Configuration",
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

    // Onboard WS2812 (GPIO10)
    let ws2812 = peripherals.pins.gpio10.downgrade_output();
    let channel = peripherals.rmt.channel0;
    let mut led = Ws2812RmtSingle::new(ws2812, channel, RgbLayout::Grb)?;
    led.set(colour::OFF)?;

    // LED task
    let (led_tx, led_rx) = mpsc::channel::<led_task::LedMessage>();
    let _led_task = thread::spawn(move || led_task::led_task(led, led_rx));

    // Try to connect to known AP (or start local AP)
    wifi.scan()?;
    let wifi_state = wifi.try_connect(
        &APStore::get_aps()?,
        Some(APConfig::new(AP_SSID, AP_PASSWORD)?),
        20_000,
    )?;
    log::info!("WifiState: {wifi_state:?}");

    // Start web server
    let mut web = WebServer::new(NAVBAR)?;

    // Add module handlers
    nvs.add_handlers(&mut web, NAVBAR)?;
    wifi.add_handlers(&mut web, NAVBAR)?;

    // Add local handlers
    web.add_handler(
        "/",
        Method::Get,
        home_page::make_handler(&wifi_state, NAVBAR),
    )?;

    let mqtt = mqtt_task::MQTTTask::init()?;
    mqtt.add_handlers(&mut web, NAVBAR)?;
    if mqtt.is_enabled() {
        let led_tx = led_tx.clone();

        match mqtt.run(
            move |s: &str| match s {
                "ON" => led_tx.send(led_task::LedMessage::Ring(true)).unwrap_or(()),
                "OFF" => led_tx.send(led_task::LedMessage::Ring(false)).unwrap_or(()),
                _ => {}
            },
            &wifi_state.to_string(),
        ) {
            Ok(_) => log::info!("mqtt_task running"),
            Err(e) => log::info!("mqtt_task error [{e}]"),
        }
    }

    // Start watchdog after initialisation
    let mut watchdog = twdt_driver.watch_current_task()?;
    let mut reset_count = 0_u64;

    loop {
        thread::sleep(Duration::from_millis(2000));

        // Check WiFi connected
        reset_count = if wifi.is_connected()? {
            0
        } else {
            log::error!("ERROR: Wifi Disconnected {reset_count}");
            reset_count + 1
        };

        if reset_count > RESET_THRESHOLD {
            log::error!("FATAL: RESET_THRESHOLD - Rebooting");
            esp_idf_hal::reset::restart();
        }

        led_tx.send(led_task::LedMessage::Flash)?;

        // Update watchdog
        watchdog.feed()?;
    }
}
