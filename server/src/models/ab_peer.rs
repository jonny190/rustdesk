use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AbPeer {
    pub id: Uuid,
    pub address_book_id: Uuid,
    pub peer_id: String,
    pub alias: Option<String>,
    pub tags: Option<Vec<String>>,
    pub note: Option<String>,
    #[serde(skip_serializing)]
    pub password_hash: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl AbPeer {
    pub async fn list(pool: &PgPool, ab_id: Uuid, page: i64, page_size: i64) -> sqlx::Result<(i64, Vec<Self>)> {
        let offset = (page - 1) * page_size;
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ab_peers WHERE address_book_id = $1")
            .bind(ab_id).fetch_one(pool).await?;
        let peers = sqlx::query_as("SELECT * FROM ab_peers WHERE address_book_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3")
            .bind(ab_id).bind(page_size).bind(offset).fetch_all(pool).await?;
        Ok((total, peers))
    }

    pub async fn add(pool: &PgPool, ab_id: Uuid, peer_id: &str, alias: Option<&str>, tags: Option<&[String]>, note: Option<&str>) -> sqlx::Result<Self> {
        sqlx::query_as(
            "INSERT INTO ab_peers (id, address_book_id, peer_id, alias, tags, note) VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (address_book_id, peer_id) DO UPDATE SET alias = COALESCE($4, ab_peers.alias), tags = COALESCE($5, ab_peers.tags), note = COALESCE($6, ab_peers.note)
             RETURNING *"
        ).bind(Uuid::new_v4()).bind(ab_id).bind(peer_id).bind(alias).bind(tags).bind(note).fetch_one(pool).await
    }

    pub async fn update(pool: &PgPool, ab_id: Uuid, peer_id: &str, alias: Option<&str>, tags: Option<&[String]>, note: Option<&str>) -> sqlx::Result<()> {
        sqlx::query("UPDATE ab_peers SET alias = COALESCE($3, alias), tags = COALESCE($4, tags), note = COALESCE($5, note) WHERE address_book_id = $1 AND peer_id = $2")
            .bind(ab_id).bind(peer_id).bind(alias).bind(tags).bind(note).execute(pool).await?;
        Ok(())
    }

    pub async fn delete(pool: &PgPool, ab_id: Uuid, peer_ids: &[String]) -> sqlx::Result<()> {
        sqlx::query("DELETE FROM ab_peers WHERE address_book_id = $1 AND peer_id = ANY($2)")
            .bind(ab_id).bind(peer_ids).execute(pool).await?;
        Ok(())
    }

    pub async fn list_all_for_ab(pool: &PgPool, ab_id: Uuid) -> sqlx::Result<Vec<Self>> {
        sqlx::query_as("SELECT * FROM ab_peers WHERE address_book_id = $1 ORDER BY created_at")
            .bind(ab_id).fetch_all(pool).await
    }
}
