use anyhow::Result;
use rustdesk_server_console::app;
use rustdesk_server_console::config::ServerConfig;
use rustdesk_server_console::db;
use rustdesk_server_console::models;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = ServerConfig::from_env();
    let listen_addr = config.listen_addr.clone();
    tracing::info!("rustdesk-server-console starting on {listen_addr}");

    let pool = db::init_pool(&config.database_url).await?;
    tracing::info!("Database connected and migrations applied");

    let state = app::AppState { pool: pool.clone(), config };
    let router = app::create_router(state);

    let cleanup_pool = pool.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            interval.tick().await;
            if let Ok(n) = models::Session::delete_expired(&cleanup_pool).await {
                if n > 0 { tracing::info!("Cleaned up {n} expired sessions"); }
            }
        }
    });

    let stale_pool = pool;
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            let _ = models::Device::mark_offline_stale(&stale_pool, 90).await;
        }
    });

    let listener = tokio::net::TcpListener::bind(&listen_addr).await?;
    tracing::info!("Listening on {listen_addr}");
    axum::serve(listener, router).await?;
    Ok(())
}
