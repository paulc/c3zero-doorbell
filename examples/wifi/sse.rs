use esp_idf_svc::http::server::{EspHttpConnection, Request};

use askama::Template;

use doorbell::web::NavBar;

#[derive(Template)]
#[template(path = "sse_test.html")]
struct SSEPage {
    navbar: NavBar<'static>,
}

pub fn make_sse_page(
    navbar: NavBar<'static>,
) -> impl for<'r> Fn(Request<&mut EspHttpConnection<'r>>) -> anyhow::Result<()> + Send + 'static {
    move |request| {
        let sse_page = SSEPage {
            navbar: navbar.clone(),
        };
        let mut response = request.into_response(200, Some("OK"), &[])?;
        let html = sse_page.render()?;
        response.write(html.as_bytes())?;

        Ok::<(), anyhow::Error>(())
    }
}

pub fn make_sse_handler(
) -> impl for<'r> Fn(Request<&mut EspHttpConnection<'r>>) -> anyhow::Result<()> + Send + 'static {
    move |request| {
        let mut response = request.into_response(
            200,
            Some("OK"),
            &[
                ("Content-Type", "text/event-stream"),
                ("Connection", "keep-alive"),
                ("Access-Control-Allow-Origin", "*"),
            ],
        )?;
        for counter in 0..100 {
            log::info!(">> Event: count={counter}");
            let msg = format!("event: data\r\ndata: {{ \"counter\": {counter} }}\r\n\r\n");
            response.write(msg.as_bytes())?;
            response.flush()?;
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        Ok::<(), anyhow::Error>(())
    }
}
