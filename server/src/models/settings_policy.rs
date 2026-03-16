use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SettingsPolicy {
    pub id: Uuid,
    pub name: String,
    pub target_type: String,
    pub target_id: Option<String>,
    pub config_options: serde_json::Value,
    pub extra: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl SettingsPolicy {
    pub async fn find_by_id(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<Self>> {
        sqlx::query_as("SELECT * FROM settings_policies WHERE id = $1").bind(id).fetch_optional(pool).await
    }

    pub async fn find_for_target(pool: &PgPool, target_type: &str, target_id: Option<&str>) -> sqlx::Result<Option<Self>> {
        match target_id {
            Some(tid) => sqlx::query_as("SELECT * FROM settings_policies WHERE target_type = $1 AND target_id = $2")
                .bind(target_type).bind(tid).fetch_optional(pool).await,
            None => sqlx::query_as("SELECT * FROM settings_policies WHERE target_type = $1 AND target_id IS NULL")
                .bind(target_type).fetch_optional(pool).await,
        }
    }

    pub async fn get_effective_for_device(pool: &PgPool, device_id: &str, group_id: Option<Uuid>) -> sqlx::Result<Option<serde_json::Value>> {
        let global = Self::find_for_target(pool, "global", None).await?;
        let group = match group_id {
            Some(gid) => Self::find_for_target(pool, "group", Some(&gid.to_string())).await?,
            None => None,
        };
        let device = Self::find_for_target(pool, "device", Some(device_id)).await?;

        let mut merged = serde_json::Map::new();
        for policy in [global, group, device].into_iter().flatten() {
            if let serde_json::Value::Object(map) = policy.config_options {
                for (k, v) in map { merged.insert(k, v); }
            }
        }
        if merged.is_empty() { Ok(None) } else { Ok(Some(serde_json::Value::Object(merged))) }
    }

    pub async fn upsert(pool: &PgPool, name: &str, target_type: &str, target_id: Option<&str>, config_options: &serde_json::Value, extra: Option<&serde_json::Value>) -> sqlx::Result<Self> {
        sqlx::query_as(
            "INSERT INTO settings_policies (id, name, target_type, target_id, config_options, extra) VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (target_type, COALESCE(target_id, '')) DO UPDATE SET name = $2, config_options = $5, extra = $6, updated_at = now()
             RETURNING *"
        ).bind(Uuid::new_v4()).bind(name).bind(target_type).bind(target_id).bind(config_options).bind(extra).fetch_one(pool).await
    }

    pub async fn save(pool: &PgPool, id: Uuid, name: &str, config_options: &serde_json::Value, extra: Option<&serde_json::Value>) -> sqlx::Result<()> {
        sqlx::query("UPDATE settings_policies SET name = $2, config_options = $3, extra = $4, updated_at = now() WHERE id = $1")
            .bind(id).bind(name).bind(config_options).bind(extra).execute(pool).await?;
        Ok(())
    }

    pub async fn list_all(pool: &PgPool) -> sqlx::Result<Vec<Self>> {
        sqlx::query_as("SELECT * FROM settings_policies ORDER BY target_type, name").fetch_all(pool).await
    }

    pub async fn delete(pool: &PgPool, id: Uuid) -> sqlx::Result<()> {
        sqlx::query("DELETE FROM settings_policies WHERE id = $1").bind(id).execute(pool).await?;
        Ok(())
    }
}
