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
use crate::models::{Device, Session, User};

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
