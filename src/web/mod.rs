use esp_idf_svc::http::server::{
    Configuration as HttpConfig, EspHttpConnection, EspHttpServer, Request,
};
use esp_idf_svc::http::Method;

mod flash_msg;
mod hello;
mod reset;
mod style;

// Export FlashMsg
pub use flash_msg::FlashMsg;

pub struct WebServer<'a> {
    server: EspHttpServer<'a>,
}

impl<'a> WebServer<'a> {
    pub fn new() -> anyhow::Result<Self> {
        log::info!("Starting HTTPD:");
        let config: HttpConfig = HttpConfig {
            uri_match_wildcard: true,
            ..Default::default()
        };
        let mut server = EspHttpServer::new(&config)?;

        // Add default handlers
        server.fn_handler("/hello", Method::Get, hello::handle_hello)?;
        server.fn_handler("/reset", Method::Get, reset::handle_reset)?;
        server.fn_handler("/reset_page", Method::Get, reset::handle_reset_page)?;
        server.fn_handler("/style.css", Method::Get, style::handle_style)?;

        Ok(Self { server })
    }

    pub fn add_handler<F>(&mut self, uri: &str, method: Method, f: F) -> anyhow::Result<()>
    where
        F: for<'r> Fn(Request<&mut EspHttpConnection<'r>>) -> anyhow::Result<()> + Send + 'static,
    {
        self.server.fn_handler(uri, method, f)?;
        Ok(())
    }
}
