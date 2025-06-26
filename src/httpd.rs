use askama::Template;
use esp_idf_svc::http;
use esp_idf_svc::http::server::{
    Configuration as HttpConfig, EspHttpConnection, EspHttpServer, Request,
};
use esp_idf_sys as _; // Import the ESP-IDF bindings

use crate::nvs::NVStore;
use crate::wifi::{APStore, WIFI_SCAN};

use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct FlashMsg<'a> {
    pub level: &'a str,
    pub message: &'a str,
}

impl<'a> FlashMsg<'a> {
    pub fn cookie(level: &'a str, message: &'a str) -> anyhow::Result<String> {
        Ok(format!(
            "flash_msg={}; path=/",
            serde_json::to_string(&FlashMsg { level, message })?
        ))
    }
}

#[derive(askama::Template)]
#[template(path = "wifi.html")]
struct WiFiConfig<'a> {
    visible: Vec<(&'a str, u8, i8, &'a str)>,
    aps: Vec<&'a str>,
}

#[derive(askama::Template)]
#[template(path = "reset_page.html")]
struct ResetPage {}

const REBOOT_DELAY_MS: u32 = 1000;

pub fn start_http_server<'a>() -> anyhow::Result<EspHttpServer<'a>> {
    log::info!("Starting HTTPD:");
    let config: HttpConfig = HttpConfig {
        uri_match_wildcard: true,
        ..Default::default()
    };
    let mut server = EspHttpServer::new(&config)?;

    server.fn_handler("/style.css", http::Method::Get, handle_style)?;
    server.fn_handler("/reset", http::Method::Get, handle_reset)?;
    server.fn_handler("/reset_page", http::Method::Get, handle_reset_page)?;
    server.fn_handler("/hello", http::Method::Get, handle_hello)?;
    server.fn_handler("/wifi", http::Method::Get, handle_wifi)?;
    server.fn_handler("/wifi/delete/*", http::Method::Get, handle_ap_delete)?;
    server.fn_handler("/wifi/add", http::Method::Post, handle_ap_add)?;
    server.fn_handler("/nvs/get/*", http::Method::Get, handle_nvs_get)?;
    server.fn_handler("/nvs/set/*", http::Method::Post, handle_nvs_set)?;
    server.fn_handler("/nvs/delete/*", http::Method::Get, handle_nvs_delete)?;

    log::info!("Web server started");

    Ok(server)
}

fn handle_style(request: Request<&mut EspHttpConnection>) -> anyhow::Result<()> {
    let mut response = request.into_response(
        200,
        Some("OK"),
        &[
            ("Content-Type", "text/css"),
            ("Cache-Control", "max-age=600"),
        ],
    )?;
    let css = std::include_bytes!("../templates/style.css");
    response.write(css)?;
    Ok(())
}

fn handle_hello(request: Request<&mut EspHttpConnection>) -> anyhow::Result<()> {
    let mut response = request.into_response(
        200,
        Some("OK"),
        &[
            ("Content-Type", "text/css"),
            ("Access-Control-Allow-Origin", "*"),
        ],
    )?;
    response.write("Hello from ESP32-C3!\n".as_bytes())?;
    Ok(())
}

fn handle_reset(request: Request<&mut EspHttpConnection>) -> anyhow::Result<()> {
    let mut response = request.into_response(
        200,
        Some("OK"),
        &[
            ("Content-Type", "text/plain"),
            ("Access-Control-Allow-Origin", "*"),
        ],
    )?;
    response.write("Rebooting\n".as_bytes())?;
    std::thread::spawn(|| {
        log::info!("Rebooting in {REBOOT_DELAY_MS}ms");
        esp_idf_hal::delay::FreeRtos::delay_ms(1000); // Give time for response to send
        log::info!("Rebooting now");
        esp_idf_hal::reset::restart();
    });
    Ok(())
}

fn handle_reset_page(request: Request<&mut EspHttpConnection>) -> anyhow::Result<()> {
    let reset_page = ResetPage {};
    let mut response = request.into_ok_response()?;
    let html = reset_page.render()?;
    response.write(html.as_bytes())?;
    Ok::<(), anyhow::Error>(())
}

fn handle_wifi(request: Request<&mut EspHttpConnection>) -> anyhow::Result<()> {
    let aps = match APStore::get_aps() {
        Ok(aps) => aps.collect::<Vec<_>>(),
        Err(e) => {
            log::info!("get_known_aps: {e:?}");
            vec![]
        }
    };
    let visible = WIFI_SCAN.lock().unwrap();
    let visible = visible
        .iter()
        .map(|ap| {
            (
                ap.ssid.as_str(),
                ap.channel,
                ap.signal_strength,
                match ap.auth_method {
                    Some(_) => "Some",
                    None => "None",
                },
            )
        })
        .collect::<Vec<_>>();
    let config_page = WiFiConfig {
        visible,
        aps: aps.iter().map(|s| s.ssid.as_str()).collect::<Vec<_>>(),
    };
    let mut response = request.into_ok_response()?;
    let html = config_page.render()?;
    response.write(html.as_bytes())?;
    Ok::<(), anyhow::Error>(())
}

fn handle_ap_delete(request: Request<&mut EspHttpConnection>) -> anyhow::Result<()> {
    log::info!("Delete AP: {:?}", request.uri());
    let ssid = request.uri().split('/').next_back().expect("Invalid SSID");
    let ssid = urlencoding::decode(ssid)?.clone().into_owned();

    let (level, message) = if APStore::get_ap_str(&ssid)?.is_some() {
        match APStore::delete_ap(&ssid) {
            Ok(_) => ("success", &format!("Successfully deleted SSID: {ssid}")),
            Err(e) => (
                "error",
                &format!("Error: Failed to delete SSID: {ssid} [{e}]"),
            ),
        }
    } else {
        ("error", &format!("Error: Invalid SSID {ssid}"))
    };

    log::info!("{level}: {message}");
    request.into_response(
        302,
        Some(message),
        &[
            ("Location", "/wifi"),
            ("Set-Cookie", &FlashMsg::cookie(level, message)?),
        ],
    )?;
    Ok::<(), anyhow::Error>(())
}

fn handle_ap_add(mut request: Request<&mut EspHttpConnection>) -> anyhow::Result<()> {
    // Read the body of the request
    let mut buf = [0_u8; 256];
    let len = request.read(&mut buf)?;

    match serde_urlencoded::from_bytes(&buf[0..len]) {
        Ok(config) => {
            // Save the WiFi configuration
            log::info!("Wifi Config: {config:?}");
            let (level, message) = match APStore::add_ap(&config) {
                Ok(_) => (
                    "success",
                    &format!("Successfully saved SSID: {}", config.ssid),
                ),
                Err(e) => (
                    "error",
                    &format!("Failed to save SSID: {} [{}]", config.ssid, e),
                ),
            };
            log::info!("{level}: {message}");
            request.into_response(
                302,
                Some(message),
                &[
                    ("Location", "/wifi"),
                    ("Set-Cookie", &FlashMsg::cookie(level, message)?),
                ],
            )?;
        }
        Err(_) => {
            log::error!("Invalid form data");
            request.into_response(400, Some("Invalid form data"), &[])?;
        }
    }
    Ok::<(), anyhow::Error>(())
}

fn handle_nvs_get(request: Request<&mut EspHttpConnection>) -> anyhow::Result<()> {
    let key = request.uri().split('/').next_back().expect("Invalid Key");
    let key = urlencoding::decode(key)?;
    log::info!("NVS_GET: {key:?}");
    match NVStore::get_raw(&key) {
        Ok(Some(v)) => {
            let mut response =
                request.into_response(200, Some("OK"), &[("Content-Type", "application/json")]);
            if let Ok(ref mut r) = response {
                r.write(&v)?;
                r.write(b"\r\n")?;
            }
            response
        }
        Ok(None) => request.into_response(
            404,
            Some("Key not found"),
            &[("Content-Type", "text/plain")],
        ),
        Err(e) => request.into_response(500, Some(&e.to_string()), &[]),
    }
    .map(|_| ())
    .map_err(|e| anyhow::anyhow!("Http Error: {e}"))
}

fn handle_nvs_delete(request: Request<&mut EspHttpConnection>) -> anyhow::Result<()> {
    let key = request.uri().split('/').next_back().expect("Invalid Key");
    let key = urlencoding::decode(key)?;
    log::info!("NVS_DELETE: {key:?}");
    match NVStore::delete(&key) {
        Ok(_) => request.into_response(200, Some("OK"), &[("Content-Type", "application/json")]),
        Err(e) => request.into_response(500, Some(&e.to_string()), &[]),
    }
    .map(|_| ())
    .map_err(|e| anyhow::anyhow!("Http Error: {e}"))
}

fn handle_nvs_set(mut request: Request<&mut EspHttpConnection>) -> anyhow::Result<()> {
    // Read the body of the request
    let mut buf = [0_u8; 1024];
    let len = request.read(&mut buf)?;

    let key = request.uri().split('/').next_back().expect("Invalid Key");
    let key = urlencoding::decode(key)?;
    log::info!("NVS_SET: {key}: {}", String::from_utf8_lossy(&buf));

    match request.header("Content-Type") {
        Some("application/json") => match NVStore::set_raw(&key, &buf[0..len]) {
            Ok(_) => request.into_ok_response(),
            Err(e) => {
                log::error!("NVS_SET: {e}");
                request.into_response(400, Some(&e.to_string()), &[])
            }
        },
        _ => request.into_response(400, Some("Invalid Content-Type"), &[]),
    }
    .map(|_| ())
    .map_err(|e| anyhow::anyhow!("Http Error: {e}"))
}
