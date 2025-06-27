use esp_idf_svc::http::server::{EspHttpConnection, Request};

use askama::Template;

#[derive(askama::Template)]
#[template(path = "reset_page.html")]
struct ResetPage {}

const REBOOT_DELAY_MS: u32 = 1000;

pub fn handle_reset(request: Request<&mut EspHttpConnection>) -> anyhow::Result<()> {
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

pub fn handle_reset_page(request: Request<&mut EspHttpConnection>) -> anyhow::Result<()> {
    let reset_page = ResetPage {};
    let mut response = request.into_ok_response()?;
    let html = reset_page.render()?;
    response.write(html.as_bytes())?;
    Ok::<(), anyhow::Error>(())
}
