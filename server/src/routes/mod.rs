pub mod ab;
pub mod auth;
pub mod console;
pub mod devices;
pub mod groups;
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
        .merge(ab::routes())
        .merge(groups::routes())
}
