use esp_idf_svc::http::server::{EspHttpConnection, Request};

use crate::web::NavBar;

use askama::Template;

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

#[derive(Clone, askama::Template)]
#[template(path = "index.html")]
pub struct HomePage {
    title: &'static str,
    status: Vec<(String, String)>,
    navbar: NavBar<'static>,
}

impl HomePage {
    pub fn new(
        title: &'static str,
        status: Vec<(String, String)>,
        navbar: NavBar<'static>,
    ) -> Self {
        Self {
            title,
            status,
            navbar,
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
