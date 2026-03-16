use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHasher};
use askama::Template;
use axum::{
    extract::{Extension, Path, Query, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Form, Router,
};
use rand::rngs::OsRng;
use serde::Deserialize;
use uuid::Uuid;

use crate::app::AppState;
use crate::middleware::{self, AuthUser};
use crate::models::{AbPeer, AbShare, AbTag, AddressBook, Device, DeviceGroup, Session, SettingsPolicy, User};

// -- Template structs --

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate {
    error: Option<String>,
}

#[derive(Template)]
#[template(path = "setup.html")]
struct SetupTemplate {
    error: Option<String>,
}

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate {}

#[derive(Template)]
#[template(path = "users/list.html")]
struct UsersListTemplate {}

#[derive(Template)]
#[template(path = "users/detail.html")]
struct UserDetailTemplate {
    user: User,
}

#[derive(Template)]
#[template(path = "devices/list.html")]
struct DevicesListTemplate {}

#[derive(Template)]
#[template(path = "devices/detail.html")]
struct DeviceDetailTemplate {
    device: Device,
}

#[derive(Template)]
#[template(path = "groups/list.html")]
struct GroupsListTemplate {}

#[derive(Template)]
#[template(path = "groups/detail.html")]
struct GroupDetailTemplate {
    group: DeviceGroup,
}

#[derive(Template)]
#[template(path = "address_books/list.html")]
struct AbListTemplate {}

#[derive(Template)]
#[template(path = "address_books/detail.html")]
struct AbDetailTemplate {
    ab: AddressBook,
}

#[derive(Template)]
#[template(path = "settings/global.html")]
struct SettingsTemplate {
    name: String,
    config_options: String,
}

// -- Form structs --

#[derive(Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct SetupForm {
    pub username: String,
    pub email: Option<String>,
    pub password: String,
    pub password_confirm: String,
}

#[derive(Deserialize)]
pub struct UserForm {
    pub username: String,
    pub email: Option<String>,
    pub password: Option<String>,
    pub is_admin: Option<String>,
    pub status: i16,
}

#[derive(Deserialize)]
pub struct DeviceUpdateForm {
    pub note: Option<String>,
    pub custom_id: Option<String>,
}

#[derive(Deserialize)]
pub struct SearchParams {
    pub search: Option<String>,
    pub page: Option<i64>,
}

#[derive(Deserialize)]
pub struct GroupForm {
    pub name: String,
    pub note: Option<String>,
}

#[derive(Deserialize)]
pub struct SettingsForm {
    pub target_type: String,
    pub name: String,
    pub config_options: String,
}

// -- Route registration --

pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/console/login", get(login_page).post(login_submit))
        .route("/console/setup", get(setup_page).post(setup_submit))
}

pub fn authed_routes(pool: sqlx::PgPool) -> Router<AppState> {
    Router::new()
        .route("/console/", get(dashboard))
        .route("/console/logout", get(logout))
        .route("/console/users", get(users_list))
        .route("/console/users/new", get(user_new_page).post(user_create))
        .route("/console/users/table", get(users_table_fragment))
        .route("/console/users/{id}", get(user_detail).post(user_update))
        .route("/console/users/{id}/delete", post(user_delete))
        .route("/console/devices", get(devices_list))
        .route("/console/devices/table", get(devices_table_fragment))
        .route(
            "/console/devices/{id}",
            get(device_detail).post(device_update),
        )
        .route("/console/groups", get(groups_list))
        .route("/console/groups/table", get(groups_table_fragment))
        .route("/console/groups/new-form", get(group_new_form))
        .route("/console/groups/new", post(group_create))
        .route("/console/groups/{id}", get(group_detail).post(group_update))
        .route("/console/groups/{id}/delete", post(group_delete))
        .route("/console/groups/{id}/devices", get(group_devices_fragment))
        .route("/console/address-books", get(ab_list))
        .route("/console/address-books/table", get(ab_table_fragment))
        .route("/console/address-books/{guid}", get(ab_detail))
        .route("/console/address-books/{guid}/peers", get(ab_peers_fragment))
        .route("/console/address-books/{guid}/tags", get(ab_tags_fragment))
        .route("/console/address-books/{guid}/shares", get(ab_shares_fragment))
        .route("/console/settings", get(settings_page).post(settings_save))
        .route("/console/settings/group-policies", get(settings_group_policies_fragment))
        .route("/console/stats/users", get(stats_users))
        .route(
            "/console/stats/devices-online",
            get(stats_devices_online),
        )
        .route("/console/stats/devices-total", get(stats_devices_total))
        .layer(axum::middleware::from_fn_with_state(
            pool,
            middleware::console_auth,
        ))
}

// -- Auth handlers --

async fn login_page(State(state): State<AppState>) -> Response {
    if let Ok(count) = User::count(&state.pool).await {
        if count == 0 {
            return Redirect::to("/console/setup").into_response();
        }
    }
    Html(
        LoginTemplate { error: None }
            .render()
            .unwrap_or_default(),
    )
    .into_response()
}

async fn login_submit(State(state): State<AppState>, Form(form): Form<LoginForm>) -> Response {
    let render_error = |msg: &str| {
        Html(
            LoginTemplate {
                error: Some(msg.to_string()),
            }
            .render()
            .unwrap_or_default(),
        )
        .into_response()
    };

    let user = match User::find_by_username(&state.pool, &form.username).await {
        Ok(Some(u)) => u,
        _ => return render_error("Invalid username or password"),
    };

    if user.status == 0 {
        return render_error("Account is disabled");
    }

    let password_hash = match &user.password_hash {
        Some(h) => h.clone(),
        None => return render_error("Invalid username or password"),
    };

    let parsed_hash = match argon2::PasswordHash::new(&password_hash) {
        Ok(h) => h,
        Err(_) => return render_error("Internal error"),
    };

    use argon2::PasswordVerifier;
    if Argon2::default()
        .verify_password(form.password.as_bytes(), &parsed_hash)
        .is_err()
    {
        return render_error("Invalid username or password");
    }

    let session = match Session::create(&state.pool, user.id, state.config.session_expiry_days).await
    {
        Ok(s) => s,
        Err(_) => return render_error("Internal error"),
    };

    let cookie = format!(
        "session={}; HttpOnly; Path=/; SameSite=Lax; Max-Age={}",
        session.access_token,
        state.config.session_expiry_days * 86400
    );

    (
        StatusCode::SEE_OTHER,
        [
            (header::SET_COOKIE, cookie),
            (header::LOCATION, "/console/".into()),
        ],
    )
        .into_response()
}

async fn setup_page(State(state): State<AppState>) -> Response {
    if let Ok(count) = User::count(&state.pool).await {
        if count > 0 {
            return Redirect::to("/console/login").into_response();
        }
    }
    Html(
        SetupTemplate { error: None }
            .render()
            .unwrap_or_default(),
    )
    .into_response()
}

async fn setup_submit(State(state): State<AppState>, Form(form): Form<SetupForm>) -> Response {
    let render_error = |msg: &str| {
        Html(
            SetupTemplate {
                error: Some(msg.to_string()),
            }
            .render()
            .unwrap_or_default(),
        )
        .into_response()
    };

    if let Ok(count) = User::count(&state.pool).await {
        if count > 0 {
            return Redirect::to("/console/login").into_response();
        }
    }

    if form.password != form.password_confirm {
        return render_error("Passwords do not match");
    }
    if form.password.len() < 8 {
        return render_error("Password must be at least 8 characters");
    }
    if form.username.trim().is_empty() {
        return render_error("Username is required");
    }

    let salt = SaltString::generate(&mut OsRng);
    let password_hash = match Argon2::default().hash_password(form.password.as_bytes(), &salt) {
        Ok(h) => h.to_string(),
        Err(_) => return render_error("Internal error"),
    };

    let email = form.email.as_deref().filter(|e| !e.is_empty());

    match User::create(&state.pool, form.username.trim(), email, &password_hash, true).await {
        Ok(_) => Redirect::to("/console/login").into_response(),
        Err(e) => render_error(&format!("Failed to create admin: {e}")),
    }
}

async fn logout(State(state): State<AppState>, Extension(auth): Extension<AuthUser>) -> Response {
    let _ = Session::delete_by_token(&state.pool, &auth.token).await;
    let cookie = "session=; HttpOnly; Path=/; Max-Age=0";
    (
        StatusCode::SEE_OTHER,
        [
            (header::SET_COOKIE, cookie.to_string()),
            (header::LOCATION, "/console/login".into()),
        ],
    )
        .into_response()
}

// -- Page handlers --

async fn dashboard() -> Html<String> {
    Html(DashboardTemplate {}.render().unwrap_or_default())
}

async fn users_list() -> Html<String> {
    Html(UsersListTemplate {}.render().unwrap_or_default())
}

async fn user_detail(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Html<String>, StatusCode> {
    let user = User::find_by_id(&state.pool, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Html(
        UserDetailTemplate { user }.render().unwrap_or_default(),
    ))
}

async fn devices_list() -> Html<String> {
    Html(DevicesListTemplate {}.render().unwrap_or_default())
}

async fn device_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Html<String>, StatusCode> {
    let device = Device::find_by_id(&state.pool, &id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Html(
        DeviceDetailTemplate { device }.render().unwrap_or_default(),
    ))
}

// -- User CRUD --

async fn user_new_page() -> Html<String> {
    Html(
        r#"<form method="POST" action="/console/users/new">
        <label for="username">Username</label>
        <input type="text" id="username" name="username" required>
        <label for="email">Email</label>
        <input type="email" id="email" name="email">
        <label for="password">Password</label>
        <input type="password" id="password" name="password" required minlength="8">
        <fieldset><label><input type="checkbox" name="is_admin"> Administrator</label></fieldset>
        <input type="hidden" name="status" value="1">
        <button type="submit">Create User</button>
    </form>"#
            .to_string(),
    )
}

async fn user_create(State(state): State<AppState>, Form(form): Form<UserForm>) -> Response {
    if form.username.trim().is_empty() {
        return Redirect::to("/console/users").into_response();
    }

    let password = form.password.as_deref().unwrap_or("changeme");
    let salt = SaltString::generate(&mut OsRng);
    let password_hash = match Argon2::default().hash_password(password.as_bytes(), &salt) {
        Ok(h) => h.to_string(),
        Err(_) => return Redirect::to("/console/users").into_response(),
    };

    let email = form.email.as_deref().filter(|e| !e.is_empty());
    let is_admin = form.is_admin.is_some();

    let _ = User::create(&state.pool, form.username.trim(), email, &password_hash, is_admin).await;
    Redirect::to("/console/users").into_response()
}

async fn user_update(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Form(form): Form<UserForm>,
) -> Response {
    let is_admin = form.is_admin.is_some();
    let email = form.email.as_deref().filter(|e| !e.is_empty());

    let _ = sqlx::query(
        "UPDATE users SET username = $1, email = $2, is_admin = $3, status = $4, updated_at = now() WHERE id = $5",
    )
    .bind(form.username.trim())
    .bind(email)
    .bind(is_admin)
    .bind(form.status)
    .bind(id)
    .execute(&state.pool)
    .await;

    Redirect::to(&format!("/console/users/{id}")).into_response()
}

async fn user_delete(State(state): State<AppState>, Path(id): Path<Uuid>) -> Response {
    let _ = User::delete(&state.pool, id).await;
    Redirect::to("/console/users").into_response()
}

async fn device_update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Form(form): Form<DeviceUpdateForm>,
) -> Response {
    let _ = sqlx::query(
        "UPDATE devices SET note = $1, custom_id = NULLIF($2, ''), updated_at = now() WHERE id = $3",
    )
    .bind(&form.note)
    .bind(&form.custom_id)
    .bind(&id)
    .execute(&state.pool)
    .await;

    Redirect::to(&format!("/console/devices/{id}")).into_response()
}

// -- HTMX fragment endpoints --

async fn stats_users(State(state): State<AppState>) -> String {
    User::count(&state.pool)
        .await
        .map(|c| c.to_string())
        .unwrap_or_else(|_| "-".into())
}

async fn stats_devices_online(State(state): State<AppState>) -> String {
    let result: Result<i64, _> =
        sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE status = 1")
            .fetch_one(&state.pool)
            .await;
    result
        .map(|c| c.to_string())
        .unwrap_or_else(|_| "-".into())
}

async fn stats_devices_total(State(state): State<AppState>) -> String {
    let result: Result<i64, _> = sqlx::query_scalar("SELECT COUNT(*) FROM devices")
        .fetch_one(&state.pool)
        .await;
    result
        .map(|c| c.to_string())
        .unwrap_or_else(|_| "-".into())
}

async fn users_table_fragment(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> Html<String> {
    let page = params.page.unwrap_or(1).max(1);
    let (_, users) = User::list(&state.pool, page, 50, params.search.as_deref())
        .await
        .unwrap_or_default();

    let mut html = String::new();
    for u in &users {
        let status = match u.status {
            1 => "Normal",
            0 => "Disabled",
            _ => "Unverified",
        };
        let role = if u.is_admin { "Admin" } else { "User" };
        html.push_str(&format!(
            "<tr><td><a href=\"/console/users/{}\">{}</a></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td><a href=\"/console/users/{}\">Edit</a></td></tr>",
            u.id,
            u.username,
            u.email.as_deref().unwrap_or("-"),
            role,
            status,
            u.created_at.format("%Y-%m-%d"),
            u.id
        ));
    }
    if users.is_empty() {
        html.push_str("<tr><td colspan=\"6\">No users found</td></tr>");
    }
    Html(html)
}

async fn devices_table_fragment(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> Html<String> {
    let page = params.page.unwrap_or(1).max(1);
    let (_, devices) = match params.search.as_deref().filter(|s| !s.is_empty()) {
        Some(q) => Device::search(&state.pool, q, page, 50)
            .await
            .unwrap_or_default(),
        None => Device::list(&state.pool, page, 50, None)
            .await
            .unwrap_or_default(),
    };

    let mut html = String::new();
    for d in &devices {
        let status = match d.status {
            1 => "<mark>Online</mark>",
            0 => "Disabled",
            _ => "Offline",
        };
        let last_seen = d
            .last_heartbeat
            .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "-".into());
        html.push_str(&format!(
            "<tr><td><a href=\"/console/devices/{}\">{}</a></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td><a href=\"/console/devices/{}\">View</a></td></tr>",
            d.id,
            d.id,
            d.hostname.as_deref().unwrap_or("-"),
            d.os.as_deref().unwrap_or("-"),
            status,
            last_seen,
            d.id
        ));
    }
    if devices.is_empty() {
        html.push_str("<tr><td colspan=\"6\">No devices registered</td></tr>");
    }
    Html(html)
}

// -- Group handlers --

async fn groups_list() -> Html<String> {
    Html(GroupsListTemplate {}.render().unwrap_or_default())
}

async fn groups_table_fragment(State(state): State<AppState>) -> Html<String> {
    let (_, groups) = DeviceGroup::list(&state.pool, 1, 100)
        .await
        .unwrap_or_default();
    let mut html = String::new();
    for g in &groups {
        html.push_str(&format!(
            "<tr><td><a href=\"/console/groups/{}\">{}</a></td><td>{}</td><td>{}</td><td><a href=\"/console/groups/{}\">Edit</a></td></tr>",
            g.id, g.name, g.note.as_deref().unwrap_or("-"), g.created_at.format("%Y-%m-%d"), g.id
        ));
    }
    if groups.is_empty() {
        html.push_str("<tr><td colspan=\"4\">No groups</td></tr>");
    }
    Html(html)
}

async fn group_new_form() -> Html<String> {
    Html(
        r#"<form method="POST" action="/console/groups/new" style="margin-bottom: 1rem;">
        <div style="display: flex; gap: 0.5rem; align-items: end;">
            <input type="text" name="name" placeholder="Group name" required style="margin-bottom: 0;">
            <input type="text" name="note" placeholder="Note (optional)" style="margin-bottom: 0;">
            <button type="submit">Create</button>
        </div>
    </form>"#
            .to_string(),
    )
}

async fn group_create(State(state): State<AppState>, Form(form): Form<GroupForm>) -> Response {
    let _ = DeviceGroup::create(&state.pool, &form.name, form.note.as_deref()).await;
    Redirect::to("/console/groups").into_response()
}

async fn group_detail(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Html<String>, StatusCode> {
    let group = DeviceGroup::find_by_id(&state.pool, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Html(
        GroupDetailTemplate { group }.render().unwrap_or_default(),
    ))
}

async fn group_update(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Form(form): Form<GroupForm>,
) -> Response {
    let _ = DeviceGroup::update(&state.pool, id, &form.name, form.note.as_deref()).await;
    Redirect::to(&format!("/console/groups/{id}")).into_response()
}

async fn group_delete(State(state): State<AppState>, Path(id): Path<Uuid>) -> Response {
    let _ = DeviceGroup::delete(&state.pool, id).await;
    Redirect::to("/console/groups").into_response()
}

async fn group_devices_fragment(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Html<String> {
    let devices: Vec<Device> =
        sqlx::query_as("SELECT * FROM devices WHERE device_group_id = $1 ORDER BY id")
            .bind(id)
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default();
    let mut html = String::from(
        "<table><thead><tr><th>ID</th><th>Hostname</th><th>Status</th></tr></thead><tbody>",
    );
    for d in &devices {
        let status = match d.status {
            1 => "Online",
            0 => "Disabled",
            _ => "Offline",
        };
        html.push_str(&format!(
            "<tr><td><a href=\"/console/devices/{}\">{}</a></td><td>{}</td><td>{}</td></tr>",
            d.id,
            d.id,
            d.hostname.as_deref().unwrap_or("-"),
            status
        ));
    }
    if devices.is_empty() {
        html.push_str("<tr><td colspan=\"3\">No devices in this group</td></tr>");
    }
    html.push_str("</tbody></table>");
    Html(html)
}

// -- Address book console handlers --

async fn ab_list() -> Html<String> {
    Html(AbListTemplate {}.render().unwrap_or_default())
}

async fn ab_table_fragment(State(state): State<AppState>) -> Html<String> {
    let (_, books) = AddressBook::list_all(&state.pool, 1, 100)
        .await
        .unwrap_or_default();
    let mut html = String::new();
    for b in &books {
        let personal = if b.is_personal { "Yes" } else { "No" };
        html.push_str(&format!(
            "<tr><td><a href=\"/console/address-books/{}\">{}</a></td><td>{}</td><td>{}</td><td>{}</td><td><a href=\"/console/address-books/{}\">View</a></td></tr>",
            b.id, b.name, b.owner_id, personal, b.created_at.format("%Y-%m-%d"), b.id
        ));
    }
    if books.is_empty() {
        html.push_str("<tr><td colspan=\"5\">No address books</td></tr>");
    }
    Html(html)
}

async fn ab_detail(
    State(state): State<AppState>,
    Path(guid): Path<Uuid>,
) -> Result<Html<String>, StatusCode> {
    let ab = AddressBook::find_by_id(&state.pool, guid)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Html(AbDetailTemplate { ab }.render().unwrap_or_default()))
}

async fn ab_peers_fragment(
    State(state): State<AppState>,
    Path(guid): Path<Uuid>,
) -> Html<String> {
    let (_, peers) = AbPeer::list(&state.pool, guid, 1, 200)
        .await
        .unwrap_or_default();
    let mut html = String::new();
    for p in &peers {
        let tags_str = p
            .tags
            .as_ref()
            .map(|t| t.join(", "))
            .unwrap_or_default();
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            p.peer_id,
            p.alias.as_deref().unwrap_or("-"),
            tags_str,
            p.note.as_deref().unwrap_or("-")
        ));
    }
    if peers.is_empty() {
        html.push_str("<tr><td colspan=\"4\">No peers</td></tr>");
    }
    Html(html)
}

async fn ab_tags_fragment(
    State(state): State<AppState>,
    Path(guid): Path<Uuid>,
) -> Html<String> {
    let tags = AbTag::list(&state.pool, guid).await.unwrap_or_default();
    let mut html = String::new();
    for t in &tags {
        html.push_str(&format!(
            "<span style=\"margin-right: 0.5rem; padding: 0.2rem 0.5rem; background: var(--pico-card-background-color); border-radius: 4px;\">{}</span>",
            t.name
        ));
    }
    if tags.is_empty() {
        html.push_str("No tags");
    }
    Html(html)
}

async fn ab_shares_fragment(
    State(state): State<AppState>,
    Path(guid): Path<Uuid>,
) -> Html<String> {
    let shares = AbShare::list_for_ab(&state.pool, guid)
        .await
        .unwrap_or_default();
    let mut html = String::from(
        "<table><thead><tr><th>Shared With</th><th>Rule</th></tr></thead><tbody>",
    );
    for s in &shares {
        let who = s
            .user_id
            .map(|u| format!("User: {u}"))
            .or_else(|| s.group_id.map(|g| format!("Group: {g}")))
            .unwrap_or_else(|| "-".into());
        let rule = match s.rule {
            1 => "Read",
            2 => "Read/Write",
            3 => "Full Control",
            _ => "Unknown",
        };
        html.push_str(&format!("<tr><td>{}</td><td>{}</td></tr>", who, rule));
    }
    if shares.is_empty() {
        html.push_str("<tr><td colspan=\"2\">Not shared</td></tr>");
    }
    html.push_str("</tbody></table>");
    Html(html)
}

// -- Settings console handlers --

async fn settings_page(State(state): State<AppState>) -> Html<String> {
    let global = SettingsPolicy::find_for_target(&state.pool, "global", None)
        .await
        .ok()
        .flatten();
    let name = global
        .as_ref()
        .map(|p| p.name.clone())
        .unwrap_or_else(|| "Global Policy".into());
    let config_options = global
        .as_ref()
        .map(|p| serde_json::to_string_pretty(&p.config_options).unwrap_or_default())
        .unwrap_or_else(|| "{}".into());
    Html(
        SettingsTemplate {
            name,
            config_options,
        }
        .render()
        .unwrap_or_default(),
    )
}

async fn settings_save(
    State(state): State<AppState>,
    Form(form): Form<SettingsForm>,
) -> Response {
    let config: serde_json::Value =
        serde_json::from_str(&form.config_options).unwrap_or(serde_json::json!({}));
    let _ = SettingsPolicy::upsert(
        &state.pool,
        &form.name,
        &form.target_type,
        None,
        &config,
        None,
    )
    .await;
    Redirect::to("/console/settings").into_response()
}

async fn settings_group_policies_fragment(State(state): State<AppState>) -> Html<String> {
    let policies = SettingsPolicy::list_all(&state.pool)
        .await
        .unwrap_or_default();
    let group_policies: Vec<_> = policies.iter().filter(|p| p.target_type == "group").collect();
    let mut html = String::from(
        "<table><thead><tr><th>Name</th><th>Target Group</th><th>Updated</th></tr></thead><tbody>",
    );
    for p in &group_policies {
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td></tr>",
            p.name,
            p.target_id.as_deref().unwrap_or("-"),
            p.updated_at.format("%Y-%m-%d %H:%M")
        ));
    }
    if group_policies.is_empty() {
        html.push_str("<tr><td colspan=\"3\">No group policies</td></tr>");
    }
    html.push_str("</tbody></table>");
    Html(html)
}
