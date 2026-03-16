use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Device {
    pub id: String,
    pub user_id: Option<Uuid>,
    pub device_group_id: Option<Uuid>,
    pub hostname: Option<String>,
    pub os: Option<String>,
    pub version: Option<String>,
    pub cpu: Option<String>,
    pub memory: Option<String>,
    pub ip: Option<String>,
    pub custom_id: Option<String>,
    pub note: Option<String>,
    pub status: i16,
    pub last_heartbeat: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Device {
    pub async fn upsert_from_heartbeat(pool: &PgPool, id: &str, ip: Option<&str>) -> sqlx::Result<()> {
        sqlx::query(
            "INSERT INTO devices (id, ip, status, last_heartbeat)
             VALUES ($1, $2, 1, now())
             ON CONFLICT (id) DO UPDATE SET
                ip = COALESCE($2, devices.ip),
                status = 1,
                last_heartbeat = now(),
                updated_at = now()"
        )
        .bind(id)
        .bind(ip)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn upsert_sysinfo(
        pool: &PgPool, id: &str, hostname: &str, os: &str, version: &str, cpu: &str, memory: &str,
    ) -> sqlx::Result<bool> {
        let result = sqlx::query(
            "INSERT INTO devices (id, hostname, os, version, cpu, memory, status)
             VALUES ($1, $2, $3, $4, $5, $6, 1)
             ON CONFLICT (id) DO UPDATE SET
                hostname = $2, os = $3, version = $4, cpu = $5, memory = $6, updated_at = now()"
        )
        .bind(id).bind(hostname).bind(os).bind(version).bind(cpu).bind(memory)
        .execute(pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn find_by_id(pool: &PgPool, id: &str) -> sqlx::Result<Option<Self>> {
        sqlx::query_as("SELECT * FROM devices WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await
    }

    pub async fn list(pool: &PgPool, page: i64, page_size: i64, status_filter: Option<i16>) -> sqlx::Result<(i64, Vec<Self>)> {
        let offset = (page - 1) * page_size;
        match status_filter {
            Some(s) => {
                let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE status = $1")
                    .bind(s).fetch_one(pool).await?;
                let devices = sqlx::query_as(
                    "SELECT * FROM devices WHERE status = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
                )
                .bind(s).bind(page_size).bind(offset).fetch_all(pool).await?;
                Ok((total, devices))
            }
            None => {
                let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM devices")
                    .fetch_one(pool).await?;
                let devices = sqlx::query_as(
                    "SELECT * FROM devices ORDER BY created_at DESC LIMIT $1 OFFSET $2"
                )
                .bind(page_size).bind(offset).fetch_all(pool).await?;
                Ok((total, devices))
            }
        }
    }

    pub async fn search(pool: &PgPool, query: &str, page: i64, page_size: i64) -> sqlx::Result<(i64, Vec<Self>)> {
        let pattern = format!("%{query}%");
        let offset = (page - 1) * page_size;
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM devices WHERE id ILIKE $1 OR hostname ILIKE $1 OR os ILIKE $1"
        )
        .bind(&pattern).fetch_one(pool).await?;
        let devices = sqlx::query_as(
            "SELECT * FROM devices WHERE id ILIKE $1 OR hostname ILIKE $1 OR os ILIKE $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
        )
        .bind(&pattern).bind(page_size).bind(offset).fetch_all(pool).await?;
        Ok((total, devices))
    }

    pub async fn mark_offline_stale(pool: &PgPool, stale_seconds: i64) -> sqlx::Result<u64> {
        let result = sqlx::query(
            "UPDATE devices SET status = 2, updated_at = now()
             WHERE status = 1 AND last_heartbeat < now() - ($1 || ' seconds')::interval"
        )
        .bind(stale_seconds.to_string())
        .execute(pool)
        .await?;
        Ok(result.rows_affected())
    }
}
