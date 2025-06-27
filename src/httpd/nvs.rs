use esp_idf_svc::http::server::{EspHttpConnection, Request};

use crate::nvs::NVStore;

pub fn handle_nvs_get(request: Request<&mut EspHttpConnection>) -> anyhow::Result<()> {
    let key = request.uri().split('/').next_back().expect("Invalid Key");
    let key = urlencoding::decode(key)?;
    log::info!("NVS_GET: {key:?}");
    match NVStore::get_raw(&key) {
        Ok(Some(v)) => {
            let mut response =
                request.into_response(200, Some("OK"), &[("Content-Type", "application/json")]);
            if let Ok(ref mut r) = response {
                r.write(&v)?;
                r.write(b"\r\n")?;
            }
            response
        }
        Ok(None) => request.into_response(
            404,
            Some("Key not found"),
            &[("Content-Type", "text/plain")],
        ),
        Err(e) => request.into_response(500, Some(&e.to_string()), &[]),
    }
    .map(|_| ())
    .map_err(|e| anyhow::anyhow!("Http Error: {e}"))
}

pub fn handle_nvs_delete(request: Request<&mut EspHttpConnection>) -> anyhow::Result<()> {
    let key = request.uri().split('/').next_back().expect("Invalid Key");
    let key = urlencoding::decode(key)?;
    log::info!("NVS_DELETE: {key:?}");
    match NVStore::delete(&key) {
        Ok(_) => request.into_response(200, Some("OK"), &[("Content-Type", "application/json")]),
        Err(e) => request.into_response(500, Some(&e.to_string()), &[]),
    }
    .map(|_| ())
    .map_err(|e| anyhow::anyhow!("Http Error: {e}"))
}

pub fn handle_nvs_set(mut request: Request<&mut EspHttpConnection>) -> anyhow::Result<()> {
    // Read the body of the request
    let mut buf = [0_u8; 1024];
    let len = request.read(&mut buf)?;

    let key = request.uri().split('/').next_back().expect("Invalid Key");
    let key = urlencoding::decode(key)?;
    log::info!("NVS_SET: {key}: {}", String::from_utf8_lossy(&buf));

    match request.header("Content-Type") {
        Some("application/json") => match NVStore::set_raw(&key, &buf[0..len]) {
            Ok(_) => request.into_ok_response(),
            Err(e) => {
                log::error!("NVS_SET: {e}");
                request.into_response(400, Some(&e.to_string()), &[])
            }
        },
        _ => request.into_response(400, Some("Invalid Content-Type"), &[]),
    }
    .map(|_| ())
    .map_err(|e| anyhow::anyhow!("Http Error: {e}"))
}
