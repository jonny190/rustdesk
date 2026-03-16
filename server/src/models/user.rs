use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub email: Option<String>,
    #[serde(skip_serializing)]
    pub password_hash: Option<String>,
    pub is_admin: bool,
    pub status: i16,
    pub oidc_provider: Option<String>,
    pub oidc_subject: Option<String>,
    #[serde(skip_serializing)]
    pub totp_secret: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct UserPayload {
    pub name: String,
    pub email: Option<String>,
    pub is_admin: bool,
    pub status: i16,
}

impl From<&User> for UserPayload {
    fn from(u: &User) -> Self {
        Self {
            name: u.username.clone(),
            email: u.email.clone(),
            is_admin: u.is_admin,
            status: u.status,
        }
    }
}

impl User {
    pub async fn find_by_id(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<Self>> {
        sqlx::query_as("SELECT * FROM users WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await
    }

    pub async fn find_by_username(pool: &PgPool, username: &str) -> sqlx::Result<Option<Self>> {
        sqlx::query_as("SELECT * FROM users WHERE username = $1")
            .bind(username)
            .fetch_optional(pool)
            .await
    }

    pub async fn create(
        pool: &PgPool,
        username: &str,
        email: Option<&str>,
        password_hash: &str,
        is_admin: bool,
    ) -> sqlx::Result<Self> {
        sqlx::query_as(
            "INSERT INTO users (id, username, email, password_hash, is_admin)
             VALUES ($1, $2, $3, $4, $5)
             RETURNING *"
        )
        .bind(Uuid::new_v4())
        .bind(username)
        .bind(email)
        .bind(password_hash)
        .bind(is_admin)
        .fetch_one(pool)
        .await
    }

    pub async fn count(pool: &PgPool) -> sqlx::Result<i64> {
        sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(pool)
            .await
    }

    pub async fn list(pool: &PgPool, page: i64, page_size: i64, search: Option<&str>) -> sqlx::Result<(i64, Vec<Self>)> {
        let offset = (page - 1) * page_size;
        match search.filter(|s| !s.is_empty()) {
            Some(q) => {
                let pattern = format!("%{q}%");
                let total: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM users WHERE username ILIKE $1 OR email ILIKE $1"
                )
                .bind(&pattern)
                .fetch_one(pool)
                .await?;
                let users = sqlx::query_as(
                    "SELECT * FROM users WHERE username ILIKE $1 OR email ILIKE $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
                )
                .bind(&pattern)
                .bind(page_size)
                .bind(offset)
                .fetch_all(pool)
                .await?;
                Ok((total, users))
            }
            None => {
                let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
                    .fetch_one(pool)
                    .await?;
                let users = sqlx::query_as(
                    "SELECT * FROM users ORDER BY created_at DESC LIMIT $1 OFFSET $2"
                )
                .bind(page_size)
                .bind(offset)
                .fetch_all(pool)
                .await?;
                Ok((total, users))
            }
        }
    }

    pub async fn delete(pool: &PgPool, id: Uuid) -> sqlx::Result<()> {
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }
}
