use std::sync::atomic::{AtomicBool, Ordering};

use embedded_svc::http::client::Client as HttpClient;
use embedded_svc::io::Write;
use esp_idf_svc::http::client::{Configuration as HttpConfiguration, EspHttpConnection};
use esp_idf_svc::http::server;
use esp_idf_svc::http::Method;

use askama::Template;
use serde::{Deserialize, Serialize};

use doorbell::nvs::NVStore;
use doorbell::web::{FlashMsg, WebServer};

use crate::NavBar;

static SETTINGS_UPDATED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Serialize, Deserialize, Debug)]
struct PushoverConfig {
    #[serde(default)]
    enabled: bool,
    url: String,
    token: String,
    user: String,
    message: String,
}

impl Default for PushoverConfig {
    fn default() -> Self {
        Self {
            url: "https://api.pushover.net/1/messages.json".to_string(),
            token: String::new(),
            user: String::new(),
            message: "DOORBELL".to_string(),
            enabled: false,
        }
    }
}

#[derive(Serialize, Debug)]
struct PushoverMessage<'a> {
    token: &'a str,
    user: &'a str,
    message: &'a str,
}

pub struct PushoverSender {
    config: PushoverConfig,
}

impl PushoverSender {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            config: NVStore::get("pushover")?.unwrap_or_default(),
        })
    }
    pub fn send_ring_msg(&mut self) -> anyhow::Result<Option<u16>> {
        let message = self.config.message.clone();
        self.send(&message)
    }
    pub fn send(&mut self, msg: &str) -> anyhow::Result<Option<u16>> {
        if self.config.enabled {
            // Create client for each request as otherwise can panic
            // if network connection dropped
            let http_config = HttpConfiguration {
                crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
                ..Default::default()
            };
            let mut client = HttpClient::wrap(EspHttpConnection::new(&http_config)?);

            let payload = PushoverMessage {
                token: &self.config.token,
                user: &self.config.user,
                message: msg,
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

            let mut request = client.post(&self.config.url, &headers)?;

            request.write_all(&payload)?;
            request.flush()?;
            log::info!("HTTP Request -> POST {}", self.config.url);

            match request.submit() {
                Ok(response) => {
                    log::info!("HTTP Response <- {}", response.status());
                    Ok(Some(response.status()))
                }
                Err(e) => {
                    log::error!("HTTP Error: {e}");
                    Ok(None)
                }
            }
        } else {
            Ok(None)
        }
    }
    pub fn add_handlers(
        &self,
        server: &mut WebServer,
        navbar: NavBar<'static>,
    ) -> anyhow::Result<()> {
        server.add_handler("/pushover", Method::Get, pushover_handler(&navbar))?;
        server.add_handler("/pushover", Method::Post, pushover_submit)?;
        server.add_handler("/pushover/test", Method::Post, pushover_test)?;
        Ok(())
    }
}

#[derive(askama::Template)]
#[template(path = "pushover.html")]
struct PushoverPage<'a> {
    title: &'a str,
    config: PushoverConfig,
    updated: bool,
    navbar: NavBar<'static>,
}

pub fn pushover_handler(
    navbar: &NavBar<'static>,
) -> impl for<'r> Fn(server::Request<&mut server::EspHttpConnection<'r>>) -> anyhow::Result<()>
       + Send
       + 'static {
    let navbar = navbar.clone();
    move |request| {
        let pushover_config = NVStore::get("pushover")?.unwrap_or_default();
        let mqtt_page = PushoverPage {
            title: "Pushover Settings",
            config: pushover_config,
            updated: SETTINGS_UPDATED.load(Ordering::Relaxed),
            navbar: navbar.clone(),
        };
        let mut response = request.into_response(200, Some("OK"), &[])?;
        let html = mqtt_page.render()?;
        response.write(html.as_bytes())?;
        Ok::<(), anyhow::Error>(())
    }
}

pub fn pushover_submit(
    mut request: server::Request<&mut server::EspHttpConnection>,
) -> anyhow::Result<()> {
    let mut buf = [0_u8; 1024];
    let len = request.read(&mut buf)?;

    match serde_urlencoded::from_bytes::<PushoverConfig>(&buf[0..len]) {
        Ok(c) => {
            log::info!("MQTT Config: >>{c:?}");
            // Update NVS
            NVStore::set::<PushoverConfig>("pushover", &c)?;
            // Set static update flag
            SETTINGS_UPDATED.store(true, Ordering::Relaxed);

            let flash = serde_json::to_string(&FlashMsg {
                level: "success",
                message: "Successfully updated Pushover settings",
            })?;
            request.into_response(
                302,
                Some("Successfully updated Pushover MQTT settings"),
                &[
                    ("Location", "/pushover"),
                    ("Set-Cookie", &format!("flash_msg={flash}; path=/")),
                ],
            )?;
        }
        Err(e) => {
            log::info!("Error decoding MQTT config: {e}");
            let flash = serde_json::to_string(&FlashMsg {
                level: "error",
                message: &format!("Error updating Pushover settings: {e}"),
            })?;
            request.into_response(
                302,
                Some("Error updating Pushover settings"),
                &[
                    ("Location", "/pushover"),
                    ("Set-Cookie", &format!("flash_msg={flash}; path=/")),
                ],
            )?;
        }
    }
    Ok::<(), anyhow::Error>(())
}

#[derive(Debug, Serialize, Deserialize)]
struct PushoverTest {
    url: Option<String>,
    token: Option<String>,
    user: Option<String>,
    message: String,
}

pub fn pushover_test(
    mut request: server::Request<&mut server::EspHttpConnection>,
) -> anyhow::Result<()> {
    let mut buf = [0_u8; 1024];
    let len = request.read(&mut buf)?;

    log::info!("pushover_test: {}", String::from_utf8_lossy(&buf[0..len]));

    match serde_json::from_slice::<PushoverTest>(&buf[0..len]) {
        Ok(c) => {
            let mut pushover = PushoverSender::new()?;
            pushover.config.enabled = true;
            if let Some(t) = c.token {
                pushover.config.token = t;
            }
            if let Some(u) = c.user {
                pushover.config.user = u;
            }
            let flash = match pushover.send(&c.message) {
                Ok(Some(r)) => serde_json::to_string(&FlashMsg {
                    level: "success",
                    message: &format!("Pushover Status: {r}"),
                })?,
                Ok(None) => serde_json::to_string(&FlashMsg {
                    level: "error",
                    message: "EspIoError",
                })?,
                Err(e) => serde_json::to_string(&FlashMsg {
                    level: "error",
                    message: &format!("Pushover Error: {e}"),
                })?,
            };
            request.into_response(
                302,
                Some("Pushover Test"),
                &[
                    ("Location", "/pushover"),
                    ("Set-Cookie", &format!("flash_msg={flash}; path=/")),
                ],
            )?;
        }
        Err(e) => {
            log::error!("Error: {e}");
            request.into_status_response(400)?;
        }
    }
    Ok::<(), anyhow::Error>(())
}
