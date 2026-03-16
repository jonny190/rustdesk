use axum::{
    extract::State,
    http::StatusCode,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::app::AppState;
use crate::models::{Device, SettingsPolicy};

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

#[derive(Deserialize)]
pub struct DeviceCliRequest {
    pub id: Option<String>,
    pub action: Option<String>,
    pub custom_id: Option<String>,
}

#[derive(Serialize)]
pub struct DeviceCliResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/heartbeat", post(heartbeat))
        .route("/sysinfo", post(sysinfo))
        .route("/sysinfo_ver", post(sysinfo_ver))
        .route("/devices/cli", post(devices_cli))
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

    let group_id = device.as_ref().and_then(|d| d.device_group_id);
    let strategy =
        SettingsPolicy::get_effective_for_device(&state.pool, device_id, group_id)
            .await
            .unwrap_or(None);
    let strategy_response = strategy
        .map(|config_options| serde_json::json!({"config_options": config_options, "extra": {}}));

    Ok(Json(HeartbeatResponse {
        sysinfo: if needs_sysinfo { Some(true) } else { None },
        strategy: strategy_response,
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

async fn devices_cli(
    State(state): State<AppState>,
    Json(req): Json<DeviceCliRequest>,
) -> Result<Json<DeviceCliResponse>, StatusCode> {
    let device_id = req.id.as_deref().ok_or(StatusCode::BAD_REQUEST)?;

    if let Some(custom_id) = &req.custom_id {
        if custom_id.is_empty() {
            sqlx::query("UPDATE devices SET custom_id = NULL, updated_at = now() WHERE id = $1")
                .bind(device_id)
                .execute(&state.pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        } else {
            let existing: Option<(String,)> =
                sqlx::query_as("SELECT id FROM devices WHERE custom_id = $1 AND id != $2")
                    .bind(custom_id)
                    .bind(device_id)
                    .fetch_optional(&state.pool)
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            if existing.is_some() {
                return Ok(Json(DeviceCliResponse {
                    error: Some("ID already taken".into()),
                }));
            }
            sqlx::query("UPDATE devices SET custom_id = $1, updated_at = now() WHERE id = $2")
                .bind(custom_id)
                .bind(device_id)
                .execute(&state.pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }

    Ok(Json(DeviceCliResponse { error: None }))
}
