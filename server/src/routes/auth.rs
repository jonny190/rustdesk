use axum::{
    extract::{Extension, State},
    http::StatusCode,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::app::AppState;
use crate::middleware::AuthUser;
use crate::models::{Session, User, UserPayload};

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    pub id: Option<String>,
    pub uuid: Option<String>,
}

#[derive(Serialize)]
pub struct AuthBody {
    pub r#type: String,
    pub access_token: String,
    pub user: UserPayload,
}

pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/login-options", axum::routing::get(login_options))
}

pub fn authed_routes() -> Router<AppState> {
    Router::new()
        .route("/currentUser", post(current_user))
        .route("/logout", post(logout))
}

async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<AuthBody>, StatusCode> {
    let user = User::find_by_username(&state.pool, &req.username)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if user.status == 0 {
        return Err(StatusCode::FORBIDDEN);
    }

    let password_hash = user.password_hash.as_deref().ok_or(StatusCode::UNAUTHORIZED)?;

    let parsed_hash =
        argon2::PasswordHash::new(password_hash).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    use argon2::PasswordVerifier;
    argon2::Argon2::default()
        .verify_password(req.password.as_bytes(), &parsed_hash)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let session = Session::create(&state.pool, user.id, state.config.session_expiry_days)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(AuthBody {
        r#type: "access_token".into(),
        access_token: session.access_token,
        user: UserPayload::from(&user),
    }))
}

async fn current_user(Extension(auth): Extension<AuthUser>) -> Json<UserPayload> {
    Json(UserPayload::from(&auth.user))
}

async fn logout(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
) -> StatusCode {
    let _ = Session::delete_by_token(&state.pool, &auth.token).await;
    StatusCode::OK
}

async fn login_options() -> Json<Vec<String>> {
    Json(vec![])
}
