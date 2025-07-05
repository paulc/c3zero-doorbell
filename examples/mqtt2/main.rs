#![feature(lock_value_accessors)]

use esp_idf_hal::gpio::OutputPin;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::prelude::*;
use esp_idf_svc::http;
use esp_idf_svc::mqtt::client::{
    Details, EspMqttClient, EventPayload, MqttClientConfiguration, QoS,
};
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::EspWifi;

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use doorbell::httpd;
use doorbell::nvs::NVStore;
use doorbell::rgb;
use doorbell::wifi::{self, APConfig};
use doorbell::ws2812;

mod config;
mod home_page;
mod led_task;

use config::{MQTTConfig, MQTT_CONFIG};

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

    // Initislise NVStore
    NVStore::init(nvs_default_partition.clone(), "DOORBELL")?;

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

    // Start HTTP server (either Client/AP
    let mut server = if let Some(config) = wifi_config {
        log::info!("Connected to SSID: {}", config.ssid);
        httpd::start_http_server()?
    } else {
        log::info!("No valid config found - starting AP");
        wifi::start_access_point(&mut wifi)?;
        httpd::start_http_server()?
    };
    server.fn_handler("/", http::Method::Get, home_page::handle_home)?;
    server.fn_handler("/mqtt", http::Method::Get, config::handle_mqtt)?;
    server.fn_handler("/mqtt", http::Method::Post, config::handle_mqtt_submit)?;

    // Ring (C3-Zero onboard WS2812 LED pin = GPIO10)
    let ws2812 = peripherals.pins.gpio10.downgrade_output();
    let channel = peripherals.rmt.channel0;
    let mut led = ws2812::Ws2812RmtSingle::new(ws2812, channel, rgb::RgbLayout::Grb)?;
    led.set(rgb::OFF)?;

    // LED channel
    let (led_tx, led_rx) = mpsc::channel::<bool>();

    // Start led task
    let _led_task = thread::Builder::new().spawn(move || led_task::led_task(led, led_rx))?;

    let mqtt_config = match NVStore::get::<MQTTConfig>("mqtt") {
        Ok(Some(c)) => {
            log::info!("MQTT Config: {c:?}");
            Some(c)
        }
        Ok(None) => {
            log::error!("No data");
            None
        }
        Err(e) => {
            log::error!("Error getting MQTT Config: {e}");
            None
        }
    };

    match mqtt_config {
        Some(c) => {
            MQTT_CONFIG.set(Some(c.clone()))?;
            if c.enabled {
                log::info!("MQTT_CLIENT STARTING");
                let sub_topic = c.ring_topic.clone();
                let mut mqtt_client = EspMqttClient::new_cb(
                    &c.url,
                    &MqttClientConfiguration {
                        client_id: Some(&c.client_id),
                        ..Default::default()
                    },
                    move |e| {
                        let e = e.payload();
                        log::info!(">> MQTT Event: {e:?}");
                        if let EventPayload::Received {
                            topic: Some(topic),
                            details: Details::Complete,
                            data,
                            ..
                        } = e
                        {
                            if topic == sub_topic {
                                let v = String::from_utf8_lossy(data);
                                match v.as_ref() {
                                    "ON" => led_tx.send(true).unwrap(),
                                    _ => led_tx.send(false).unwrap(),
                                }
                            }
                        }
                    },
                )?;
                log::info!("MQTT_CLIENT STARTED");

                let alarm_ip = if let Ok(Some(ip)) = wifi::IP_INFO.get_cloned() {
                    ip.ip.to_string()
                } else {
                    "<Unknown IP>".to_string()
                };

                match mqtt_client.enqueue(
                    &c.status_topic,
                    QoS::AtMostOnce,
                    false,
                    alarm_ip.as_bytes(),
                ) {
                    Ok(_id) => log::info!("MQTT Send: {alarm_ip}"),
                    Err(e) => log::error!("MQTT Error: {e}"),
                }

                mqtt_client.subscribe(&c.ring_topic, QoS::AtMostOnce)?;
                loop {
                    std::thread::sleep(Duration::from_secs(5));
                    match mqtt_client.enqueue(
                        &c.status_topic,
                        QoS::AtMostOnce,
                        false,
                        alarm_ip.as_bytes(),
                    ) {
                        Ok(_id) => log::info!("MQTT Send: {alarm_ip}"),
                        Err(e) => log::error!("MQTT Error: {e}"),
                    }
                }
            } else {
                log::error!("MQTT Not Enabled");
                loop {
                    std::thread::sleep(Duration::from_secs(5));
                }
            }
        }
        None => {
            log::error!("MQTT Configuration Not Found");
            loop {
                std::thread::sleep(Duration::from_secs(5));
            }
        }
    }
}
