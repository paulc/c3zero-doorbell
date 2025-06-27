use esp_idf_svc::http::server::{EspHttpConnection, Request};

pub fn handle_hello(request: Request<&mut EspHttpConnection>) -> anyhow::Result<()> {
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
