use esp_idf_svc::mqtt::client::{
    Details, EspMqttClient, EventPayload, MqttClientConfiguration, QoS,
};

pub struct MqttManager {
    client: EspMqttClient<'static>,
    topics: Vec<String>,
}

impl MqttManager {
    pub fn new<F>(url: &str, client_id: &str, callback: F) -> anyhow::Result<Self>
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
            .and_then(|id| {
                log::info!("Subscribed: {topic} [{id}]");
                Ok(self.topics.push(topic.to_owned()))
            })
            .map_err(|e| anyhow::anyhow!("MQTT Error: {e}"))
    }

    pub fn unsubscribe(&mut self, topic: &str) -> anyhow::Result<()> {
        self.client
            .unsubscribe(topic)
            .and_then(|_| {
                log::info!("Unsubscribed: {topic}");
                Ok(self.topics.retain(|t| t != topic))
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
        match EspMqttClient::new(url, &Default::default()) {
            Ok(_) => true,
            Err(_) => false,
        }
    }
}
