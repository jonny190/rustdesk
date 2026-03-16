use std::env;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub listen_addr: String,
    pub database_url: String,
    pub secret_key: String,
    pub hbbs_url: String,
    pub session_expiry_days: i64,
}

impl ServerConfig {
    pub fn from_env() -> Self {
        Self {
            listen_addr: env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:21114".into()),
            database_url: env::var("DATABASE_URL").expect("DATABASE_URL must be set"),
            secret_key: env::var("SECRET_KEY").expect("SECRET_KEY must be set"),
            hbbs_url: env::var("HBBS_URL").unwrap_or_default(),
            session_expiry_days: env::var("SESSION_EXPIRY_DAYS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(7),
        }
    }
}
