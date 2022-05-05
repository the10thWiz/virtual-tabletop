use std::{
    future::Future,
    sync::{
        atomic::{AtomicU32, AtomicUsize},
        Arc,
    },
    time::Duration,
};

use chrono::Utc;
use rand::Rng;
use rocket::{
    fairing::{Fairing, Info, Kind},
    form::{Form, FromForm},
    get,
    http::Status,
    message, post,
    response::Redirect,
    routes,
    serde::json::Json,
    websocket::Channel,
    Build, Rocket, State,
};
use rocket_auth::UserId;
use rocket_dyn_templates::Template;
use rocket::serde::{Deserialize, Serialize};
//use serde_repr::{Deserialize_repr, Serialize_repr};

use crate::{
    account::{DBConnInst, PUser},
    APIResponse, Error, TemplateCtx,
};

pub struct Routes;

#[rocket::async_trait]
impl Fairing for Routes {
    fn info(&self) -> Info {
        Info {
            name: "Account Routes",
            kind: Kind::Ignite,
        }
    }

    async fn on_ignite(&self, rocket: Rocket<Build>) -> rocket::fairing::Result {
        Ok(rocket.mount(
            "/",
            routes![
                table,
                table_state,
                handle_message,
                create,
                create_table,
                get_icon_pack,
                find_table,
            ],
        ))
    }
}

#[derive(Debug, Serialize)]
#[serde(crate = "rocket::serde")]
struct CreateCtx {
    #[serde(flatten)]
    tem: TemplateCtx,
    item_packs: Vec<ItemPack>,
}

#[derive(Debug, Serialize)]
#[serde(crate = "rocket::serde")]
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
fn table(id: &str, user: PUser<'_>, state: &State<GlobalState>) -> Result<Template, Status> {
    {
        let guard = state.map.guard();
        if state.map.get(id, &guard).is_none() {
            return Err(Status::NotFound);
        }
    }
    Ok(Template::render(
        "table",
        TemplateCtx {
            page: "table",
            error: None,
            user: user.map(|u| u.info().clone()),
            update_url: None,
        },
    ))
}

#[derive(Debug, FromForm)]
struct FindTable<'a> {
    code: &'a str,
}

#[post("/find", data = "<data>")]
fn find_table(
    data: Form<FindTable<'_>>,
    state: &State<GlobalState>,
    user: PUser<'_>,
) -> Result<Redirect, (Status, Template)> {
    {
        let guard = state.map.guard();
        if state.map.get(data.code, &guard).is_none() {
            return Err((
                Status::BadRequest,
                Template::render(
                    "find",
                    TemplateCtx {
                        page: "find",
                        error: Some("Table not found"),
                        user: user.map(|u| u.info().clone()),
                        update_url: None,
                    },
                ),
            ));
        }
    }
    Ok(Redirect::to(rocket::uri!(table(id = data.code))))
}

#[derive(Debug, Serialize)]
#[serde(crate = "rocket::serde")]
struct TableState {
    created: chrono::DateTime<Utc>,
    name: String,
    #[serde(skip)]
    sharing: SharingType,
    #[serde(skip)] // TODO: host
    host: Option<UserId<'static>>,
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
            sharing: self.sharing.clone(),
            host: self.host.as_ref().map(|u| u.clone()),
            cur_id: AtomicU32::new(self.cur_id.load(std::sync::atomic::Ordering::Acquire)),
            elements: self.elements.clone(),
            icon_packs: self.icon_packs.clone(),
        }
    }
}

impl TableState {
    fn new(
        name: String,
        sharing: SharingType,
        host: Option<UserId<'static>>,
        icon_packs: flurry::HashSet<u32>,
    ) -> Self {
        Self {
            created: chrono::Utc::now(),
            name,
            sharing,
            host,
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
                    public_state: flurry::HashMap::new(),
                    private_state: flurry::HashMap::new(),
                    action: flurry::HashMap::new(),
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
            sharing: SharingType::Public {},
            host: None,
            cur_id: AtomicU32::new(2),
            elements: map,
            icon_packs,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(crate = "rocket::serde")]
struct ElementState {
    icon_pack: u32,
    icon_id: u32,
    #[serde(skip)]
    element_id: u32,
    top: AtomicUsize,
    left: AtomicUsize,
    // TODO: Add state that allows card flipping
    // We also want an action option, for e.g. a deck
    //
    // Private State? e.g. the other side of the card? (What about public state?)
    // - For decks a list of cards to randomly choose from
    // - For dice, a list of sides to randomly choose from ( Ideally this could be a special case
    // of a deck, e.g. it would automatically reset or something?)
    public_state: flurry::HashMap<String, Property>,
    #[serde(skip)]
    private_state: flurry::HashMap<String, Property>,
    #[serde(skip)]
    action: flurry::HashMap<String, Action>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(crate = "rocket::serde")]
enum Property {
    Single(ItemState),
    List(flurry::HashSet<ItemState>),
    Obj(flurry::HashMap<String, ItemState>),
}

#[derive(Debug, Serialize, Hash, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[serde(crate = "rocket::serde")]
enum ItemState {
    Icon { icon_pack: u32, icon_id: u32 },
    Num(usize),
    Str(String),
}

#[derive(Debug, Serialize, Hash, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[serde(crate = "rocket::serde")]
enum Action {
    Draw(String),
    Select(String),
}

impl Clone for ElementState {
    fn clone(&self) -> Self {
        Self {
            icon_pack: self.icon_pack,
            icon_id: self.icon_id,
            element_id: self.element_id,
            top: AtomicUsize::new(self.top.load(std::sync::atomic::Ordering::Acquire)),
            left: AtomicUsize::new(self.left.load(std::sync::atomic::Ordering::Acquire)),
            public_state: self.public_state.clone(),
            private_state: self.private_state.clone(),
            action: self.action.clone(),
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

pub struct GlobalState {
    map: Arc<flurry::HashMap<String, TableState>>,
}

impl GlobalState {
    pub fn new() -> (Self, impl Future<Output = ()>) {
        let map = Arc::new(flurry::HashMap::new());
        {
            let guard = map.guard();
            map.insert("abc".into(), TableState::tester(), &guard);
        }
        let cleanup_handle = Arc::clone(&map);
        (Self { map }, async move {
            loop {
                rocket::tokio::time::sleep(Duration::from_secs(4 * 60 * 60)).await;
                // TODO: cleanup old tables
                let guard = cleanup_handle.guard();
                cleanup_handle.retain(
                    |_code, _table| {
                        // true keeps element, so we need to decide when it's okay to remove a
                        // table
                        true
                    },
                    &guard,
                );
            }
        })
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(crate = "rocket::serde")]
#[serde(rename_all = "snake_case", untagged)]
enum SharingType {
    Public {},
    Password { password: String },
    Whitelist {},
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
struct TableOptions<'a> {
    name: &'a str,
    sharing: SharingType,
    icons: Vec<&'a str>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
struct TableName {
    id: String,
}

#[post("/api/table/create", data = "<options>")]
async fn create_table(
    options: Json<TableOptions<'_>>,
    state: &State<GlobalState>,
    user: PUser<'_>,
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
    let table = TableState::new(
        name,
        options.sharing,
        user.as_ref().map(|u| u.id().to_owned()),
        ids,
    );
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
#[serde(crate = "rocket::serde")]
struct IconPack {
    name: String,
    icons: Vec<Icon>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
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

#[derive(Debug, Clone, Copy, enum_utils::TryFromRepr, Serialize, Deserialize)]
#[repr(i32)]
#[serde(crate = "rocket::serde", into = "i32", try_from = "i32")]
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
                Err(_e) => todo!(),
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
#[serde(crate = "rocket::serde")]
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
        id: u32,
        top: usize,
        left: usize,
    },
    Action {
        act: String,
    },
}

#[message("/ws/table/<id>", data = "<update>")]
async fn handle_message(
    id: &str,
    state: &State<GlobalState>,
    mut update: Json<TableUpdate>,
    ws: &Channel<'_>,
) {
    println!("Update: {:?}", &*update);
    {
        let guard = state.map.guard();
        if let Some(t) = state.map.get(id, &guard) {
            match &mut *update {
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
                    id,
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
                    *id = t.cur_id.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                    let el_guard = t.elements.guard();
                    t.elements.insert(
                        *id,
                        ElementState {
                            icon_pack: *icon_pack,
                            icon_id: *icon_id,
                            element_id: *id,
                            top: AtomicUsize::new(*top),
                            left: AtomicUsize::new(*left),
                            public_state: flurry::HashMap::new(),
                            private_state: flurry::HashMap::new(),
                            action: flurry::HashMap::new(),
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
                TableUpdate::Action { act } => {
                    println!("TODO: action {}", act);
                    return;
                }
            }
        } else {
            return;
        }
    }
    ws.broadcast(update).await;
}
