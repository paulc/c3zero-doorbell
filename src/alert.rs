use embedded_svc::http::client::Client as HttpClient;
use embedded_svc::io::Write;
use esp_idf_svc::http::client::{Configuration as HttpConfiguration, EspHttpConnection};
use esp_idf_svc::mqtt::client::{EspMqttClient, MqttClientConfiguration, QoS};

use std::sync::mpsc;

use serde::Serialize;

#[derive(Debug)]
pub enum AlertMessage {
    RingStart,
    RingStop,
}

#[derive(Serialize, Debug)]
struct PushoverMessage<'a> {
    token: &'a str,
    user: &'a str,
    message: &'a str,
}

pub fn alert_task(rx: mpsc::Receiver<AlertMessage>) -> anyhow::Result<()> {
    // HTTP Client
    let config = &HttpConfiguration {
        crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
        ..Default::default()
    };

    // Pushover API payload
    let url = "https://api.pushover.net/1/messages.json";
    let token = "amfa9dzeck8bongtab3nrta3xux3hj";
    let user = "uomfetdtawqotwp3ii9jpf4buys3p4";
    let message = "DOORBELL";

    // MQTT Client
    const MQTT_URL: &str = "mqtt://192.168.60.1:1883";
    const MQTT_CLIENT_ID: &str = "Esp32c3-Doorbell";
    const MQTT_TOPIC: &str = "doorbell/ring";
    let (mut mqtt_client, _) = EspMqttClient::new(
        MQTT_URL,
        &MqttClientConfiguration {
            client_id: Some(MQTT_CLIENT_ID),
            ..Default::default()
        },
    )?;
    mqtt_client.enqueue(MQTT_TOPIC, QoS::AtMostOnce, false, "OFF".as_bytes())?;

    loop {
        match rx.recv() {
            Ok(AlertMessage::RingStart) => {
                // Send MQTT Update
                mqtt_client.enqueue(MQTT_TOPIC, QoS::AtMostOnce, true, "ON".as_bytes())?;

                // Send Webhook
                let mut client = HttpClient::wrap(EspHttpConnection::new(config)?);

                let payload = PushoverMessage {
                    token,
                    user,
                    message,
                };
                log::info!("Sending Pushover message: {payload:?}");

                // Convert to JSON
                let payload = serde_json::to_vec(&payload)?;

                // Prepare headers and URL
                let content_length_header = format!("{}", payload.len());
                let headers = [
                    ("content-type", "application/json"),
                    ("content-length", content_length_header.as_str()),
                    ("accept", "application/json"),
                ];

                let mut request = client.post(url, &headers)?;

                request.write_all(&payload)?;
                request.flush()?;
                log::info!("-> POST {url}");

                let response = request.submit()?;
                log::info!("<- {}", response.status());
            }
            Ok(AlertMessage::RingStop) => {
                // Send MQTT Update
                mqtt_client.enqueue(MQTT_TOPIC, QoS::AtMostOnce, true, "OFF".as_bytes())?;
            }
            Err(e) => {
                log::error!("ERROR :: alert_task :: {e:?}");
            }
        }
    }
}
