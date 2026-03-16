use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DeviceGroup {
    pub id: Uuid,
    pub name: String,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl DeviceGroup {
    pub async fn find_by_id(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<Self>> {
        sqlx::query_as("SELECT * FROM device_groups WHERE id = $1").bind(id).fetch_optional(pool).await
    }

    pub async fn create(pool: &PgPool, name: &str, note: Option<&str>) -> sqlx::Result<Self> {
        sqlx::query_as("INSERT INTO device_groups (id, name, note) VALUES ($1, $2, $3) RETURNING *")
            .bind(Uuid::new_v4()).bind(name).bind(note).fetch_one(pool).await
    }

    pub async fn update(pool: &PgPool, id: Uuid, name: &str, note: Option<&str>) -> sqlx::Result<()> {
        sqlx::query("UPDATE device_groups SET name = $2, note = $3, updated_at = now() WHERE id = $1")
            .bind(id).bind(name).bind(note).execute(pool).await?;
        Ok(())
    }

    pub async fn delete(pool: &PgPool, id: Uuid) -> sqlx::Result<()> {
        sqlx::query("DELETE FROM device_groups WHERE id = $1").bind(id).execute(pool).await?;
        Ok(())
    }

    pub async fn list(pool: &PgPool, page: i64, page_size: i64) -> sqlx::Result<(i64, Vec<Self>)> {
        let offset = (page - 1) * page_size;
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM device_groups").fetch_one(pool).await?;
        let groups = sqlx::query_as("SELECT * FROM device_groups ORDER BY name LIMIT $1 OFFSET $2")
            .bind(page_size).bind(offset).fetch_all(pool).await?;
        Ok((total, groups))
    }
}
