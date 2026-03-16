use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AbShare {
    pub id: Uuid,
    pub address_book_id: Uuid,
    pub user_id: Option<Uuid>,
    pub group_id: Option<Uuid>,
    pub rule: i16,
}

impl AbShare {
    pub async fn list_for_ab(pool: &PgPool, ab_id: Uuid) -> sqlx::Result<Vec<Self>> {
        sqlx::query_as("SELECT * FROM ab_shares WHERE address_book_id = $1")
            .bind(ab_id).fetch_all(pool).await
    }

    pub async fn share_with_user(pool: &PgPool, ab_id: Uuid, user_id: Uuid, rule: i16) -> sqlx::Result<()> {
        sqlx::query("INSERT INTO ab_shares (id, address_book_id, user_id, rule) VALUES ($1, $2, $3, $4) ON CONFLICT (address_book_id, user_id) DO UPDATE SET rule = $4")
            .bind(Uuid::new_v4()).bind(ab_id).bind(user_id).bind(rule).execute(pool).await?;
        Ok(())
    }

    pub async fn share_with_group(pool: &PgPool, ab_id: Uuid, group_id: Uuid, rule: i16) -> sqlx::Result<()> {
        sqlx::query("INSERT INTO ab_shares (id, address_book_id, group_id, rule) VALUES ($1, $2, $3, $4) ON CONFLICT (address_book_id, group_id) DO UPDATE SET rule = $4")
            .bind(Uuid::new_v4()).bind(ab_id).bind(group_id).bind(rule).execute(pool).await?;
        Ok(())
    }

    pub async fn remove(pool: &PgPool, id: Uuid) -> sqlx::Result<()> {
        sqlx::query("DELETE FROM ab_shares WHERE id = $1").bind(id).execute(pool).await?;
        Ok(())
    }

    pub async fn user_has_access(pool: &PgPool, ab_id: Uuid, user_id: Uuid, min_rule: i16) -> sqlx::Result<bool> {
        let has: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM ab_shares WHERE address_book_id = $1 AND user_id = $2 AND rule >= $3)")
            .bind(ab_id).bind(user_id).bind(min_rule).fetch_one(pool).await?;
        Ok(has)
    }
}
