#![feature(lock_value_accessors)]

use esp_idf_hal::gpio::OutputPin;
use esp_idf_hal::task::watchdog::{TWDTConfig, TWDTDriver};
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::prelude::*;
use esp_idf_svc::http::Method;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::EspWifi;

use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use doorbell::mqtt::{MqttManager, MqttMessage};
use doorbell::nvs::NVStore;
use doorbell::web::{NavBar, NavLink, WebServer};
use doorbell::wifi::{APConfig, APStore, WifiManager};
use doorbell::ws2812::{colour, RgbLayout, Ws2812RmtSingle};

mod home_page;
mod led_task;
// mod mqtt;

const AP_SSID: &str = "ESP32C3-AP";
const AP_PASSWORD: &str = "password";

const NVS_NAMESPACE: &str = "DOORBELL";

const WATCHDOG_TIMEOUT: u64 = 10;
const RESET_THRESHOLD: u64 = 5;

const MQTT_URL: &str = "mqtt://192.168.60.1:10883";
const MQTT_RING_TOPIC: &str = "doorbell/ring";

// Static NavBar
pub const NAVBAR: NavBar = NavBar {
    title: "MQTT Alarm",
    links: &[
        NavLink {
            url: "/wifi",
            label: "Wifi Configuration",
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
    let (led_tx, led_rx) = mpsc::channel::<bool>();
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

    // Create MQTT client
    let (mqtt_tx, mqtt_rx) = mpsc::channel::<MqttMessage>();
    let mqtt = Arc::new(Mutex::new(MqttManager::new(MQTT_URL, None, mqtt_tx)?));
    let mqtt_c = mqtt.clone();

    if let Ok(mut mqtt) = mqtt.lock() {
        mqtt.subscribe(MQTT_RING_TOPIC)?;
    }

    // Handle MQTT messages
    let _mqtt_t = thread::spawn(move || loop {
        match mqtt_rx.recv_timeout(Duration::from_secs(2)) {
            Ok(MqttMessage::Message(topic, data)) => {
                let data = String::from_utf8_lossy(&data).to_string();
                log::info!("mqtt_rx: {topic} : {data}");
                if topic == MQTT_RING_TOPIC {
                    match data.as_str() {
                        "ON" => led_tx.send(true).unwrap_or(()),
                        "OFF" => led_tx.send(false).unwrap_or(()),
                        _ => {}
                    }
                }
            }
            Ok(MqttMessage::Reconnected) => {
                log::info!("MQTT re-connected: resubscribing");
                if let Ok(mut mqtt) = mqtt_c.lock() {
                    mqtt.subscribe(MQTT_RING_TOPIC)
                        .expect("Failed to resubscribe to MQTT_RING_TOPIC");
                }
            }
            _ => {}
        }
    });

    // Start watchdog after initialisation
    let mut watchdog = twdt_driver.watch_current_task()?;

    let mut count = 0_u64;
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

        if let Ok(mut mqtt) = mqtt.lock() {
            mqtt.publish(
                "alarm/counter",
                &format!("{count}").as_bytes().to_vec(),
                false,
            )?;
        }

        // Update watchdog
        watchdog.feed()?;

        count += 1;
    }
}
