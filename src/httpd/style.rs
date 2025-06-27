use esp_idf_svc::http::server::{EspHttpConnection, Request};

pub fn handle_style(request: Request<&mut EspHttpConnection>) -> anyhow::Result<()> {
    let mut response = request.into_response(
        200,
        Some("OK"),
        &[
            ("Content-Type", "text/css"),
            ("Cache-Control", "max-age=600"),
        ],
    )?;
    let css = std::include_bytes!("../../templates/style.css");
    response.write(css)?;
    Ok(())
}
