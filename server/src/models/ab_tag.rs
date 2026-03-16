use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AbTag {
    pub id: Uuid,
    pub address_book_id: Uuid,
    pub name: String,
    pub color: Option<i32>,
}

impl AbTag {
    pub async fn list(pool: &PgPool, ab_id: Uuid) -> sqlx::Result<Vec<Self>> {
        sqlx::query_as("SELECT * FROM ab_tags WHERE address_book_id = $1 ORDER BY name")
            .bind(ab_id).fetch_all(pool).await
    }

    pub async fn add(pool: &PgPool, ab_id: Uuid, name: &str, color: Option<i32>) -> sqlx::Result<Self> {
        sqlx::query_as("INSERT INTO ab_tags (id, address_book_id, name, color) VALUES ($1, $2, $3, $4) RETURNING *")
            .bind(Uuid::new_v4()).bind(ab_id).bind(name).bind(color).fetch_one(pool).await
    }

    pub async fn rename(pool: &PgPool, ab_id: Uuid, old_name: &str, new_name: &str) -> sqlx::Result<()> {
        sqlx::query("UPDATE ab_tags SET name = $3 WHERE address_book_id = $1 AND name = $2")
            .bind(ab_id).bind(old_name).bind(new_name).execute(pool).await?;
        Ok(())
    }

    pub async fn update_color(pool: &PgPool, ab_id: Uuid, name: &str, color: i32) -> sqlx::Result<()> {
        sqlx::query("UPDATE ab_tags SET color = $3 WHERE address_book_id = $1 AND name = $2")
            .bind(ab_id).bind(name).bind(color).execute(pool).await?;
        Ok(())
    }

    pub async fn delete(pool: &PgPool, ab_id: Uuid, names: &[String]) -> sqlx::Result<()> {
        sqlx::query("DELETE FROM ab_tags WHERE address_book_id = $1 AND name = ANY($2)")
            .bind(ab_id).bind(names).execute(pool).await?;
        Ok(())
    }
}
