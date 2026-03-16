use axum::{middleware as axum_mw, Router};
use sqlx::PgPool;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use crate::config::ServerConfig;
use crate::middleware;
use crate::routes;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: ServerConfig,
}

pub fn create_router(state: AppState) -> Router {
    let public_api = routes::public_api_routes();
    let authed_api = routes::authed_api_routes()
        .layer(axum_mw::from_fn_with_state(state.pool.clone(), middleware::api_auth));

    let api = Router::new()
        .merge(public_api)
        .merge(authed_api)
        .layer(CorsLayer::permissive());

    let console_public = routes::console::public_routes();
    let console_authed = routes::console::authed_routes(state.pool.clone());

    Router::new()
        .nest("/api", api)
        .merge(console_public)
        .merge(console_authed)
        .nest_service(
            "/static",
            ServeDir::new(
                std::env::var("STATIC_DIR").unwrap_or_else(|_| "server/src/static".into()),
            ),
        )
        .with_state(state)
}
