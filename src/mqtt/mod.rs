use esp_idf_svc::mqtt::client::{
    Details, EspMqttClient, EventPayload, MqttClientConfiguration, QoS,
};

use core::time::Duration;
use std::sync::{mpsc, Mutex};

const MQTT_RETRY_COUNT: u32 = 5;

pub enum MqttMessage {
    Message(String, Vec<u8>),
    Reconnected,
}

static MQTT_MANAGER: Mutex<Option<MqttManager>> = Mutex::new(None);

pub struct StaticMqttManager {}

impl StaticMqttManager {
    pub fn init(url: &str, client_id: Option<&str>) -> anyhow::Result<mpsc::Receiver<MqttMessage>> {
        let (tx, rx) = mpsc::channel::<MqttMessage>();
        let mqtt_manager = MqttManager::new(url, client_id, tx)?;
        MQTT_MANAGER
            .replace(Some(mqtt_manager))
            .map_err(|e| anyhow::anyhow!("Mutex Error: {e}"))?;
        Ok(rx)
    }
    pub fn subscribe(topic: &str) -> anyhow::Result<()> {
        MQTT_MANAGER
            .lock()
            .map_err(|e| anyhow::anyhow!("Mutex Error: {e}"))?
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("MQTT_MANAGER not initialised"))?
            .subscribe(topic)
    }
    pub fn unsubscribe(topic: &str) -> anyhow::Result<()> {
        MQTT_MANAGER
            .lock()
            .map_err(|e| anyhow::anyhow!("Mutex Error: {e}"))?
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("MQTT_MANAGER not initialised"))?
            .unsubscribe(topic)
    }
    pub fn publish(topic: &str, message: &[u8], retain: bool) -> anyhow::Result<u32> {
        MQTT_MANAGER
            .lock()
            .map_err(|e| anyhow::anyhow!("Mutex Error: {e}"))?
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("MQTT_MANAGER not initialised"))?
            .publish(topic, message, retain)
    }
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
        log::info!("Creating MqttClient: {url}");
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
                // TODO Remove has_disconnected test
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
        for _ in 0..=MQTT_RETRY_COUNT {
            match self.client.subscribe(topic, QoS::AtMostOnce) {
                Ok(_) => {
                    log::info!("Subscribed: {topic}");
                    return Ok(());
                }
                Err(e) => {
                    log::error!("Failed to subscribe to topic: {topic} [{e}], retrying...");
                    std::thread::sleep(Duration::from_millis(500));
                }
            }
        }
        Err(anyhow::anyhow!("Error subscribing to topic: {topic}"))
    }

    pub fn unsubscribe(&mut self, topic: &str) -> anyhow::Result<()> {
        self.client
            .unsubscribe(topic)
            .map(|_| {
                log::info!("Unsubscribed: {topic}");
            })
            .map_err(|e| anyhow::anyhow!("MQTT Error: {e}"))
    }

    pub fn publish(&mut self, topic: &str, message: &[u8], retain: bool) -> anyhow::Result<u32> {
        self.client
            .enqueue(topic, QoS::AtMostOnce, retain, message)
            .map_err(|e| anyhow::anyhow!("MQTT Error: {e}"))
    }
}

pub fn check_mqtt_url(url: &str) -> bool {
    EspMqttClient::new(url, &Default::default()).is_ok()
}
