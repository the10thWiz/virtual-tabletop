use std::sync::{
    atomic::{AtomicU32, AtomicUsize},
    RwLock,
};

use account::{DBConnInst, PUser, UserInfo};
use chrono::Utc;
use rand::Rng;
use rocket::{
    fs::{FileServer, Options},
    futures::TryStreamExt,
    get, message, post,
    request::FromParam,
    routes,
    serde::json::Json,
    websocket::Channel,
    Responder, State,
};
use rocket_auth::AuthFairing;
use rocket_db_pools::{sqlx::MySqlPool, Database};
use rocket_dyn_templates::Template;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use sqlx::mysql::MySqlDatabaseError;

mod account;

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

#[derive(Debug, Serialize)]
struct CreateCtx {
    #[serde(flatten)]
    tem: TemplateCtx,
    item_packs: Vec<ItemPack>,
}

#[derive(Debug, Serialize)]
struct ItemPack {
    id: String,
    name: String,
    desc: String,
    author: String,
    default: bool,
    img: Option<String>,
}

fn default_packs() -> Vec<ItemPack> {
    vec![
        ItemPack {
            id: "cards".into(),
            name: "Cards".into(),
            desc: "A 52 pack of cards, along with the jokers".into(),
            author: "Loading_M_".into(),
            default: true,
            img: None,
        },
        ItemPack {
            id: "icons".into(),
            name: "Icons".into(),
            desc: "A wide variety of icons".into(),
            author: "Loading_M_".into(),
            default: false,
            img: None,
        },
    ]
}

#[get("/create")]
fn create(user: PUser<'_>) -> Template {
    Template::render(
        "create",
        CreateCtx {
            tem: TemplateCtx {
                page: "create",
                error: None,
                user: user.map(|u| u.info().clone()),
                update_url: None,
            },
            item_packs: default_packs(),
        },
    )
}

#[get("/table/<id>")]
fn table(id: &str, user: PUser<'_>) -> Template {
    Template::render(
        "table",
        TemplateCtx {
            page: "table",
            error: None,
            user: user.map(|u| u.info().clone()),
            update_url: None,
        },
    )
}

#[derive(Debug, Serialize)]
struct TableState {
    created: chrono::DateTime<Utc>,
    name: String,
    #[serde(skip)]
    sharing: SharingType,
    #[serde(skip)]
    cur_id: AtomicU32,
    elements: flurry::HashMap<u32, ElementState>,
    icon_packs: flurry::HashSet<u32>,
}

impl Clone for TableState {
    fn clone(&self) -> Self {
        Self {
            created: self.created.clone(),
            name: self.name.clone(),
            sharing: self.sharing,
            cur_id: AtomicU32::new(self.cur_id.load(std::sync::atomic::Ordering::Acquire)),
            elements: self.elements.clone(),
            icon_packs: self.icon_packs.clone(),
        }
    }
}

impl TableState {
    fn new(name: String, sharing: SharingType, icon_packs: flurry::HashSet<u32>) -> Self {
        Self {
            created: chrono::Utc::now(),
            name,
            sharing,
            cur_id: AtomicU32::new(1),
            elements: flurry::HashMap::new(),
            icon_packs,
        }
    }

    fn tester() -> Self {
        let map = flurry::HashMap::new();
        {
            let guard = map.guard();
            map.insert(
                1,
                ElementState {
                    icon_pack: 1,
                    icon_id: 1,
                    element_id: 1,
                    top: AtomicUsize::new(0),
                    left: AtomicUsize::new(0),
                },
                &guard,
            );
        }
        let icon_packs = flurry::HashSet::new();
        {
            let guard = icon_packs.guard();
            icon_packs.insert(1, &guard);
        }
        Self {
            created: chrono::Utc::now(),
            name: "Default Tester".into(),
            sharing: SharingType::Public,
            cur_id: AtomicU32::new(2),
            elements: map,
            icon_packs,
        }
    }
}

#[derive(Debug, Serialize)]
struct ElementState {
    icon_pack: u32,
    icon_id: u32,
    #[serde(skip)]
    element_id: u32,
    top: AtomicUsize,
    left: AtomicUsize,
}

impl Clone for ElementState {
    fn clone(&self) -> Self {
        Self {
            icon_pack: self.icon_pack,
            icon_id: self.icon_id,
            element_id: self.element_id,
            top: AtomicUsize::new(self.top.load(std::sync::atomic::Ordering::Acquire)),
            left: AtomicUsize::new(self.left.load(std::sync::atomic::Ordering::Acquire)),
        }
    }
}

impl std::cmp::PartialOrd for ElementState {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.element_id.partial_cmp(&other.element_id)
    }
}
impl std::cmp::Ord for ElementState {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.element_id.cmp(&other.element_id)
    }
}
impl std::cmp::PartialEq for ElementState {
    fn eq(&self, other: &Self) -> bool {
        self.element_id.eq(&other.element_id)
    }
}
impl std::cmp::Eq for ElementState {}

struct GlobalState {
    map: flurry::HashMap<String, TableState>,
}

impl GlobalState {
    fn new() -> Self {
        let map = flurry::HashMap::new();
        {
            let guard = map.guard();
            map.insert("abc".into(), TableState::tester(), &guard);
        }
        Self { map }
    }
}

#[get("/api/table/<id>/state")]
fn table_state(id: &str, state: &State<GlobalState>) -> APIResponse<TableState> {
    let state = {
        let guard = state.map.guard();
        match state.map.get(id, &guard) {
            Some(t) => t.clone(),
            None => {
                return APIResponse::not_found(Error {
                    text: format!("`{}` not found", id),
                })
            }
        }
    };
    APIResponse::ok(state)
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(into = "&str", try_from = "&str")]
enum SharingType {
    Public,
    Password,
    Whitelist,
}

impl Into<&'static str> for SharingType {
    fn into(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Password => "password",
            Self::Whitelist => "whitelist",
        }
    }
}

impl TryFrom<&str> for SharingType {
    type Error = &'static str;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "public" => Ok(Self::Public),
            "password" => Ok(Self::Password),
            "whitelist" => Ok(Self::Whitelist),
            _ => Err("Invalid variant"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct TableOptions<'a> {
    name: &'a str,
    sharing: SharingType,
    icons: Vec<&'a str>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TableName {
    id: String,
}

#[post("/api/table/create", data = "<options>")]
async fn create_table(
    options: Json<TableOptions<'_>>,
    state: &State<GlobalState>,
    mut db: DBConnInst,
) -> APIResponse<TableName> {
    let options = options.into_inner();
    let name = options.name.to_string();
    let ids = match lookup_ids(options.icons, &mut db).await {
        Ok(ids) => ids,
        Err(sqlx::Error::RowNotFound) => {
            return APIResponse::not_found(Error {
                text: format!("Icon pack not found"),
            })
        }
        Err(_e) => {
            return APIResponse::internal_error(Error {
                text: format!("Internal Error"),
            });
        }
    };
    let table = TableState::new(name, options.sharing, ids);
    let guard = state.map.guard();
    let mut rng = rand::thread_rng();
    let id = format!("{:X}", rng.gen::<u16>());
    match state.map.try_insert(id.clone(), table, &guard) {
        Ok(_) => return APIResponse::ok(TableName { id }),
        Err(_) => {
            return APIResponse::internal_error(Error {
                text: format!("Internal Error"),
            });
        }
    }
}

async fn lookup_ids(
    names: Vec<&str>,
    db: &mut DBConnInst,
) -> Result<flurry::HashSet<u32>, sqlx::Error> {
    let ret = flurry::HashSet::new();
    for name in names {
        let row = sqlx::query!("SELECT id FROM icon_packs WHERE name = ?", name)
            .fetch_one(db.con())
            .await?;
        {
            let guard = ret.guard();
            ret.insert(row.id, &guard);
        }
    }
    Ok(ret)
}

#[derive(Debug, Serialize, Deserialize)]
struct IconPack {
    name: String,
    icons: Vec<Icon>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
enum Icon {
    Image {
        id: u32,
        name: String,
        src: String,
    },
    Icon {
        id: u32,
        name: String,
        class: String,
    },
    Svg {
        id: u32,
        name: String,
        src: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize_repr, Deserialize_repr, enum_utils::TryFromRepr)]
#[repr(i32)]
pub enum IconType {
    Image = 1,
    Icon = 2,
    Svg = 3,
}

#[get("/api/icons/<id>")]
async fn get_icon_pack(id: u32, mut db: DBConnInst) -> APIResponse<IconPack> {
    let name = match sqlx::query!("SELECT name FROM icon_packs WHERE id = ?", id)
        .fetch_one(db.con())
        .await
    {
        Ok(row) => row.name,
        Err(sqlx::Error::RowNotFound) => {
            return APIResponse::not_found(Error {
                text: format!("Icon pack not found"),
            })
        }
        Err(_e) => {
            return APIResponse::internal_error(Error {
                text: format!("Icon pack not found"),
            })
        }
    };
    let mut icons = vec![];
    let mut stream = sqlx::query!(
        "SELECT ty, icon_id, name, img FROM icons WHERE table_id = ?",
        id
    )
    .fetch(db.con());
    use rocket::futures::StreamExt;
    loop {
        match stream.next().await {
            None => break,
            Some(Ok(row)) => icons.push(match IconType::try_from(row.ty) {
                Ok(IconType::Image) => Icon::Image {
                    id: row.icon_id,
                    name: row.name,
                    src: row.img,
                },
                Ok(IconType::Icon) => Icon::Icon {
                    id: row.icon_id,
                    name: row.name,
                    class: row.img,
                },
                Ok(IconType::Svg) => Icon::Svg {
                    id: row.icon_id,
                    name: row.name,
                    src: row.img,
                },
                Err(e) => todo!(),
            }),
            Some(Err(_e)) => {
                return APIResponse::internal_error(Error {
                    text: format!("Internal Error"),
                })
            }
        }
    }
    APIResponse::ok(IconPack { name, icons })
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
enum TableUpdate {
    ElementDelete {
        id: u32,
    },
    Position {
        id: u32,
        top: usize,
        left: usize,
    },
    IconpackLoad {
        pack: u32,
    },
    ElementCreate {
        icon_pack: u32,
        icon_id: u32,
        top: usize,
        left: usize,
    },
}

#[message("/ws/table/<id>", data = "<update>")]
async fn handle_message(
    id: &str,
    state: &State<GlobalState>,
    update: Json<TableUpdate>,
    ws: &Channel<'_>,
) {
    println!("Update: {:?}", &*update);
    {
        let guard = state.map.guard();
        if let Some(t) = state.map.get(id, &guard) {
            match &*update {
                TableUpdate::Position { id, top, left } => {
                    let el_guard = t.elements.guard();
                    if let Some(el) = t.elements.get(id, &el_guard) {
                        el.top.store(*top, std::sync::atomic::Ordering::Release);
                        el.left.store(*left, std::sync::atomic::Ordering::Release);
                    } else {
                        return;
                    }
                }
                TableUpdate::IconpackLoad { pack } => {
                    let packs_guard = t.icon_packs.guard();
                    if !t.icon_packs.insert(*pack, &packs_guard) {
                        // Icon pack was already loaded
                        return;
                    }
                }
                TableUpdate::ElementCreate {
                    icon_pack,
                    icon_id,
                    top,
                    left,
                } => {
                    {
                        let icon_guard = t.icon_packs.guard();
                        if !t.icon_packs.contains(icon_pack, &icon_guard) {
                            // Icon pack not loaded
                            return;
                        }
                    }
                    let id = t.cur_id.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                    let el_guard = t.elements.guard();
                    t.elements.insert(
                        id,
                        ElementState {
                            icon_pack: *icon_pack,
                            icon_id: *icon_id,
                            element_id: id,
                            top: AtomicUsize::new(*top),
                            left: AtomicUsize::new(*left),
                        },
                        &el_guard,
                    );
                }
                TableUpdate::ElementDelete { id } => {
                    let el_guard = t.elements.guard();
                    if t.elements.remove(id, &el_guard).is_none() {
                        // Element did not exist
                        return;
                    }
                } //_ => todo!(),
            }
        } else {
            return;
        }
    }
    ws.broadcast(update).await;
}

#[rocket::launch]
fn launch() -> _ {
    let auth = AuthFairing::<DBConnInst>::fairing();
    let google_button = auth.google_button();
    rocket::build()
        .attach(DBConn::init())
        .manage(GlobalState::new())
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
                table,
                table_state,
                handle_message,
                create,
                create_table,
                get_icon_pack
            ],
        )
}
