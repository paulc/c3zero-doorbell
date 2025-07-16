use askama::Template;
use esp_idf_svc::http::server::{EspHttpConnection, Request};
use esp_idf_svc::http::Method;

use doorbell::web::{FlashMsg, NavBar, WebServer};

#[derive(askama::Template)]
#[template(path = "ota_page.html")]
struct OtaPage {
    navbar: NavBar<'static>,
}

pub fn make_ota_page(
    navbar: NavBar<'static>,
) -> impl for<'r> Fn(Request<&mut EspHttpConnection<'r>>) -> anyhow::Result<()> + Send + 'static {
    move |request| {
        let ota_page = OtaPage {
            navbar: navbar.clone(),
        };
        let mut response = request.into_ok_response()?;
        let html = ota_page.render()?;
        response.write(html.as_bytes())?;
        Ok::<(), anyhow::Error>(())
    }
}

pub fn add_handlers(server: &mut WebServer, navbar: NavBar<'static>) -> anyhow::Result<()> {
    server.add_handler("/ota_page", Method::Get, make_ota_page(navbar))?;
    server.add_handler("/ota", Method::Post, ota_handler)?;
    server.add_handler("/ota_rollback", Method::Get, rollback_handler)?;
    server.add_handler("/ota_valid", Method::Get, valid_handler)?;
    Ok(())
}

pub fn ota_handler(mut request: Request<&mut EspHttpConnection>) -> anyhow::Result<()> {
    let mut buf = [0_u8; 1024];

    log::info!("Starting OTA Update");

    let mut total = 0_usize;
    let mut ota = esp_ota::OtaUpdate::begin()?;

    while let Ok(len) = request.read(&mut buf) {
        if len == 0 {
            break;
        }
        total += len;
        log::info!("Writing OTA Chunk: {len}/{total}");
        ota.write(&buf[0..len])?;
    }
    log::info!("OTA Image: {total} bytes");

    log::info!("Finalising OTA Update");

    match ota.finalize() {
        Ok(mut completed_ota) => match completed_ota.set_as_boot_partition() {
            Ok(_) => {
                let _ = std::thread::spawn(move || {
                    log::info!("OTA Restarting");
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    esp_idf_hal::reset::restart();
                });
                request.into_response(200, Some("OTA Update OK"), &[])
            }
            Err(e) => request.into_response(400, Some(&format!("OTA Error: {e}")), &[]),
        },
        Err(e) => request.into_response(400, Some(&format!("OTA Error: {e}")), &[]),
    }?;

    Ok::<(), anyhow::Error>(())
}

pub fn valid_handler(request: Request<&mut EspHttpConnection>) -> anyhow::Result<()> {
    esp_ota::mark_app_valid();
    let flash = serde_json::to_string(&FlashMsg {
        level: "success",
        message: "Marked App Valid",
    })?;
    request.into_response(
        302,
        Some("Marked App Valid"),
        &[
            ("Location", "/ota_page"),
            ("Set-Cookie", &format!("flash_msg={flash}; path=/")),
        ],
    )?;
    Ok::<(), anyhow::Error>(())
}

pub fn rollback_handler(request: Request<&mut EspHttpConnection>) -> anyhow::Result<()> {
    match esp_ota::rollback_and_reboot() {
        Ok(_) => request.into_response(200, Some("Rollback Succeeded"), &[]), // Not reached
        Err(e) => {
            let m = format!("OTA Rollback Error: {e}");
            let flash = serde_json::to_string(&FlashMsg {
                level: "error",
                message: &m,
            })?;
            request.into_response(
                302,
                Some(&m),
                &[
                    ("Location", "/ota_page"),
                    ("Set-Cookie", &format!("flash_msg={flash}; path=/")),
                ],
            )
        }
    }?;

    Ok::<(), anyhow::Error>(())
}
