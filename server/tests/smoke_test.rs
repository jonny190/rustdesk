//! Smoke tests for the management server.
//! Requires: TEST_DATABASE_URL env var pointing to a test PostgreSQL database.
//! Tests must run serially: cargo test -p rustdesk-server-console -- --test-threads=1

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

async fn setup() -> (axum::Router, sqlx::PgPool) {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be set for integration tests");

    let pool = sqlx::PgPool::connect(&database_url).await.unwrap();

    // Clean slate
    sqlx::raw_sql("DROP TABLE IF EXISTS _migrations, sessions, devices, device_groups, users CASCADE")
        .execute(&pool)
        .await
        .unwrap();

    rustdesk_server_console::db::init_pool_from_existing(pool.clone()).await.unwrap();

    let config = rustdesk_server_console::config::ServerConfig {
        listen_addr: "127.0.0.1:0".into(),
        database_url,
        secret_key: "test-secret".into(),
        hbbs_url: String::new(),
        session_expiry_days: 1,
    };

    let state = rustdesk_server_console::app::AppState { pool: pool.clone(), config };
    let router = rustdesk_server_console::app::create_router(state);

    (router, pool)
}

#[tokio::test]
async fn test_setup_page_shown_when_no_users() {
    let (app, _pool) = setup().await;

    let response = app
        .oneshot(Request::get("/console/login").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        response.headers().get("location").unwrap().to_str().unwrap(),
        "/console/setup"
    );
}

#[tokio::test]
async fn test_login_returns_unauthorized_for_bad_credentials() {
    let (app, _pool) = setup().await;

    let response = app
        .oneshot(
            Request::post("/api/login")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"username":"nope","password":"nope"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_heartbeat_registers_device() {
    let (app, pool) = setup().await;

    let response = app
        .oneshot(
            Request::post("/api/heartbeat")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"id":"test123","uuid":"abc"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let device: Option<(String,)> =
        sqlx::query_as("SELECT id FROM devices WHERE id = 'test123'")
            .fetch_optional(&pool)
            .await
            .unwrap();
    assert!(device.is_some());
}
