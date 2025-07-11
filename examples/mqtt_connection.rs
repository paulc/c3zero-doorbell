use core::time::Duration;

use std::sync::{mpsc, Arc, Mutex};

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::*;

use doorbell::nvs::NVStore;
use doorbell::wifi::{APConfig, APStore, WifiManager};

const NVS_NAMESPACE: &str = "DOORBELL";

const AP_SSID: &str = "ESP32C3-AP";
const AP_PASSWORD: &str = "password";

const MQTT_URL: &str = "mqtt://192.168.60.1:10883/";
const MQTT_TOPIC: &str = "esp-mqtt-demo";
//const MQTT_CLIENT_ID: &str = "esp-mqtt-demo";

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take().unwrap();
    let nvs_default_partition = EspDefaultNvsPartition::take().unwrap();

    // NVStore
    let _nvs = NVStore::init(nvs_default_partition.clone(), NVS_NAMESPACE)?;

    // WiFi
    let mut wifi = WifiManager::new(EspWifi::new(
        peripherals.modem,
        sys_loop.clone(),
        Some(nvs_default_partition.clone()),
    )?)?;

    // Try to connect to known AP (or start local AP)
    wifi.scan()?;
    let wifi_state = wifi.try_connect(
        &APStore::get_aps()?,
        Some(APConfig::new(AP_SSID, AP_PASSWORD)?),
        20_000,
    )?;
    log::info!("WifiState: {wifi_state:?}");

    let (mqtt_tx, mqtt_rx) = mpsc::channel::<mqtt::MqttMessage>();
    let mqtt = Arc::new(Mutex::new(mqtt::MqttManager::new(MQTT_URL, None, mqtt_tx)?));
    {
        let mut mqtt = mqtt.lock().unwrap();
        let _ = mqtt.subscribe(MQTT_TOPIC);
        let _ = mqtt.subscribe("test/#");
    }

    let mqtt_c = mqtt.clone();
    std::thread::spawn(move || {
        let mut count = 0_u64;
        loop {
            let msg = format!("Test:: {count}");
            {
                let mut mqtt = mqtt_c.lock().unwrap();
                let _ = mqtt.publish(MQTT_TOPIC, msg.as_bytes(), false);
            }
            std::thread::sleep(Duration::from_millis(5000));
            count += 1;
        }
    });

    loop {
        match mqtt_rx.recv_timeout(Duration::from_millis(1000)) {
            Ok(mqtt::MqttMessage::Message(topic, message)) => {
                log::info!("[RX] {topic} >> {}", String::from_utf8_lossy(&message))
            }
            Ok(mqtt::MqttMessage::Reconnected) => {
                // Resubscribe
                log::info!("Reconnected - re-subscribing");
                let mut mqtt = mqtt.lock().unwrap();
                mqtt.subscribe(MQTT_TOPIC)?;
                mqtt.subscribe("test/#")?;
            }
            _ => {}
        }
    }
}

mod mqtt {

    use esp_idf_svc::mqtt::client::{
        Details, EspMqttClient, EventPayload, MqttClientConfiguration, QoS,
    };

    use core::time::Duration;
    use std::cmp::max;
    use std::sync::mpsc;

    pub enum MqttMessage {
        Message(String, Vec<u8>),
        Reconnected,
    }

    pub struct MqttManager {
        client: EspMqttClient<'static>,
        _conn_handle: std::thread::JoinHandle<()>,
    }

    impl MqttManager {
        pub fn new(
            url: &str,
            client_id: Option<&str>,
            tx: mpsc::Sender<MqttMessage>,
        ) -> anyhow::Result<Self> {
            let (client, mut connection) = EspMqttClient::new(
                url,
                &MqttClientConfiguration {
                    client_id,
                    keep_alive_interval: Some(Duration::from_secs(30)),
                    ..Default::default()
                },
            )?;

            // Handle events in thread
            let _conn_handle = std::thread::Builder::new()
                .stack_size(8192)
                .spawn(move || {
                    let mut has_disconnected = false;
                    log::info!("MQTT Listening for messages");
                    while let Ok(event) = connection.next() {
                        log::info!("[Queue] Event: {}", event.payload());
                        match event.payload() {
                            EventPayload::Received {
                                topic: Some(t),
                                details: Details::Complete,
                                data,
                                ..
                            } => tx
                                .send(MqttMessage::Message(t.to_owned(), data.to_vec()))
                                .unwrap_or(()),
                            EventPayload::Connected(_) => {
                                if has_disconnected {
                                    log::info!("MQTT Reconnected");
                                    has_disconnected = false;
                                    tx.send(MqttMessage::Reconnected).unwrap_or(())
                                }
                            }
                            EventPayload::Disconnected => {
                                log::info!("MQTT disconnected");
                                has_disconnected = true;
                            }
                            _ => {}
                        }
                    }
                    log::info!("Connection closed");
                })?;

            // Allow thread to start processing events before returning
            std::thread::sleep(Duration::from_millis(500));

            Ok(Self {
                client,
                _conn_handle,
            })
        }

        pub fn subscribe(&mut self, topic: &str) -> anyhow::Result<()> {
            for i in 0..5 {
                if let Err(e) = self.client.subscribe(topic, QoS::AtMostOnce) {
                    log::error!("Failed to subscribe to topic: {topic} [{e}], retrying...");
                    // Back off retry
                    std::thread::sleep(Duration::from_millis(max(500, 500 * i * i)));
                    continue;
                };
                log::info!("Subscribed: {topic}");
                return Ok(());
            }
            Err(anyhow::anyhow!("Error subscribing to topic: {topic}"))
        }

        pub fn publish(
            &mut self,
            topic: &str,
            message: &[u8],
            retain: bool,
        ) -> anyhow::Result<u32> {
            self.client
                .enqueue(topic, QoS::AtMostOnce, retain, message)
                .map_err(|e| anyhow::anyhow!("MQTT Error: {e}"))
        }
    }
}
