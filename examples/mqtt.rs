#![feature(lock_value_accessors)]

use esp_idf_hal::gpio::OutputPin;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::prelude::*;
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

use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct MQTTConfig {
    pub url: String,
    pub client_id: String,
    pub ring_topic: String,
    pub status_topic: String,
}

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

    let mut _server = if let Some(config) = wifi_config {
        log::info!("Connected to SSID: {}", config.ssid);
        httpd::start_http_server()?
    } else {
        log::info!("No valid config found - starting AP");
        wifi::start_access_point(&mut wifi)?;
        httpd::start_http_server()?
    };

    // Ring (C3-Zero onboard WS2812 LED pin = GPIO10)
    let ws2812 = peripherals.pins.gpio10.downgrade_output();
    let channel = peripherals.rmt.channel0;
    let mut status = ws2812::Ws2812RmtSingle::new(ws2812, channel, rgb::RgbLayout::Grb)?;
    status.set(rgb::OFF)?;

    // Status channel
    let (status_tx, status_rx) = mpsc::channel::<bool>();

    // Start status task
    let _status_task = thread::Builder::new()
        .spawn(move || {
            let mut ring = false;
            let mut timeout: Option<u8> = None;
            let mut on = false;
            loop {
                match status_rx.recv_timeout(Duration::from_millis(200)) {
                    Ok(v) => {
                        log::info!(">> status_rx: {v}");
                        if v {
                            ring = true;
                            timeout = None; // Reset timeout if necessary
                        } else {
                            // Keep flashing for timeout cycles
                            timeout = Some(5);
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(e) => log::error!("status_rx error: {e}"),
                }
                // log::info!("ring={ring} timeout={timeout:?} on={on}");
                status
                    .set(if ring && on { rgb::RED } else { rgb::OFF })
                    .unwrap();
                on = !on;

                timeout = match timeout {
                    Some(0) => {
                        ring = false;
                        None
                    }
                    Some(n) => Some(n - 1),
                    None => None,
                };
            }
        })
        .expect("Error starting status_task:");

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
                                "ON" => status_tx.send(true).unwrap(),
                                _ => status_tx.send(false).unwrap(),
                            }
                        }
                    }
                },
            )?;
            let alarm_ip = if let Ok(Some(ip)) = wifi::IP_INFO.get_cloned() {
                ip.ip.to_string()
            } else {
                "<Unknown IP>".to_string()
            };

            match mqtt_client.enqueue(&c.status_topic, QoS::AtMostOnce, false, alarm_ip.as_bytes())
            {
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
        }
        None => {
            log::error!("MQTT Configuration Not Found");
            loop {
                std::thread::sleep(Duration::from_secs(5));
            }
        }
    }
}
