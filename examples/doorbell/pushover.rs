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

#[derive(Clone, Serialize, Deserialize, Debug)]
struct PushoverConfig {
    #[serde(default)]
    enabled: bool,
    url: String,
    token: String,
    user: String,
    ring_message: String,
}

impl Default for PushoverConfig {
    fn default() -> Self {
        Self {
            url: "https://api.pushover.net/1/messages.json".to_string(),
            token: String::new(),
            user: String::new(),
            ring_message: "DOORBELL".to_string(),
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
    pub fn send_ring_msg(&mut self) -> anyhow::Result<()> {
        let ring_message = self.config.ring_message.clone();
        self.send(&ring_message)
    }
    pub fn send(&mut self, msg: &str) -> anyhow::Result<()> {
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
                Ok(response) => log::info!("HTTP Response <- {}", response.status()),
                Err(e) => log::error!("HTTP Error: {e}"),
            }

            Ok(())
        } else {
            Ok(())
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
    message: String,
}

pub fn pushover_test(
    mut request: server::Request<&mut server::EspHttpConnection>,
) -> anyhow::Result<()> {
    let mut buf = [0_u8; 1024];
    let len = request.read(&mut buf)?;

    log::info!("pushover_test: {}", String::from_utf8_lossy(&buf[0..len]));

    match serde_json::from_slice::<PushoverTest>(&buf[0..len]) {
        Ok(t) => {
            let mut pushover = PushoverSender::new()?;
            let flash = match pushover.send(&t.message) {
                Ok(_) => serde_json::to_string(&FlashMsg {
                    level: "success",
                    message: "Pushover Success",
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
