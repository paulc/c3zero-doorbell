use std::thread;
use std::time::Duration;

use esp_idf_svc::http::server::{EspHttpConnection, Request};
use esp_idf_svc::http::Method;

use askama::Template;
use serde::{Deserialize, Serialize};

use doorbell::mqtt::{check_mqtt_url, MqttMessage, StaticMqttManager};
use doorbell::nvs::NVStore;
use doorbell::web::{FlashMsg, NavBar, WebServer};

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct MqttConfig {
    #[serde(default)]
    pub enabled: bool,
    pub url: String,
    pub client_id: String,
    pub ring_topic: String,
    pub status_topic: String,
}

pub struct MqttTask(MqttConfig);

impl MqttTask {
    pub fn init() -> anyhow::Result<Self> {
        Ok(Self(NVStore::get("mqtt")?.unwrap_or_default()))
    }

    pub fn run(&self) -> anyhow::Result<()> {
        if self.0.enabled {
            let mqtt_rx = StaticMqttManager::init(&self.0.url, Some(&self.0.client_id))?;

            log::info!("Starting MQTT Connection Thread");
            let _connection_t = thread::spawn(move || loop {
                match mqtt_rx.recv_timeout(Duration::from_secs(2)) {
                    Ok(MqttMessage::Reconnected) => {
                        log::info!("MQTT re-connected: resubscribing");
                        // Re-subscribe channels here
                    }
                    Ok(MqttMessage::Message(topic, data)) => {
                        let data = String::from_utf8_lossy(&data).to_string();
                        log::info!("mqtt_rx: {topic} : {data}");
                    }
                    _ => {}
                }
            });

            let wifi_topic = format!("{}/wifi", self.0.status_topic);
            log::info!("Starting MQTT Status Thread");
            let _update_t = thread::spawn(move || loop {
                let wifi_state = match crate::WIFI_STATE.try_lock() {
                    Ok(wifi_state) => wifi_state.to_string(),
                    Err(_) => "<Unknown>".to_string(),
                };
                let _ = StaticMqttManager::publish(&wifi_topic, wifi_state.as_bytes(), false);
                thread::sleep(Duration::from_secs(30));
            });
        }
        Ok(())
    }

    pub fn ring_msg(&self, state: bool) -> anyhow::Result<u32> {
        if self.0.enabled {
            StaticMqttManager::publish(
                &self.0.ring_topic,
                if state {
                    "ON".as_bytes()
                } else {
                    "OFF".as_bytes()
                },
                true,
            )
        } else {
            Ok(0)
        }
    }

    pub fn add_handlers(
        &self,
        server: &mut WebServer,
        navbar: NavBar<'static>,
    ) -> anyhow::Result<()> {
        server.add_handler("/mqtt", Method::Get, mqtt_handler(&navbar))?;
        server.add_handler("/mqtt", Method::Post, mqtt_submit)?;
        Ok(())
    }
}

#[derive(askama::Template)]
#[template(path = "mqtt.html")]
struct MqttPage<'a> {
    title: &'a str,
    config: MqttConfig,
    navbar: NavBar<'static>,
}

pub fn mqtt_handler(
    navbar: &NavBar<'static>,
) -> impl for<'r> Fn(Request<&mut EspHttpConnection<'r>>) -> anyhow::Result<()> + Send + 'static {
    let navbar = navbar.clone();
    move |request| {
        let mqtt_config = NVStore::get("mqtt")?.unwrap_or_default();
        let mqtt_page = MqttPage {
            title: "MQTT Settings",
            config: mqtt_config,
            navbar: navbar.clone(),
        };
        let mut response = request.into_response(200, Some("OK"), &[])?;
        let html = mqtt_page.render()?;
        response.write(html.as_bytes())?;
        Ok::<(), anyhow::Error>(())
    }
}

pub fn mqtt_submit(mut request: Request<&mut EspHttpConnection>) -> anyhow::Result<()> {
    let mut buf = [0_u8; 1024];
    let len = request.read(&mut buf)?;

    match serde_urlencoded::from_bytes::<MqttConfig>(&buf[0..len]) {
        Ok(c) => {
            log::info!("MQTT Config: >>{c:?}");
            // Check config
            if !check_mqtt_url(&c.url) {
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
            // Update NVS
            NVStore::set::<MqttConfig>("mqtt", &c)?;
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
