pub mod auth;
pub mod devices;
pub mod users;

use axum::Router;
use crate::app::AppState;

pub fn public_api_routes() -> Router<AppState> {
    Router::new()
        .merge(auth::public_routes())
        .merge(devices::public_routes())
}

pub fn authed_api_routes() -> Router<AppState> {
    Router::new()
        .merge(auth::authed_routes())
        .merge(devices::routes())
        .merge(users::routes())
}
