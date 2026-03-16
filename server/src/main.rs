mod config;
mod db;

use anyhow::Result;
use config::ServerConfig;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = ServerConfig::from_env();
    tracing::info!("rustdesk-server-console starting on {}", config.listen_addr);

    let pool = db::init_pool(&config.database_url).await?;
    tracing::info!("Database connected and migrations applied");

    // Keep pool alive (will be used by router in later tasks)
    drop(pool);

    Ok(())
}
