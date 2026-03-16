use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AddressBook {
    pub id: Uuid,
    pub name: String,
    pub owner_id: Uuid,
    pub is_personal: bool,
    pub created_at: DateTime<Utc>,
}

impl AddressBook {
    pub async fn find_by_id(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<Self>> {
        sqlx::query_as("SELECT * FROM address_books WHERE id = $1")
            .bind(id).fetch_optional(pool).await
    }

    pub async fn find_personal(pool: &PgPool, owner_id: Uuid) -> sqlx::Result<Option<Self>> {
        sqlx::query_as("SELECT * FROM address_books WHERE owner_id = $1 AND is_personal = true")
            .bind(owner_id).fetch_optional(pool).await
    }

    pub async fn get_or_create_personal(pool: &PgPool, owner_id: Uuid) -> sqlx::Result<Self> {
        if let Some(ab) = Self::find_personal(pool, owner_id).await? {
            return Ok(ab);
        }
        sqlx::query_as(
            "INSERT INTO address_books (id, name, owner_id, is_personal) VALUES ($1, 'Personal', $2, true) RETURNING *"
        ).bind(Uuid::new_v4()).bind(owner_id).fetch_one(pool).await
    }

    pub async fn create(pool: &PgPool, name: &str, owner_id: Uuid) -> sqlx::Result<Self> {
        sqlx::query_as(
            "INSERT INTO address_books (id, name, owner_id, is_personal) VALUES ($1, $2, $3, false) RETURNING *"
        ).bind(Uuid::new_v4()).bind(name).bind(owner_id).fetch_one(pool).await
    }

    pub async fn list_shared_for_user(pool: &PgPool, user_id: Uuid, page: i64, page_size: i64) -> sqlx::Result<(i64, Vec<Self>)> {
        let offset = (page - 1) * page_size;
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM address_books ab WHERE ab.is_personal = false AND (ab.owner_id = $1 OR EXISTS (SELECT 1 FROM ab_shares s WHERE s.address_book_id = ab.id AND s.user_id = $1))"
        ).bind(user_id).fetch_one(pool).await?;
        let books = sqlx::query_as(
            "SELECT ab.* FROM address_books ab WHERE ab.is_personal = false AND (ab.owner_id = $1 OR EXISTS (SELECT 1 FROM ab_shares s WHERE s.address_book_id = ab.id AND s.user_id = $1)) ORDER BY ab.created_at DESC LIMIT $2 OFFSET $3"
        ).bind(user_id).bind(page_size).bind(offset).fetch_all(pool).await?;
        Ok((total, books))
    }

    pub async fn list_all(pool: &PgPool, page: i64, page_size: i64) -> sqlx::Result<(i64, Vec<Self>)> {
        let offset = (page - 1) * page_size;
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM address_books").fetch_one(pool).await?;
        let books = sqlx::query_as("SELECT * FROM address_books ORDER BY created_at DESC LIMIT $1 OFFSET $2")
            .bind(page_size).bind(offset).fetch_all(pool).await?;
        Ok((total, books))
    }

    pub async fn delete(pool: &PgPool, id: Uuid) -> sqlx::Result<()> {
        sqlx::query("DELETE FROM address_books WHERE id = $1").bind(id).execute(pool).await?;
        Ok(())
    }
}
