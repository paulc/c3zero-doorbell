use esp_idf_svc::http::server::{EspHttpConnection, Request};

use askama::Template;

use crate::web::FlashMsg;
use crate::wifi::{APStore, WIFI_SCAN};

#[derive(askama::Template)]
#[template(path = "wifi.html")]
struct WiFiConfig<'a> {
    visible: Vec<(&'a str, u8, i8, String)>,
    aps: Vec<&'a str>,
    navbar: crate::web::NavBar<'static>,
}

pub fn wifi_handler(
    navbar: crate::web::NavBar<'static>,
) -> impl for<'r> Fn(Request<&mut EspHttpConnection<'r>>) -> anyhow::Result<()> + Send + 'static {
    move |request| {
        let aps = APStore::get_aps()?;
        let visible = WIFI_SCAN.lock().unwrap();
        let visible = visible
            .iter()
            .map(|ap| {
                (
                    ap.ssid.as_str(),
                    ap.channel,
                    ap.signal_strength,
                    match ap.auth_method {
                        Some(m) => format!("{m:?}"),
                        None => "None".to_string(),
                    },
                )
            })
            .collect::<Vec<_>>();
        let config_page = WiFiConfig {
            visible,
            aps: aps.iter().map(|s| s.ssid.as_str()).collect::<Vec<_>>(),
            navbar: navbar.clone(),
        };
        let mut response = request.into_ok_response()?;
        let html = config_page.render()?;
        response.write(html.as_bytes())?;
        Ok::<(), anyhow::Error>(())
    }
}

pub fn ap_delete_handler(
) -> impl for<'r> Fn(Request<&mut EspHttpConnection<'r>>) -> anyhow::Result<()> + Send + 'static {
    move |request| {
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
}

pub fn ap_add_handler(
) -> impl for<'r> Fn(Request<&mut EspHttpConnection<'r>>) -> anyhow::Result<()> + Send + 'static {
    move |mut request| {
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
}
