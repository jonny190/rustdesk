use axum::{
    extract::State,
    http::StatusCode,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::app::AppState;
use crate::models::Device;

#[derive(Deserialize)]
pub struct HeartbeatRequest {
    pub id: Option<String>,
    pub uuid: Option<String>,
    pub ver: Option<i64>,
    pub conns: Option<Vec<i64>>,
    pub modified_at: Option<i64>,
}

#[derive(Serialize, Default)]
pub struct HeartbeatResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sysinfo: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disconnect: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy: Option<serde_json::Value>,
}

#[derive(Deserialize)]
pub struct SysinfoRequest {
    pub id: Option<String>,
    pub hostname: Option<String>,
    pub os: Option<String>,
    pub version: Option<String>,
    pub cpu: Option<String>,
    pub memory: Option<String>,
}

pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/heartbeat", post(heartbeat))
        .route("/sysinfo", post(sysinfo))
        .route("/sysinfo_ver", post(sysinfo_ver))
}

pub fn routes() -> Router<AppState> {
    Router::new()
}

async fn heartbeat(
    State(state): State<AppState>,
    Json(req): Json<HeartbeatRequest>,
) -> Result<Json<HeartbeatResponse>, StatusCode> {
    let device_id = req.id.as_deref().ok_or(StatusCode::BAD_REQUEST)?;

    Device::upsert_from_heartbeat(&state.pool, device_id, None)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let device = Device::find_by_id(&state.pool, device_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let needs_sysinfo = device
        .as_ref()
        .map(|d| d.hostname.is_none())
        .unwrap_or(true);

    Ok(Json(HeartbeatResponse {
        sysinfo: if needs_sysinfo { Some(true) } else { None },
        ..Default::default()
    }))
}

async fn sysinfo(
    State(state): State<AppState>,
    Json(req): Json<SysinfoRequest>,
) -> Result<String, StatusCode> {
    let id = req.id.as_deref().ok_or(StatusCode::BAD_REQUEST)?;

    let updated = Device::upsert_sysinfo(
        &state.pool,
        id,
        req.hostname.as_deref().unwrap_or(""),
        req.os.as_deref().unwrap_or(""),
        req.version.as_deref().unwrap_or(""),
        req.cpu.as_deref().unwrap_or(""),
        req.memory.as_deref().unwrap_or(""),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if updated {
        Ok("SYSINFO_UPDATED".into())
    } else {
        Ok("ID_NOT_FOUND".into())
    }
}

async fn sysinfo_ver(
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Result<String, StatusCode> {
    let id = req
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;
    let device = Device::find_by_id(&state.pool, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    match device {
        Some(d) => Ok(d.updated_at.timestamp().to_string()),
        None => Ok("0".into()),
    }
}
