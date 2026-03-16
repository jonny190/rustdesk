use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::app::AppState;
use crate::middleware::AuthUser;
use crate::models::{AbPeer, AbTag, AddressBook};

#[derive(Deserialize)]
pub struct PaginationParams {
    pub current: Option<i64>,
    #[serde(rename = "pageSize")]
    pub page_size: Option<i64>,
    pub ab: Option<Uuid>,
}

#[derive(Serialize)]
pub struct PaginatedResponse<T: Serialize> {
    pub total: i64,
    pub data: Vec<T>,
}

#[derive(Serialize)]
pub struct AbSettingsResponse {
    pub max_peers: i64,
}

#[derive(Serialize)]
pub struct AbPersonalResponse {
    pub guid: String,
}

#[derive(Serialize)]
pub struct AbProfile {
    pub guid: String,
    pub name: String,
    pub owner: String,
    pub rule: i16,
}

#[derive(Deserialize)]
pub struct AddPeerRequest {
    pub id: String,
    pub alias: Option<String>,
    pub tags: Option<Vec<String>>,
    pub note: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdatePeerRequest {
    pub id: Option<String>,
    pub alias: Option<String>,
    pub tags: Option<Vec<String>>,
    pub note: Option<String>,
}

#[derive(Deserialize)]
pub struct AddTagRequest {
    pub name: String,
    pub color: Option<i32>,
}

#[derive(Deserialize)]
pub struct RenameTagRequest {
    pub old: String,
    pub new: String,
}

#[derive(Deserialize)]
pub struct UpdateTagRequest {
    pub name: String,
    pub color: i32,
}

#[derive(Deserialize)]
pub struct DeleteRequest {
    pub ids: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize)]
pub struct LegacyAb {
    pub peers: Vec<serde_json::Value>,
    pub tags: Vec<String>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/ab", get(get_legacy_ab).post(push_legacy_ab))
        .route("/ab/settings", post(ab_settings))
        .route("/ab/personal", post(ab_personal))
        .route("/ab/shared/profiles", post(shared_profiles))
        .route("/ab/peers", post(list_peers))
        .route("/ab/peer/add/{guid}", post(add_peer))
        .route("/ab/peer/update/{guid}", put(update_peer))
        .route("/ab/peer/{guid}", delete(delete_peers))
        .route("/ab/tags/{guid}", post(list_tags))
        .route("/ab/tag/add/{guid}", post(add_tag))
        .route("/ab/tag/rename/{guid}", put(rename_tag))
        .route("/ab/tag/update/{guid}", put(update_tag))
        .route("/ab/tag/{guid}", delete(delete_tags))
}

async fn ab_settings() -> Json<AbSettingsResponse> {
    Json(AbSettingsResponse { max_peers: 1000 })
}

async fn ab_personal(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
) -> Result<Json<AbPersonalResponse>, StatusCode> {
    let ab = AddressBook::get_or_create_personal(&state.pool, auth.user.id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(AbPersonalResponse {
        guid: ab.id.to_string(),
    }))
}

async fn shared_profiles(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<AbProfile>>, StatusCode> {
    let page = params.current.unwrap_or(1).max(1);
    let page_size = params.page_size.unwrap_or(100).min(500);
    let (total, books) =
        AddressBook::list_shared_for_user(&state.pool, auth.user.id, page, page_size)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let data = books
        .iter()
        .map(|b| AbProfile {
            guid: b.id.to_string(),
            name: b.name.clone(),
            owner: b.owner_id.to_string(),
            rule: 3,
        })
        .collect();
    Ok(Json(PaginatedResponse { total, data }))
}

async fn list_peers(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<AbPeer>>, StatusCode> {
    let ab_id = params.ab.ok_or(StatusCode::BAD_REQUEST)?;
    let page = params.current.unwrap_or(1).max(1);
    let page_size = params.page_size.unwrap_or(100).min(500);
    let (total, peers) = AbPeer::list(&state.pool, ab_id, page, page_size)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(PaginatedResponse { total, data: peers }))
}

async fn add_peer(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(guid): Path<Uuid>,
    Json(req): Json<AddPeerRequest>,
) -> Result<StatusCode, StatusCode> {
    AbPeer::add(
        &state.pool,
        guid,
        &req.id,
        req.alias.as_deref(),
        req.tags.as_deref(),
        req.note.as_deref(),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

async fn update_peer(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(guid): Path<Uuid>,
    Json(req): Json<UpdatePeerRequest>,
) -> Result<StatusCode, StatusCode> {
    let peer_id = req.id.as_deref().ok_or(StatusCode::BAD_REQUEST)?;
    AbPeer::update(
        &state.pool,
        guid,
        peer_id,
        req.alias.as_deref(),
        req.tags.as_deref(),
        req.note.as_deref(),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

async fn delete_peers(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(guid): Path<Uuid>,
    Json(req): Json<DeleteRequest>,
) -> Result<StatusCode, StatusCode> {
    AbPeer::delete(&state.pool, guid, &req.ids.unwrap_or_default())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

async fn list_tags(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(guid): Path<Uuid>,
) -> Result<Json<Vec<AbTag>>, StatusCode> {
    AbTag::list(&state.pool, guid)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn add_tag(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(guid): Path<Uuid>,
    Json(req): Json<AddTagRequest>,
) -> Result<StatusCode, StatusCode> {
    AbTag::add(&state.pool, guid, &req.name, req.color)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

async fn rename_tag(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(guid): Path<Uuid>,
    Json(req): Json<RenameTagRequest>,
) -> Result<StatusCode, StatusCode> {
    AbTag::rename(&state.pool, guid, &req.old, &req.new)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

async fn update_tag(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(guid): Path<Uuid>,
    Json(req): Json<UpdateTagRequest>,
) -> Result<StatusCode, StatusCode> {
    AbTag::update_color(&state.pool, guid, &req.name, req.color)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

async fn delete_tags(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(guid): Path<Uuid>,
    Json(req): Json<DeleteRequest>,
) -> Result<StatusCode, StatusCode> {
    AbTag::delete(&state.pool, guid, &req.ids.unwrap_or_default())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

async fn get_legacy_ab(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
) -> Result<Json<LegacyAb>, StatusCode> {
    let ab = AddressBook::get_or_create_personal(&state.pool, auth.user.id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let peers = AbPeer::list_all_for_ab(&state.pool, ab.id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let tags = AbTag::list(&state.pool, ab.id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let peer_values = peers
        .iter()
        .map(|p| serde_json::json!({"id": p.peer_id, "alias": p.alias, "tags": p.tags}))
        .collect();
    let tag_names = tags.into_iter().map(|t| t.name).collect();
    Ok(Json(LegacyAb {
        peers: peer_values,
        tags: tag_names,
    }))
}

async fn push_legacy_ab(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(data): Json<LegacyAb>,
) -> Result<StatusCode, StatusCode> {
    let ab = AddressBook::get_or_create_personal(&state.pool, auth.user.id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    sqlx::query("DELETE FROM ab_peers WHERE address_book_id = $1")
        .bind(ab.id)
        .execute(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    for peer in &data.peers {
        let peer_id = peer
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let alias = peer.get("alias").and_then(|v| v.as_str());
        let tags: Option<Vec<String>> = peer.get("tags").and_then(|v| {
            v.as_array().map(|arr| {
                arr.iter()
                    .filter_map(|t| t.as_str().map(String::from))
                    .collect()
            })
        });
        let _ = AbPeer::add(&state.pool, ab.id, peer_id, alias, tags.as_deref(), None).await;
    }

    sqlx::query("DELETE FROM ab_tags WHERE address_book_id = $1")
        .bind(ab.id)
        .execute(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    for tag_name in &data.tags {
        let _ = AbTag::add(&state.pool, ab.id, tag_name, None).await;
    }

    Ok(StatusCode::OK)
}
