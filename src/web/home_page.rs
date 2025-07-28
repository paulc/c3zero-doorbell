use std::sync::Mutex;

use askama::Template;
use esp_idf_svc::http::server::{EspHttpConnection, Request};

use crate::web::NavBar;

#[derive(Clone)]
pub struct BuildInfo {
    pub build_ts: &'static str,
    pub build_branch: &'static str,
    pub build_hash: &'static str,
    pub build_profile: &'static str,
}

impl BuildInfo {
    pub fn display_fields(&self) -> Vec<(String, String)> {
        vec![
            ("Build Timestamp".to_owned(), self.build_ts.to_owned()),
            ("Build Branch".to_owned(), self.build_branch.to_owned()),
            ("Build Hash".to_owned(), self.build_hash.to_owned()),
            ("Build Profile".to_owned(), self.build_profile.to_owned()),
        ]
    }
}

pub fn get_partition_info() -> Vec<(String, String)> {
    let mut out = Vec::new();
    unsafe {
        let running_partition = esp_idf_sys::esp_ota_get_running_partition();
        let address = (*running_partition).address;
        let mut ota_state: esp_idf_sys::esp_ota_img_states_t = 0;
        esp_idf_sys::esp_ota_get_state_partition(running_partition, &mut ota_state);
        let state = match ota_state {
            esp_idf_sys::esp_ota_img_states_t_ESP_OTA_IMG_NEW => "ESP_OTA_IMG_NEW",
            esp_idf_sys::esp_ota_img_states_t_ESP_OTA_IMG_PENDING_VERIFY => {
                "ESP_OTA_IMG_PENDING_VERIFY"
            }
            esp_idf_sys::esp_ota_img_states_t_ESP_OTA_IMG_VALID => "ESP_OTA_IMG_VALID",
            esp_idf_sys::esp_ota_img_states_t_ESP_OTA_IMG_INVALID => "ESP_OTA_IMG_INVALID",
            esp_idf_sys::esp_ota_img_states_t_ESP_OTA_IMG_ABORTED => "ESP_OTA_IMG_ABORTED",
            esp_idf_sys::esp_ota_img_states_t_ESP_OTA_IMG_UNDEFINED => "ESP_OTA_IMG_UNDEFINED",
            _ => "ERROR: UNKNOWN STATE",
        };
        out.push(("Partition Address".to_string(), format!("0x{address:x}")));
        out.push(("Partition State".to_string(), state.to_string()));
    };
    out
}

pub static STATUS: Mutex<Option<Vec<(String, String)>>> = Mutex::new(None);

#[derive(Clone, askama::Template)]
#[template(path = "index.html")]
pub struct HomePage {
    title: &'static str,
    build_info: Vec<(String, String)>,
    navbar: NavBar<'static>,
}

impl HomePage {
    pub fn new(
        title: &'static str,
        build_info: Vec<(String, String)>,
        navbar: NavBar<'static>,
    ) -> Self {
        let mut build_info = build_info;
        build_info.extend(get_partition_info());
        Self {
            title,
            build_info,
            navbar,
        }
    }

    pub fn set_status(&self, status: Vec<(String, String)>) -> anyhow::Result<()> {
        STATUS.replace(Some(status))?;
        Ok(())
    }

    pub fn get_status(&self) -> Vec<(String, String)> {
        match STATUS.get_cloned() {
            Ok(Some(v)) => v,
            _ => Vec::new(),
        }
    }

    pub fn make_handler(
        &self,
    ) -> impl for<'r> Fn(Request<&mut EspHttpConnection<'r>>) -> anyhow::Result<()> + Send + 'static
    {
        let home_page = self.clone();
        move |request| {
            let mut response = request.into_response(200, Some("OK"), &[])?;
            let html = home_page.render()?;
            response.write(html.as_bytes())?;
            Ok::<(), anyhow::Error>(())
        }
    }
}
