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

pub struct MqttSubscribe {
    _config: MqttConfig,
    client: EspMqttClient<'static>,
}

impl MqttSubscribe {
    pub fn new<F>(config: MqttConfig, callback: F) -> anyhow::Result<Self>
    where
        F: Fn(String, String) + Send + 'static,
    {
        log::info!("Creating MQTT Client");
        let topic = config.topic.clone();
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
                    topic: Some(rx),
                    details: Details::Complete,
                    data,
                    ..
                } = e
                {
                    topic.iter().filter(|&t| t == rx).for_each(|t| {
                        callback(t.to_string(), String::from_utf8_lossy(data).to_string());
                    })
                }
            },
        )?;
        for t in &config.topic {
            log::info!("Subscribing: {t}");
            client.subscribe(t, QoS::AtMostOnce)?;
        }
        Ok(MqttSubscribe {
            _config: config,
            client,
        })
    }

    pub fn enqueue(&mut self, topic: &str, retain: bool, payload: &[u8]) -> anyhow::Result<()> {
        self.client
            .enqueue(topic, QoS::AtMostOnce, retain, payload)
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!("MQTT Error: {e}"))
    }
}
