use esp_idf_svc::http;
use esp_idf_svc::http::server::{Configuration as HttpConfig, EspHttpServer};

mod flash_msg;
pub use flash_msg::FlashMsg;

mod hello;
mod nvs;
mod reset;
mod style;
mod wifi;

pub fn start_http_server<'a>() -> anyhow::Result<EspHttpServer<'a>> {
    log::info!("Starting HTTPD:");
    let config: HttpConfig = HttpConfig {
        uri_match_wildcard: true,
        ..Default::default()
    };
    let mut server = EspHttpServer::new(&config)?;

    server.fn_handler("/hello", http::Method::Get, hello::handle_hello)?;

    server.fn_handler("/style.css", http::Method::Get, style::handle_style)?;

    server.fn_handler("/reset", http::Method::Get, reset::handle_reset)?;
    server.fn_handler("/reset_page", http::Method::Get, reset::handle_reset_page)?;

    server.fn_handler("/wifi", http::Method::Get, wifi::handle_wifi)?;
    server.fn_handler("/wifi/delete/*", http::Method::Get, wifi::handle_ap_delete)?;
    server.fn_handler("/wifi/add", http::Method::Post, wifi::handle_ap_add)?;

    server.fn_handler("/nvs/get/*", http::Method::Get, nvs::handle_nvs_get)?;
    server.fn_handler("/nvs/set/*", http::Method::Post, nvs::handle_nvs_set)?;
    server.fn_handler("/nvs/delete/*", http::Method::Get, nvs::handle_nvs_delete)?;

    log::info!("Web server started");

    Ok(server)
}
