use esp_idf_svc::http::server::{EspHttpConnection, Request};
use esp_idf_svc::mqtt::client::{EspMqttClient, MqttClientConfiguration};

use std::sync::Mutex;

use askama::Template;
use serde::{Deserialize, Serialize};

use doorbell::httpd::FlashMsg;
use doorbell::nvs::NVStore;

pub static MQTT_CONFIG: Mutex<Option<MQTTConfig>> = Mutex::new(None);

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct MQTTConfig {
    #[serde(default)]
    pub enabled: bool,
    pub url: String,
    pub client_id: String,
    pub ring_topic: String,
    pub status_topic: String,
}

#[derive(askama::Template)]
#[template(path = "mqtt.html")]
struct MqttPage<'a> {
    title: &'a str,
    config: MQTTConfig,
}

pub fn handle_mqtt(request: Request<&mut EspHttpConnection>) -> anyhow::Result<()> {
    let mqtt_config = match MQTT_CONFIG.get_cloned() {
        Ok(Some(c)) => c,
        _ => Default::default(),
    };
    let mqtt_page = MqttPage {
        title: "MQTT Settings",
        config: mqtt_config,
    };
    let mut response = request.into_response(200, Some("OK"), &[])?;
    let html = mqtt_page.render()?;
    response.write(html.as_bytes())?;
    Ok::<(), anyhow::Error>(())
}

pub fn handle_mqtt_submit(mut request: Request<&mut EspHttpConnection>) -> anyhow::Result<()> {
    let mut buf = [0_u8; 1024];
    let len = request.read(&mut buf)?;

    match serde_urlencoded::from_bytes::<MQTTConfig>(&buf[0..len]) {
        Ok(c) => {
            log::info!("MQTT Config: >>{c:?}");
            // Check config
            if !test_mqtt_settings(&c) {
                let flash = serde_json::to_string(&FlashMsg {
                    level: "error",
                    message: "Invalid MQTT URL",
                })?;
                request.into_response(
                    302,
                    Some("Error updating MQTT settings"),
                    &[
                        ("Location", "/mqtt"),
                        ("Set-Cookie", &format!("flash_msg={flash}; path=/")),
                    ],
                )?;
                return Ok::<(), anyhow::Error>(());
            }
            MQTT_CONFIG.set(Some(c.clone()))?;
            // Also update NVS
            NVStore::set::<MQTTConfig>("mqtt", &c)?;
            let flash = serde_json::to_string(&FlashMsg {
                level: "success",
                message: "Successfully updated MQTT settings",
            })?;
            request.into_response(
                302,
                Some("Successfully updated MQTT settings"),
                &[
                    ("Location", "/mqtt"),
                    ("Set-Cookie", &format!("flash_msg={flash}; path=/")),
                ],
            )?;
        }
        Err(e) => {
            log::info!("Error decoding MQTT config: {e}");
            let flash = serde_json::to_string(&FlashMsg {
                level: "error",
                message: &format!("Error updating MQTT settings: {e}"),
            })?;
            request.into_response(
                302,
                Some("Error updating MQTT settings"),
                &[
                    ("Location", "/mqtt"),
                    ("Set-Cookie", &format!("flash_msg={flash}; path=/")),
                ],
            )?;
        }
    }
    Ok::<(), anyhow::Error>(())
}

fn test_mqtt_settings(c: &MQTTConfig) -> bool {
    if let Ok((_mqtt_client, _)) = EspMqttClient::new(
        &c.url,
        &MqttClientConfiguration {
            client_id: Some(&c.client_id),
            ..Default::default()
        },
    ) {
        // We can only really test if URL looks ok here as mqtt_client.publish/enqueue
        // always returns/success
        true
    } else {
        false
    }
}
