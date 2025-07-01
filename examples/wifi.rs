#![feature(lock_value_accessors)]

use esp_idf_hal::gpio::OutputPin;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::prelude::*;
use esp_idf_svc::http::Method;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::EspWifi;

use std::thread;
use std::time::Duration;

use doorbell::nvs::NVStore;
use doorbell::web::{NavBar, NavLink, WebServer};
use doorbell::wifi::{APConfig, APStore, WifiManager};
use doorbell::ws2812::{colour, RgbLayout, Ws2812RmtSingle};

const AP_SSID: &str = "ESP32C3-AP";
const AP_PASSWORD: &str = "password";

// Static NavBar
pub const NAVBAR: NavBar = NavBar {
    title: "MQTT Alarm",
    links: &[
        NavLink {
            url: "/wifi",
            label: "Wifi Configuration",
        },
        NavLink {
            url: "/sse_page",
            label: "SSE Test",
        },
        NavLink {
            url: "/reset_page",
            label: "Reset",
        },
    ],
};

fn main() -> anyhow::Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();
    log::info!("Starting...");

    // Initialise peripherals
    let peripherals = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs_default_partition = EspDefaultNvsPartition::take()?;

    // Initialise NVStore
    let nvs = NVStore::init(nvs_default_partition.clone(), "DOORBELL")?;

    // Initialise WiFi
    let mut wifi = WifiManager::new(EspWifi::new(
        peripherals.modem,
        sys_loop.clone(),
        Some(nvs_default_partition.clone()),
    )?)?;

    // Onboard WS2812 (GPIO10)
    let ws2812 = peripherals.pins.gpio10.downgrade_output();
    let channel = peripherals.rmt.channel0;
    let mut led = Ws2812RmtSingle::new(ws2812, channel, RgbLayout::Grb)?;
    led.set(colour::OFF)?;

    // Try to connect to known AP (or start local AP)
    wifi.scan()?;
    let wifi_state = wifi.try_connect(
        &APStore::get_aps()?,
        Some(APConfig::new(AP_SSID, AP_PASSWORD)?),
        20_000,
    )?;
    log::info!("WifiState: {wifi_state:?}");

    // Start web server and attach routes
    let mut web = WebServer::new(NAVBAR)?;
    nvs.add_handlers(&mut web, NAVBAR)?;
    wifi.add_handlers(&mut web, NAVBAR)?;

    web.add_handler(
        "/",
        Method::Get,
        home_page::make_handler(&wifi_state, NAVBAR),
    )?;

    web.add_handler("/sse", Method::Get, sse::make_sse_handler())?;
    web.add_handler("/sse_page", Method::Get, sse::make_sse_page(NAVBAR))?;

    loop {
        thread::sleep(Duration::from_millis(500));
    }
}

mod sse {
    use esp_idf_svc::http::server::{EspHttpConnection, Request};

    use askama::Template;

    use doorbell::web::NavBar;

    #[derive(Template)]
    #[template(path = "sse_test.html")]
    struct SSEPage {
        navbar: NavBar<'static>,
    }

    pub fn make_sse_page(
        navbar: NavBar<'static>,
    ) -> impl for<'r> Fn(Request<&mut EspHttpConnection<'r>>) -> anyhow::Result<()> + Send + 'static
    {
        move |request| {
            let sse_page = SSEPage {
                navbar: navbar.clone(),
            };
            let mut response = request.into_response(200, Some("OK"), &[])?;
            let html = sse_page.render()?;
            response.write(html.as_bytes())?;

            Ok::<(), anyhow::Error>(())
        }
    }

    pub fn make_sse_handler(
    ) -> impl for<'r> Fn(Request<&mut EspHttpConnection<'r>>) -> anyhow::Result<()> + Send + 'static
    {
        move |request| {
            let mut response = request.into_response(
                200,
                Some("OK"),
                &[
                    ("Content-Type", "text/event-stream"),
                    ("Connection", "keep-alive"),
                    ("Access-Control-Allow-Origin", "*"),
                ],
            )?;
            for counter in 0..100 {
                log::info!(">> Event: count={counter}");
                let msg = format!("event: data\r\ndata: {{ \"counter\": {counter} }}\r\n\r\n");
                response.write(msg.as_bytes())?;
                response.flush()?;
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Ok::<(), anyhow::Error>(())
        }
    }
}

mod home_page {
    use esp_idf_svc::http::server::{EspHttpConnection, Request};

    use doorbell::web::NavBar;
    use doorbell::wifi::WifiState;

    use askama::Template;

    #[derive(askama::Template)]
    #[template(path = "index.html")]
    struct HomePage {
        title: &'static str,
        status: Vec<(String, String)>,
        navbar: NavBar<'static>,
    }

    pub fn make_handler(
        wifi_state: &WifiState,
        navbar: NavBar<'static>,
    ) -> impl for<'r> Fn(Request<&mut EspHttpConnection<'r>>) -> anyhow::Result<()> + Send + 'static
    {
        let status = wifi_state.display_fields();

        move |request| {
            let home_page = HomePage {
                title: "MQTT Alarm",
                status: status.clone(),
                navbar: navbar.clone(),
            };
            let mut response = request.into_response(200, Some("OK"), &[])?;
            let html = home_page.render()?;
            response.write(html.as_bytes())?;
            Ok::<(), anyhow::Error>(())
        }
    }
}
