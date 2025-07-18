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
