use embedded_svc::http::client::Client as HttpClient;
use esp_idf_svc::http::client::{Configuration as HttpConfiguration, EspHttpConnection};

/// Send an HTTP POST request.
pub fn send_pushover_alert() -> anyhow::Result<()> {
    use embedded_svc::io::Write;

    // HTTP Client
    let config = &HttpConfiguration {
        crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
        ..Default::default()
    };
    let mut client = HttpClient::wrap(EspHttpConnection::new(config)?);

    // Pushover API payload
    let app_token = "amfa9dzeck8bongtab3nrta3xux3hj";
    let user = "uomfetdtawqotwp3ii9jpf4buys3p4";
    let message = "DOORBELL";

    let payload =
        format!("{{\"token\":\"{app_token}\",\"user\":\"{user}\",\"message\":\"{message}\"}}");
    let payload = payload.as_bytes();

    // Prepare headers and URL
    let content_length_header = format!("{}", payload.len());
    let headers = [
        ("content-type", "application/json"),
        ("content-length", content_length_header.as_str()),
        ("accept", "application/json"),
    ];

    let url = "https://api.pushover.net/1/messages.json";

    let mut request = client.post(url, &headers)?;

    request.write_all(payload)?;
    request.flush()?;
    log::info!("-> POST {url}");

    let response = request.submit()?;
    log::info!("<- {}", response.status());

    Ok(())
}
