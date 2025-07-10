use core::time::Duration;

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
const MQTT_CLIENT_ID: &str = "esp-mqtt-demo";
const MQTT_TOPIC: &str = "esp-mqtt-demo";

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

    let mut mqtt = mqtt::MqttManager::new(MQTT_URL, MQTT_CLIENT_ID)?;
    mqtt.subscribe(MQTT_TOPIC)?;

    loop {
        mqtt.publish(MQTT_TOPIC, "TEST".as_bytes(), false)?;
        std::thread::sleep(Duration::from_millis(1000));
    }
}

mod mqtt {

    use esp_idf_svc::mqtt::client::{
        EspMqttClient,
        MqttClientConfiguration,
        QoS, // Details, EspMqttClient, EspMqttConnection, EventPayload, MqttClientConfiguration, QoS,
    };

    use core::time::Duration;
    use log::*;

    pub struct MqttManager {
        client: EspMqttClient<'static>,
        _conn_handle: std::thread::JoinHandle<()>,
    }

    impl MqttManager {
        pub fn new(url: &str, client_id: &str) -> anyhow::Result<Self> {
            let (client, mut connection) = EspMqttClient::new(
                url,
                &MqttClientConfiguration {
                    client_id: Some(client_id),
                    keep_alive_interval: Some(Duration::from_secs(30)),
                    ..Default::default()
                },
            )?;

            // Handle events in thread
            let _conn_handle = std::thread::Builder::new()
                .stack_size(8192)
                .spawn(move || {
                    info!("MQTT Listening for messages");
                    while let Ok(event) = connection.next() {
                        info!("[Queue] Event: {}", event.payload());
                    }
                    info!("Connection closed");
                })?;

            // Allow thread to start processing events before returning
            std::thread::sleep(Duration::from_millis(500));

            Ok(Self {
                client,
                _conn_handle,
            })
        }

        pub fn subscribe(&mut self, topic: &str) -> anyhow::Result<()> {
            loop {
                if let Err(e) = self.client.subscribe(topic, QoS::AtMostOnce) {
                    error!("Failed to subscribe to topic \"{topic}\": {e}, retrying...");
                    // Re-try in 0.5s
                    std::thread::sleep(Duration::from_millis(500));
                    continue;
                };
                info!("Subscribed to topic \"{topic}\"");
                break;
            }
            Ok(())
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
