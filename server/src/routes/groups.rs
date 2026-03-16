use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::app::AppState;
use crate::models::DeviceGroup;

#[derive(Deserialize)]
pub struct PaginationParams {
    pub current: Option<i64>,
    #[serde(rename = "pageSize")]
    pub page_size: Option<i64>,
}

#[derive(Serialize)]
pub struct PaginatedResponse<T: Serialize> {
    pub total: i64,
    pub data: Vec<T>,
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/device-group/accessible", get(list_groups))
}

async fn list_groups(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<DeviceGroup>>, StatusCode> {
    let page = params.current.unwrap_or(1).max(1);
    let page_size = params.page_size.unwrap_or(100).min(500);
    let (total, groups) = DeviceGroup::list(&state.pool, page, page_size)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(PaginatedResponse { total, data: groups }))
}
