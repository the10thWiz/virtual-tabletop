use std::{
    sync::{
        atomic::{AtomicU32, AtomicUsize},
        Arc, RwLock,
    },
    time::Duration, future::Future,
};

use account::{DBConnInst, PUser, UserInfo};
use chrono::Utc;
use rand::Rng;
use rocket::{
    form::Form,
    fs::{FileServer, Options},
    futures::TryStreamExt,
    get,
    http::{uri, Status},
    message, post,
    request::FromParam,
    response::Redirect,
    routes,
    serde::json::Json,
    websocket::Channel,
    FromForm, Responder, Shutdown, State,
};
use rocket_auth::AuthFairing;
use rocket_db_pools::{sqlx::MySqlPool, Database};
use rocket_dyn_templates::Template;
use serde::{Deserialize, Serialize};
use table::GlobalState;

mod account;
mod table;

#[derive(Database)]
#[database("tabletop")]
pub struct DBConn(MySqlPool);

#[derive(Debug, Serialize, Deserialize)]
pub struct Error {
    text: String,
}

#[derive(Debug, Responder)]
#[response(bound = "T: Serialize")]
pub enum APIResponse<T> {
    #[response(status = 200, content_type = "json")]
    Ok(Json<T>),
    #[response(status = 404, content_type = "json")]
    NotFound(Json<Error>),
    #[response(status = 500, content_type = "json")]
    InternalError(Json<Error>),
}

impl<T: Default> Default for APIResponse<T> {
    fn default() -> Self {
        Self::Ok(Json(T::default()))
    }
}

impl<T> APIResponse<T> {
    pub fn ok(inner: T) -> Self {
        Self::Ok(Json(inner))
    }

    pub fn not_found(inner: Error) -> Self {
        Self::NotFound(Json(inner))
    }

    pub fn internal_error(inner: Error) -> Self {
        Self::InternalError(Json(inner))
    }
}

impl APIResponse<Empty> {
    pub fn empty() -> Self {
        Self::default()
    }
}

#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize, Deserialize)]
pub struct Empty {}

#[derive(Debug, serde::Serialize)]
pub struct TemplateCtx {
    page: &'static str,
    error: Option<&'static str>,
    user: Option<UserInfo>,
    update_url: Option<&'static str>,
}

#[get("/")]
fn index() -> Template {
    Template::render(
        "index",
        TemplateCtx {
            page: "index",
            error: None,
            user: None,
            update_url: None,
        },
    )
}

macro_rules! string_enum {
    ($name:ident { $($temp:ident => $file:literal),* $(,)?}) => {
        enum $name {
            $($temp,)*
        }

        impl<'a> FromParam<'a> for $name {
            type Error = ();
            fn from_param(param: &'a str) -> Result<Self, Self::Error> {
                match param {
                    $($file => Ok(Self::$temp),)*
                    _ => Err(()),
                }
            }
        }

        impl $name {
            pub fn file_name(&self) -> &'static str {
                match self {
                    $(Self::$temp => $file,)*
                }
            }
        }
    };
}

string_enum!(Page {
    //Create => "create",
    Find => "find",
});

#[get("/<page>")]
fn pages(page: Page, user: PUser<'_>) -> Template {
    Template::render(
        page.file_name(),
        TemplateCtx {
            page: page.file_name(),
            error: None,
            user: user.map(|u| u.info().clone()),
            update_url: None,
        },
    )
}

#[rocket::main]
async fn main() -> Result<(), rocket::Error> {
    let auth = AuthFairing::<DBConnInst>::fairing();
    let google_button = auth.google_button();
    let (state, task) = GlobalState::new();
    let r = rocket::build()
        .attach(DBConn::init())
        .manage(state)
        .attach(Template::custom(move |engines| {
            engines
                .tera
                .register_function("google_button", google_button.clone());
        }))
        .attach(auth)
        .attach(account::Routes)
        .mount("/", FileServer::new("static", Options::default()))
        .mount(
            "/",
            routes![
                index,
                pages,
            ],
        )
        .ignite().await?;
    let (res, _) = rocket::tokio::join![r.launch(), task];
    res
}
