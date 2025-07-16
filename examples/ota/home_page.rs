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
) -> impl for<'r> Fn(Request<&mut EspHttpConnection<'r>>) -> anyhow::Result<()> + Send + 'static {
    let status = wifi_state.display_fields();

    move |request| {
        let home_page = HomePage {
            title: "OTA Test",
            status: status.clone(),
            navbar: navbar.clone(),
        };
        let mut response = request.into_response(200, Some("OK"), &[])?;
        let html = home_page.render()?;
        response.write(html.as_bytes())?;
        Ok::<(), anyhow::Error>(())
    }
}
