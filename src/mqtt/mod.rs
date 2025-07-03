use esp_idf_svc::mqtt::client::{
    Details, EspMqttClient, EventPayload, MqttClientConfiguration, QoS,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct MqttConfig {
    #[serde(default)]
    pub url: String,
    pub client_id: String,
    pub topic: Vec<String>,
}

pub struct MqttManager {
    config: MqttConfig,
    client: EspMqttClient<'static>,
}

impl MqttManager {
    pub fn new<F>(config: MqttConfig, callback: F) -> anyhow::Result<Self>
    where
        F: Fn(&str, &[u8]) + Send + 'static,
    {
        log::info!("Creating MQTT Client");
        let mut client = EspMqttClient::new_cb(
            &config.url,
            &MqttClientConfiguration {
                client_id: Some(&config.client_id),
                ..Default::default()
            },
            move |e| {
                let e = e.payload();
                log::info!("MQTT Event: {e:?}");
                if let EventPayload::Received {
                    topic: Some(t),
                    details: Details::Complete,
                    data,
                    ..
                } = e
                {
                    callback(t, data);
                }
            },
        )?;
        for t in &config.topic {
            log::info!("Subscribing: {t}");
            client.subscribe(t, QoS::AtMostOnce)?;
        }
        Ok(MqttManager { config, client })
    }

    pub fn subscribe(&mut self, topic: &str) -> anyhow::Result<()> {
        self.client
            .subscribe(topic, QoS::AtMostOnce)
            .and_then(|_| Ok(self.config.topic.push(topic.to_owned())))
            .map_err(|e| anyhow::anyhow!("MQTT Error: {e}"))
    }

    pub fn unsubscribe(&mut self, topic: &str) -> anyhow::Result<()> {
        self.client
            .unsubscribe(topic)
            .and_then(|_| Ok(self.config.topic.retain(|t| t != topic)))
            .map_err(|e| anyhow::anyhow!("MQTT Error: {e}"))
    }

    pub fn send(&mut self, topic: &str, payload: &[u8], retain: bool) -> anyhow::Result<()> {
        self.client
            .enqueue(topic, QoS::AtMostOnce, retain, payload)
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!("MQTT Error: {e}"))
    }
}
