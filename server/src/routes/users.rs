use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::app::AppState;
use crate::models::{Device, User, UserPayload};

#[derive(Deserialize)]
pub struct PaginationParams {
    pub current: Option<i64>,
    #[serde(rename = "pageSize")]
    pub page_size: Option<i64>,
    pub accessible: Option<String>,
    pub status: Option<i16>,
}

#[derive(Serialize)]
pub struct PaginatedResponse<T: Serialize> {
    pub total: i64,
    pub data: Vec<T>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/users", get(list_users))
        .route("/peers", get(list_peers))
}

async fn list_users(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<UserPayload>>, axum::http::StatusCode> {
    let page = params.current.unwrap_or(1).max(1);
    let page_size = params.page_size.unwrap_or(100).min(500);
    let (total, users) = User::list(&state.pool, page, page_size, None)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    let data = users.iter().map(UserPayload::from).collect();
    Ok(Json(PaginatedResponse { total, data }))
}

async fn list_peers(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<Device>>, axum::http::StatusCode> {
    let page = params.current.unwrap_or(1).max(1);
    let page_size = params.page_size.unwrap_or(100).min(500);
    let (total, devices) = Device::list(&state.pool, page, page_size, params.status)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(PaginatedResponse { total, data: devices }))
}
