use embedded_svc::http::client::Client as HttpClient;
use embedded_svc::io::Write;
use esp_idf_svc::http::client::{Configuration as HttpConfiguration, EspHttpConnection};
use esp_idf_svc::mqtt::client::{EspMqttClient, MqttClientConfiguration, QoS};

use std::sync::mpsc;

use crate::adc;
use crate::wifi;

use serde::Serialize;

// XXX Development version - use dev MQTT server & dont send Pushover notifications

// MQTT Client
const MQTT_URL: &str = "mqtt://192.168.60.1:10883";
const MQTT_CLIENT_ID: &str = "Esp32c3-Doorbell-Dev";
const MQTT_TOPIC: &str = "doorbell/ring";
const MQTT_TOPIC_STATUS: &str = "doorbell/status";

#[allow(dead_code)]
const URL: &str = "https://api.pushover.net/1/messages.json";
#[allow(dead_code)]
const TOKEN: &str = "amfa9dzeck8bongtab3nrta3xux3hj";
#[allow(dead_code)]
const USER: &str = "uomfetdtawqotwp3ii9jpf4buys3p4";
#[allow(dead_code)]
const MESSAGE: &str = "DOORBELL";

#[derive(Debug)]
pub enum AlertMessage {
    RingStart(adc::Stats),
    RingStop,
    Status,
}

#[allow(dead_code)]
#[derive(Serialize, Debug)]
struct PushoverMessage<'a> {
    token: &'a str,
    user: &'a str,
    message: &'a str,
}

pub fn alert_task(rx: mpsc::Receiver<AlertMessage>) -> anyhow::Result<()> {
    // HTTP Client
    let _http_config = &HttpConfiguration {
        crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
        ..Default::default()
    };

    let (mut mqtt_client, _) = EspMqttClient::new(
        MQTT_URL,
        &MqttClientConfiguration {
            client_id: Some(MQTT_CLIENT_ID),
            ..Default::default()
        },
    )?;

    send_status(&mut mqtt_client);

    match mqtt_client.enqueue(MQTT_TOPIC, QoS::AtMostOnce, true, "OFF".as_bytes()) {
        Ok(_id) => log::info!("MQTT Send: {MQTT_TOPIC} = OFF"),
        Err(e) => log::error!("MQTT Error; {e}"),
    }

    loop {
        match rx.recv() {
            Ok(AlertMessage::RingStart(s)) => {
                // Send MQTT Update
                match mqtt_client.enqueue(MQTT_TOPIC, QoS::AtMostOnce, true, "ON".as_bytes()) {
                    Ok(_id) => log::info!("MQTT Send: {MQTT_TOPIC} = ON"),
                    Err(e) => log::error!("MQTT Error: {e}"),
                }

                // Send Pushover Webhook
                // match send_pushover(http_config) {
                //     Ok(_) => {}
                //     Err(e) => log::error!("Error sending Pushover request: {e}"),
                // }

                match mqtt_client.enqueue(
                    &format!("{MQTT_TOPIC_STATUS}/ring_stats"),
                    QoS::AtMostOnce,
                    false,
                    s.to_string().as_bytes(),
                ) {
                    Ok(_id) => log::info!("MQTT Send: {MQTT_TOPIC_STATUS}/ring_stats"),
                    Err(e) => log::error!("MQTT Error: {e}"),
                }
            }
            Ok(AlertMessage::RingStop) => {
                // Send MQTT Update
                match mqtt_client.enqueue(MQTT_TOPIC, QoS::AtMostOnce, true, "OFF".as_bytes()) {
                    Ok(_id) => log::info!("MQTT Send: {MQTT_TOPIC} = OFF"),
                    Err(e) => log::error!("MQTT Error; {e}"),
                }
            }
            Ok(AlertMessage::Status) => send_status(&mut mqtt_client),
            Err(e) => {
                log::error!("ERROR :: alert_task :: {e:?}");
            }
        }
    }
}

fn send_status(mqtt: &mut EspMqttClient<'static>) {
    let alarm_ip = if let Ok(Some(ip)) = wifi::IP_INFO.get_cloned() {
        ip.ip.to_string()
    } else {
        "<Unknown IP>".to_string()
    };

    let stats = if let Ok(Some(s)) = adc::ADC_STATS.get_cloned() {
        s.to_string()
    } else {
        "<No Stats>".to_string()
    };

    match mqtt.enqueue(
        &format!("{MQTT_TOPIC_STATUS}/ip"),
        QoS::AtMostOnce,
        false,
        alarm_ip.as_bytes(),
    ) {
        Ok(_id) => log::info!("MQTT Send: {MQTT_TOPIC_STATUS}/ip"),
        Err(e) => log::error!("MQTT Error: {e}"),
    }

    match mqtt.enqueue(
        &format!("{MQTT_TOPIC_STATUS}/stats"),
        QoS::AtMostOnce,
        false,
        stats.as_bytes(),
    ) {
        Ok(_id) => log::info!("MQTT Send: {MQTT_TOPIC_STATUS}/stats"),
        Err(e) => log::error!("MQTT Error: {e}"),
    }
}

fn _send_pushover(config: &HttpConfiguration) -> anyhow::Result<()> {
    let mut client = HttpClient::wrap(EspHttpConnection::new(config)?);

    let payload = PushoverMessage {
        token: TOKEN,
        user: USER,
        message: MESSAGE,
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

    let mut request = client.post(URL, &headers)?;

    request.write_all(&payload)?;
    request.flush()?;
    log::info!("HTTP Request -> POST {URL}");

    match request.submit() {
        Ok(response) => log::info!("HTTP Response <- {}", response.status()),
        Err(e) => log::error!("HTTP Error: {e}"),
    }

    Ok(())
}
