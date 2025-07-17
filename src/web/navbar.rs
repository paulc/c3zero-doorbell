#[derive(Clone)]
pub struct NavLink {
    pub url: &'static str,
    pub label: &'static str,
}

#[derive(Clone)]
pub struct NavBar<'a> {
    pub title: &'a str,
    pub links: &'a [NavLink],
}
