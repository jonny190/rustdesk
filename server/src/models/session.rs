use chrono::{DateTime, Duration, Utc};
use rand::RngCore;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Session {
    pub id: Uuid,
    pub user_id: Uuid,
    pub access_token: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

impl Session {
    pub async fn create(pool: &PgPool, user_id: Uuid, expiry_days: i64) -> sqlx::Result<Self> {
        let token = generate_token();
        let expires_at = Utc::now() + Duration::days(expiry_days);
        sqlx::query_as(
            "INSERT INTO sessions (id, user_id, access_token, expires_at)
             VALUES ($1, $2, $3, $4)
             RETURNING *"
        )
        .bind(Uuid::new_v4())
        .bind(user_id)
        .bind(&token)
        .bind(expires_at)
        .fetch_one(pool)
        .await
    }

    pub async fn find_by_token(pool: &PgPool, token: &str) -> sqlx::Result<Option<Self>> {
        sqlx::query_as(
            "SELECT * FROM sessions WHERE access_token = $1 AND expires_at > now()"
        )
        .bind(token)
        .fetch_optional(pool)
        .await
    }

    pub async fn delete_by_token(pool: &PgPool, token: &str) -> sqlx::Result<()> {
        sqlx::query("DELETE FROM sessions WHERE access_token = $1")
            .bind(token)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn delete_expired(pool: &PgPool) -> sqlx::Result<u64> {
        let result = sqlx::query("DELETE FROM sessions WHERE expires_at < now()")
            .execute(pool)
            .await?;
        Ok(result.rows_affected())
    }
}

fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}
