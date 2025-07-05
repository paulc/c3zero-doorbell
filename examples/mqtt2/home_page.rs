use esp_idf_svc::http::server::{EspHttpConnection, Request};

use askama::Template;

use doorbell::wifi;

#[derive(askama::Template)]
#[template(path = "index.html")]
struct HomePage<'a> {
    title: &'a str,
    ip: &'a str,
}

pub fn handle_home(request: Request<&mut EspHttpConnection>) -> anyhow::Result<()> {
    let alarm_ip = if let Ok(Some(ip)) = wifi::IP_INFO.get_cloned() {
        &ip.ip.to_string()
    } else {
        "<Unknown IP>"
    };

    let home_page = HomePage {
        title: "MQTT Alarm",
        ip: alarm_ip,
    };
    let mut response = request.into_response(200, Some("OK"), &[])?;
    let html = home_page.render()?;
    response.write(html.as_bytes())?;
    Ok::<(), anyhow::Error>(())
}
