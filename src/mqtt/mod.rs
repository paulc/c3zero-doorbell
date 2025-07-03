use esp_idf_svc::mqtt::client::{
    Details, EspMqttClient, EventPayload, MqttClientConfiguration, QoS,
};

use std::sync::mpsc;

pub struct MqttManager {
    client: EspMqttClient<'static>,
    topics: Vec<String>,
}

impl MqttManager {
    pub fn new_cb<F>(url: &str, client_id: &str, callback: F) -> anyhow::Result<Self>
    where
        F: Fn(&str, &[u8]) + Send + 'static,
    {
        log::info!("Creating MQTT Client:");
        let rand = unsafe { esp_idf_svc::hal::sys::esp_random() };
        let client_id = format!("{client_id}-{rand}");
        match EspMqttClient::new_cb(
            url,
            &MqttClientConfiguration {
                client_id: Some(&client_id),

                ..Default::default()
            },
            move |e| {
                let e = e.payload();
                log::info!("MQTT Event: {e:?}");
                match e {
                    EventPayload::Received {
                        topic: Some(t),
                        details: Details::Complete,
                        data,
                        ..
                    } => callback(t, data),
                    EventPayload::Disconnected => (), // XXX Handle disconnect
                    _ => (),
                }
            },
        ) {
            Ok(client) => {
                log::info!("Created Client:");
                Ok(MqttManager {
                    client,
                    topics: vec![],
                })
            }
            Err(e) => Err(anyhow::anyhow!("Error creating MQTT client: {e}")),
        }
    }

    pub fn new_chan(
        url: &str,
        client_id: &str,
        tx: mpsc::Sender<(String, Vec<u8>)>,
    ) -> anyhow::Result<Self> {
        log::info!("Creating MQTT Client:");
        let rand = unsafe { esp_idf_svc::hal::sys::esp_random() };
        let client_id = format!("{client_id}-{rand}");
        match EspMqttClient::new_cb(
            url,
            &MqttClientConfiguration {
                client_id: Some(&client_id),

                ..Default::default()
            },
            move |e| {
                let e = e.payload();
                log::info!("MQTT Event: {e:?}");
                match e {
                    EventPayload::Received {
                        topic: Some(t),
                        details: Details::Complete,
                        data,
                        ..
                    } => tx.send((t.to_owned(), data.to_vec())).unwrap_or(()),
                    EventPayload::Disconnected => (), // XXX Handle disconnect
                    _ => (),
                }
            },
        ) {
            Ok(client) => {
                log::info!("Created Client:");
                Ok(MqttManager {
                    client,
                    topics: vec![],
                })
            }
            Err(e) => Err(anyhow::anyhow!("Error creating MQTT client: {e}")),
        }
    }

    pub fn subscribe(&mut self, topic: &str) -> anyhow::Result<()> {
        self.client
            .subscribe(topic, QoS::AtMostOnce)
            .map(|id| {
                log::info!("Subscribed: {topic} [{id}]");
                self.topics.push(topic.to_owned())
            })
            .map_err(|e| anyhow::anyhow!("MQTT Error: {e}"))
    }

    pub fn unsubscribe(&mut self, topic: &str) -> anyhow::Result<()> {
        self.client
            .unsubscribe(topic)
            .map(|_| {
                log::info!("Unsubscribed: {topic}");
                self.topics.retain(|t| t != topic)
            })
            .map_err(|e| anyhow::anyhow!("MQTT Error: {e}"))
    }

    pub fn send(&mut self, topic: &str, payload: &[u8], retain: bool) -> anyhow::Result<()> {
        self.client
            .enqueue(topic, QoS::AtMostOnce, retain, payload)
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!("MQTT Error: {e}"))
    }

    pub fn check_url(url: &str) -> bool {
        EspMqttClient::new(url, &Default::default()).is_ok()
    }
}
