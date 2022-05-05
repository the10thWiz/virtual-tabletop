//
// account.rs
// Copyright (C) 2022 matthew <matthew@WINDOWS-05HIC4F>
// Distributed under terms of the MIT license.
//

use rocket::{
    fairing::{Fairing, Info, Kind},
    form::Form,
    get,
    http::Status,
    post,
    request::{FromRequest, Outcome},
    response::Redirect,
    routes,
    serde::json::Json,
    uri, Build, FromForm, Request, Rocket,
};
use rocket_auth::{
    AuthCtx, AuthHash, AuthUpdate, GoogleToken, Password, UserDb, UserId, UserIdentifier,
};
use rocket_db_pools::{sqlx, Connection};
use rocket_dyn_templates::Template;
use serde::{Deserialize, Serialize};

use crate::{APIResponse, DBConn, Empty, TemplateCtx};

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
                login_page,
                login,
                create_user,
                create_account_page,
                logout_page,
                account_redirect,
                update_account_page,
                update_account_email,
                update_account_password,
                get_account_redirect,
                post_account_redirect,
                google_login,
                admin_panel,
                admin_users,
                admin_update_user,
                get_admin_redirect,
                post_admin_redirect,
            ],
        ))
    }
}

pub use permissions::*;
mod permissions {
    use super::{Role::*, UserInfo};
    use rocket::request::FromParam;
    use rocket_auth::permission;
    use serde_repr::{Deserialize_repr, Serialize_repr};

    #[derive(Debug, Clone, Copy, Serialize_repr, Deserialize_repr, enum_utils::TryFromRepr)]
    #[repr(i32)]
    pub enum Role {
        Admin = 2,
        User = 3,
    }

    impl Role {
        //pub fn as_str(&self) -> &'static str {
            //match self {
                //Self::Admin => "Admin",
                //Self::User => "User",
            //}
        //}
    }

    impl<'a> FromParam<'a> for Role {
        type Error = ();
        fn from_param(param: &'a str) -> Result<Self, Self::Error> {
            match param {
                "admin" => Ok(Self::Admin),
                "owner" => Ok(Self::User),
                _ => Err(()),
            }
        }
    }

    permission!(pub ViewAdminPanel = |user: &UserInfo| matches!(user.role, Admin));
    permission!(pub ManageUsers = |user: &UserInfo| matches!(user.role, Admin));
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    username: String,
    email: String,
    role: Role,
}
pub type User<'r> = rocket_auth::User<'r, DBConnInst>;
pub type PUser<'r> = Option<rocket_auth::User<'r, DBConnInst>>;

pub struct DBConnInst(Connection<DBConn>);

impl DBConnInst {
    pub fn con<'s>(&'s mut self) -> impl sqlx::MySqlExecutor<'s> + 's {
        &mut *self.0
    }
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for DBConnInst {
    type Error = <Connection<DBConn> as FromRequest<'r>>::Error;
    async fn from_request(requst: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        requst.guard().await.map(Self)
    }
}

#[rocket::async_trait]
impl UserDb for DBConnInst {
    type UserInfo = UserInfo;
    type DbError = sqlx::Error;

    async fn get_user(
        &mut self,
        id: &UserIdentifier<'_>,
    ) -> Result<Option<(AuthHash, UserId<'static>, Self::UserInfo)>, Self::DbError> {
        match id {
            UserIdentifier::UserId(id) => {
                match sqlx::query!("SELECT * FROM users WHERE id = ?", id)
                    .fetch_optional(self.con())
                    .await?
                {
                    None => Ok(None),
                    Some(user) => Ok(Some((
                        AuthHash::from_bytes(&user.auth).expect("Corrupt Database"),
                        UserId(user.id.into()),
                        UserInfo {
                            username: user.username,
                            email: user.email,
                            role: user.role.try_into().expect("Corrupt Database"),
                        },
                    ))),
                }
            }
            UserIdentifier::Username(name) => {
                match sqlx::query!("SELECT * FROM users WHERE username = ?", name)
                    .fetch_optional(self.con())
                    .await?
                {
                    None => Ok(None),
                    Some(user) => Ok(Some((
                        AuthHash::from_bytes(&user.auth).expect("Corrupt Database"),
                        UserId(user.id.into()),
                        UserInfo {
                            username: user.username,
                            email: user.email,
                            role: user.role.try_into().expect("Corrupt Database"),
                        },
                    ))),
                }
            }
        }
    }

    async fn create_user(
        &mut self,
        id: UserId<'_>,
        auth: AuthHash,
        info: Self::UserInfo,
    ) -> Result<bool, Self::DbError> {
        Ok(sqlx::query!(
            "INSERT INTO users (id, auth, username, email, role) VALUES (?, ?, ?, ?, ?)",
            id,
            auth,
            info.username,
            info.email,
            info.role as i32
        )
        .execute(self.con())
        .await?.rows_affected() == 1)
    }

    async fn update_user(&mut self, id: UserId<'_>, auth: AuthHash) -> Result<bool, Self::DbError> {
        Ok(sqlx::query!(
            "UPDATE users SET auth = ? WHERE id = ?",
            auth,
            id,
        )
        .execute(self.con())
        .await?.rows_affected() == 1)
    }
}

#[derive(FromForm)]
struct LoginInfo<'a> {
    username: &'a str,
    password: Password<'a>,
}
impl std::fmt::Debug for LoginInfo<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "")}
}

#[post("/account/login", data = "<login_info>")]
async fn login(
    login_info: Form<LoginInfo<'_>>,
    mut auth_ctx: AuthCtx<'_, DBConnInst>,
) -> (Status, Template) {
    let login_info = login_info.into_inner();
    match auth_ctx
        .login(login_info.username, login_info.password)
        .await
    {
        Ok(Some(u)) => (
            Status::Ok,
            Template::render(
                "index",
                &TemplateCtx {
                    page: "index",
                    update_url: Some("/"),
                    error: None,
                    user: Some(UserInfo::clone(u.info())),
                },
            ),
        ),
        Ok(None) => (
            Status::Unauthorized,
            Template::render(
                "account/login",
                &TemplateCtx {
                    page: "account/login",
                    update_url: None,
                    error: Some("Username or password is incorrect"),
                    user: None,
                },
            ),
        ),
        Err(_e) => (
            Status::InternalServerError,
            Template::render(
                "account/login",
                &TemplateCtx {
                    page: "account/login",
                    update_url: None,
                    error: Some("Internal Server Error"),
                    user: None,
                },
            ),
        ),
    }
}

#[post("/api/login/google", data = "<token>")]
async fn google_login(
    token: GoogleToken,
    mut auth_ctx: AuthCtx<'_, DBConnInst>,
) -> (Status, Template) {
    println!("{:?}", token);
    // TODO: This needs to be easier. I suspect it will come down to a get_or_create method or
    // something like that. Potentially I can offer to make it easier, but I will likely need to
    // take more control over the database end.
    match auth_ctx
        .login_oauth(token, |token| UserInfo {
            username: token.name().to_owned(),
            email: token.email().to_owned(),
            role: Role::User,
        })
        .await
    {
        Ok(Some(u)) => (
            Status::Ok,
            Template::render(
                "index",
                &TemplateCtx {
                    page: "index",
                    update_url: Some("/"),
                    error: None,
                    user: Some(UserInfo::clone(u.info())),
                },
            ),
        ),
        Ok(None) => (
            Status::Unauthorized,
            Template::render(
                "account/login",
                &TemplateCtx {
                    page: "account/login",
                    update_url: None,
                    error: Some("Account not found"),
                    user: None,
                },
            ),
        ),
        Err(_e) => (
            Status::InternalServerError,
            Template::render(
                "account/login",
                &TemplateCtx {
                    page: "account/login",
                    update_url: None,
                    error: Some("Internal Server Error"),
                    user: None,
                },
            ),
        ),
    }
}

#[derive(FromForm)]
struct UserCreateInfo<'a> {
    username: UserId<'a>,
    password: Password<'a>,
    email: &'a str,
}

impl std::fmt::Debug for UserCreateInfo<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "")}
}

#[post("/account/create", data = "<login_info>")]
async fn create_user(
    login_info: Form<UserCreateInfo<'_>>,
    mut auth_ctx: AuthCtx<'_, DBConnInst>,
) -> (Status, Template) {
    let login_info = login_info.into_inner();
    let info = UserInfo {
        username: login_info.username.0.into_owned(),
        email: login_info.email.to_owned(),
        role: Role::User,
    };
    match auth_ctx.create_user(login_info.password, info).await {
        Ok(Some(u)) => (
            Status::Ok,
            Template::render(
                "index",
                &TemplateCtx {
                    page: "index",
                    update_url: Some("/"),
                    error: None,
                    user: Some(u.info().to_owned()),
                },
            ),
        ),
        Ok(None) => (
            Status::BadRequest,
            Template::render(
                "account/create",
                &TemplateCtx {
                    page: "account/create",
                    update_url: None,
                    error: Some("Username is already taken"),
                    user: None,
                },
            ),
        ),
        Err(_e) => {
            println!("Error: {:?}", _e);
            (
                Status::InternalServerError,
                Template::render(
                    "account/create",
                    &TemplateCtx {
                        page: "account/create",
                        update_url: None,
                        error: Some("Internal Server Error"),
                        user: None,
                    },
                ),
            )
        }
    }
}

//pub fn user_map(user: PUser<'_>) -> Option<UserInfo> {
    //user.map(|u| u.info().to_owned())
//}

#[get("/account/login")]
fn login_page(user: PUser<'_>) -> Result<Template, Redirect> {
    if user.is_some() {
        Err(Redirect::to(uri!("/")))
    } else {
        Ok(Template::render(
            "account/login",
            &TemplateCtx {
                page: "account/login",
                user: None,
                update_url: None,
                error: None,
            },
        ))
    }
}

//#[post()]

#[get("/account/create")]
fn create_account_page(user: PUser<'_>) -> Result<Template, Redirect> {
    if user.is_some() {
        Err(Redirect::to(uri!("/")))
    } else {
        Ok(Template::render(
            "account/create",
            &TemplateCtx {
                page: "account/login",
                user: None,
                update_url: None,
                error: None,
            },
        ))
    }
}

#[get("/account/logout")]
fn logout_page(user: AuthCtx<DBConnInst>) -> Template {
    user.logout();
    Template::render(
        "account/logout",
        &TemplateCtx {
            page: "account/login",
            user: None,
            update_url: None,
            error: None,
        },
    )
}

#[get("/account")]
fn account_redirect() -> Redirect {
    Redirect::permanent(uri!("/account/update"))
}

#[derive(Debug, Serialize)]
struct UpdateCtx {
    #[serde(flatten)]
    template: TemplateCtx,
    has_passwd: bool,
}

#[get("/account/update")]
fn update_account_page(user: PUser<'_>) -> Result<Template, Redirect> {
    if let Some(user) = user {
        Ok(Template::render(
            "account/update",
            &UpdateCtx {
                template: TemplateCtx {
                    page: "account/update",
                    user: Some(user.info().to_owned()),
                    update_url: None,
                    error: None,
                },
                has_passwd: user.has_passwd(),
            },
        ))
    } else {
        Err(Redirect::to(uri!("/account/login")))
    }
}

#[derive(Debug, Serialize, Deserialize, FromForm)]
struct UpdateEmail {
    email: String,
}

#[post("/account/update/email", data = "<changes>")]
async fn update_account_email(
    changes: Form<UpdateEmail>,
    db: DBConnInst,
    user: User<'_>,
) -> (Status, Template) {
    todo!()
    //use diesel::{dsl::*, prelude::*};
    //let id = user.id().to_owned();
    //let changes = changes.into_inner();
    //match db
    //.run(move |conn| {
    //update(users::table.filter(users::username.eq(id.0)))
    //.set(users::email.eq(changes.email))
    //.execute(conn)
    //})
    //.await
    //{
    //Ok(1) => (
    //Status::Ok,
    //Template::render(
    //"account/update",
    //&TemplateCtx {
    //page: "account/update",
    //user: Some(user.info().to_owned()),
    //update_url: Some("account/update"),
    //error: None,
    //},
    //),
    //),
    //_ => (
    //Status::InternalServerError,
    //Template::render(
    //"account/update",
    //&TemplateCtx {
    //page: "account/update",
    //user: Some(user.info().to_owned()),
    //update_url: Some("account/update"),
    //error: Some("Failed to update email"),
    //},
    //),
    //),
    //}
}

#[derive(FromForm)]
struct UpdatePassword<'a> {
    old_password: Password<'a>,
    new_password: Password<'a>,
}
impl std::fmt::Debug for UpdatePassword<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "")}
}

#[post("/account/update/password", data = "<changes>")]
async fn update_account_password(
    changes: Form<UpdatePassword<'_>>,
    user: User<'_>,
    mut auth_ctx: AuthCtx<'_, DBConnInst>,
) -> (Status, Template) {
    let changes = changes.into_inner();
    match auth_ctx
        .update_userauth(user, changes.old_password, changes.new_password)
        .await
    {
        Ok(AuthUpdate::Ok(user)) => (
            Status::Ok,
            Template::render(
                "account/update",
                &TemplateCtx {
                    page: "account/update",
                    user: Some(user.info().to_owned()),
                    update_url: Some("account/update"),
                    error: None,
                },
            ),
        ),
        Ok(AuthUpdate::Failed(user)) => (
            Status::Unauthorized,
            Template::render(
                "account/update",
                &TemplateCtx {
                    page: "account/update",
                    user: Some(user.info().to_owned()),
                    update_url: Some("account/update"),
                    error: Some("Invalid Password"),
                },
            ),
        ),
        Err(_e) => (
            Status::InternalServerError,
            Template::render(
                "account/update",
                &TemplateCtx {
                    page: "account/update",
                    user: None,
                    update_url: Some("account/update"),
                    error: Some("Internal Server Error"),
                },
            ),
        ),
    }
}

#[derive(Debug, Serialize)]
struct AdminCtx {
    #[serde(flatten)]
    template: TemplateCtx,
}

#[get("/admin")]
fn admin_panel(user: ViewAdminPanel<'_, DBConnInst>) -> Template {
    let template = TemplateCtx {
        user: Some(user.info().clone()),
        update_url: None,
        error: None,
        page: "admin/panel",
    };
    Template::render("admin/panel", &AdminCtx { template })
}

#[derive(Debug, Serialize)]
struct UserHandle {
    id: String,
    username: String,
    email: String,
    role: &'static str,
}

#[derive(Debug, Serialize)]
struct AdminUserCtx {
    users: Vec<UserHandle>,
    #[serde(flatten)]
    template: TemplateCtx,
}

#[get("/admin/users")]
async fn admin_users(user: ViewAdminPanel<'_, DBConnInst>, db: DBConnInst) -> Template {
    todo!()
    //let template = TemplateCtx {
    //user: Some(user.info().clone()),
    //update_url: None,
    //error: None,
    //page: "admin/users",
    //};
    //use diesel::prelude::*;

    //let users = match db
    //.run(|conn| {
    //users::table
    //.select(UserInfoWithAuth::query())
    //.get_results::<UserInfoWithAuth>(conn)
    //})
    //.await
    //{
    //Ok(_r) => _r
    //.into_iter()
    //.map(|u| UserHandle {
    //id: u.id,
    //username: u.username,
    //email: u.email,
    //role: u.role.try_into().map_or("Error", |r: Role| r.as_str()),
    //})
    //.collect(),
    //Err(_e) => {
    //return Template::render(
    //"error",
    //&ErrorTemplateCtx {
    //error_code: 500,
    //template,
    //},
    //)
    //}
    //};
    //Template::render("admin/users", &AdminUserCtx { users, template })
}

#[derive(Debug, Serialize)]
struct UpdateResponse {
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UserUpdate<'a> {
    email: String,
    role: &'a str,
}

#[post("/api/admin/user/<id>", data = "<update>")]
async fn admin_update_user(
    id: String,
    update: Json<UserUpdate<'_>>,
    _user: ManageUsers<'_, DBConnInst>,
    db: DBConnInst,
) -> APIResponse<Empty> {
    todo!()
    // TODO: Audit log action
    //let role = Role::from_param(update.role).unwrap_or(Role::Owner);
    //let email = update.into_inner().email;

    //match db
    //.run(move |conn| {
    //use diesel::{dsl::*, prelude::*};
    //update(users::table.filter(users::id.eq(id)))
    //.set((users::role.eq(role as i32), users::email.eq(email)))
    //.execute(conn)
    //})
    //.await
    //{
    //Ok(0) => APIResponse::empty(),// TODO: check if user id was found
    //Ok(1) => APIResponse::empty(),
    //Ok(2..) => APIResponse::internal_error(Error { text: format!("Duplicate User Ids found") }),
    //Err(e) => APIResponse::internal_error(Error { text: format!("Internal Error: {:?}", e) }),
    //Ok(_) => unreachable!(),
    //}
}

#[get("/admin/<_..>", rank = 2)]
fn get_admin_redirect() -> Status {
    Status::Unauthorized
}
#[post("/admin/<_..>", rank = 2)]
fn post_admin_redirect() -> Status {
    Status::Unauthorized
}

#[get("/account/<_..>", rank = 2)]
fn get_account_redirect() -> Redirect {
    Redirect::temporary(uri!("/account/login"))
}

#[post("/account/<_..>", rank = 2)]
fn post_account_redirect() -> Redirect {
    Redirect::temporary(uri!("/account/login"))
}
