use rocket::{Rocket, get, routes, fs::{FileServer, Options}};
use rocket_dyn_templates::Template;

#[derive(Debug, serde::Serialize)]
pub struct TemplateCtx {
    page: &'static str,
    error: Option<&'static str>,
    user: Option<()>,
    update_url: Option<&'static str>,
}

#[get("/")]
fn index() -> Template {
    Template::render("index", TemplateCtx {
        page: "index",
        error: None,
        user: None,
        update_url: None,
    })
}

#[rocket::launch]
fn launch() -> _ {
    Rocket::build()
        .attach(Template::fairing())
        .mount("/", FileServer::new("static", Options::default()))
        .mount("/", routes![index])
}
