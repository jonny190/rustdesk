# Management Server Phase 2 Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add address book management, device groups, centralized settings/policy push, and change ID to the management server built in Phase 1.

**Architecture:** Extends the existing `server/` crate with new database tables (migration 002), new models, new API routes matching the RustDesk client expectations, and new console pages with HTMX. Four independent feature areas that share the same migration.

**Tech Stack:** Same as Phase 1: Rust 1.75+, Axum 0.8, SQLx 0.8, Askama 0.13, HTMX 2.0, Pico CSS, PostgreSQL 15+

**Spec:** `docs/superpowers/specs/2026-03-16-management-server-design.md`

**Depends on:** Phase 1 complete (branch `feat/management-server-phase1`)

---

## Chunk 1: Migration and Models

### File Structure for Chunk 1

```
server/src/
  db/migrations/
    002_phase2.sql                    (create)
  models/
    mod.rs                            (modify - add new exports)
    address_book.rs                   (create)
    ab_peer.rs                        (create)
    ab_tag.rs                         (create)
    ab_share.rs                       (create)
    device_group.rs                   (create)
    settings_policy.rs                (create)
```

---

### Task 1: Phase 2 database migration

**Files:**
- Create: `server/src/db/migrations/002_phase2.sql`
- Modify: `server/src/db/mod.rs`

- [ ] **Step 1: Create 002_phase2.sql**

```sql
-- Address books
CREATE TABLE address_books (
    id UUID PRIMARY KEY,
    name VARCHAR NOT NULL,
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    is_personal BOOLEAN DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_ab_owner ON address_books(owner_id);

CREATE TABLE ab_peers (
    id UUID PRIMARY KEY,
    address_book_id UUID NOT NULL REFERENCES address_books(id) ON DELETE CASCADE,
    peer_id VARCHAR NOT NULL,
    alias VARCHAR,
    tags TEXT[],
    note TEXT,
    password_hash VARCHAR,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(address_book_id, peer_id)
);

CREATE TABLE ab_tags (
    id UUID PRIMARY KEY,
    address_book_id UUID NOT NULL REFERENCES address_books(id) ON DELETE CASCADE,
    name VARCHAR NOT NULL,
    color INT,
    UNIQUE(address_book_id, name)
);

CREATE TABLE ab_shares (
    id UUID PRIMARY KEY,
    address_book_id UUID NOT NULL REFERENCES address_books(id) ON DELETE CASCADE,
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    group_id UUID REFERENCES device_groups(id) ON DELETE CASCADE,
    rule SMALLINT NOT NULL,
    UNIQUE(address_book_id, user_id),
    UNIQUE(address_book_id, group_id),
    CHECK (
        (user_id IS NOT NULL AND group_id IS NULL) OR
        (user_id IS NULL AND group_id IS NOT NULL)
    )
);

-- Centralized settings
CREATE TABLE settings_policies (
    id UUID PRIMARY KEY,
    name VARCHAR NOT NULL,
    target_type VARCHAR NOT NULL,
    target_id VARCHAR,
    config_options JSONB NOT NULL DEFAULT '{}',
    extra JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX idx_policies_target ON settings_policies(target_type, COALESCE(target_id, ''));
```

- [ ] **Step 2: Register migration in db/mod.rs**

Add to the `migrations` vec in `run_migrations`:

```rust
    let migrations = vec![
        ("001_initial", include_str!("migrations/001_initial.sql")),
        ("002_phase2", include_str!("migrations/002_phase2.sql")),
    ];
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p rustdesk-server-console`

- [ ] **Step 4: Commit**

```bash
git add server/src/db/
git commit -m "feat(server): add Phase 2 migration for address books and settings policies"
```

---

### Task 2: Address book model

**Files:**
- Create: `server/src/models/address_book.rs`

- [ ] **Step 1: Create address_book.rs**

```rust
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
            .bind(id)
            .fetch_optional(pool)
            .await
    }

    pub async fn find_personal(pool: &PgPool, owner_id: Uuid) -> sqlx::Result<Option<Self>> {
        sqlx::query_as("SELECT * FROM address_books WHERE owner_id = $1 AND is_personal = true")
            .bind(owner_id)
            .fetch_optional(pool)
            .await
    }

    pub async fn get_or_create_personal(pool: &PgPool, owner_id: Uuid) -> sqlx::Result<Self> {
        if let Some(ab) = Self::find_personal(pool, owner_id).await? {
            return Ok(ab);
        }
        sqlx::query_as(
            "INSERT INTO address_books (id, name, owner_id, is_personal)
             VALUES ($1, 'Personal', $2, true)
             RETURNING *"
        )
        .bind(Uuid::new_v4())
        .bind(owner_id)
        .fetch_one(pool)
        .await
    }

    pub async fn create(pool: &PgPool, name: &str, owner_id: Uuid) -> sqlx::Result<Self> {
        sqlx::query_as(
            "INSERT INTO address_books (id, name, owner_id, is_personal)
             VALUES ($1, $2, $3, false)
             RETURNING *"
        )
        .bind(Uuid::new_v4())
        .bind(name)
        .bind(owner_id)
        .fetch_one(pool)
        .await
    }

    /// List shared address books accessible to a user (owned or shared with them).
    pub async fn list_shared_for_user(
        pool: &PgPool,
        user_id: Uuid,
        page: i64,
        page_size: i64,
    ) -> sqlx::Result<(i64, Vec<Self>)> {
        let offset = (page - 1) * page_size;
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM address_books ab
             WHERE ab.is_personal = false AND (
                ab.owner_id = $1
                OR EXISTS (SELECT 1 FROM ab_shares s WHERE s.address_book_id = ab.id AND s.user_id = $1)
             )"
        )
        .bind(user_id)
        .fetch_one(pool)
        .await?;

        let books = sqlx::query_as(
            "SELECT ab.* FROM address_books ab
             WHERE ab.is_personal = false AND (
                ab.owner_id = $1
                OR EXISTS (SELECT 1 FROM ab_shares s WHERE s.address_book_id = ab.id AND s.user_id = $1)
             )
             ORDER BY ab.created_at DESC LIMIT $2 OFFSET $3"
        )
        .bind(user_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(pool)
        .await?;

        Ok((total, books))
    }

    /// List all address books (for admin console).
    pub async fn list_all(pool: &PgPool, page: i64, page_size: i64) -> sqlx::Result<(i64, Vec<Self>)> {
        let offset = (page - 1) * page_size;
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM address_books")
            .fetch_one(pool)
            .await?;
        let books = sqlx::query_as(
            "SELECT * FROM address_books ORDER BY created_at DESC LIMIT $1 OFFSET $2"
        )
        .bind(page_size)
        .bind(offset)
        .fetch_all(pool)
        .await?;
        Ok((total, books))
    }

    pub async fn delete(pool: &PgPool, id: Uuid) -> sqlx::Result<()> {
        sqlx::query("DELETE FROM address_books WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add server/src/models/address_book.rs
git commit -m "feat(server): add address book model"
```

---

### Task 3: Address book peer model

**Files:**
- Create: `server/src/models/ab_peer.rs`

- [ ] **Step 1: Create ab_peer.rs**

```rust
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
    pub async fn list(
        pool: &PgPool,
        ab_id: Uuid,
        page: i64,
        page_size: i64,
    ) -> sqlx::Result<(i64, Vec<Self>)> {
        let offset = (page - 1) * page_size;
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM ab_peers WHERE address_book_id = $1"
        )
        .bind(ab_id)
        .fetch_one(pool)
        .await?;

        let peers = sqlx::query_as(
            "SELECT * FROM ab_peers WHERE address_book_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
        )
        .bind(ab_id)
        .bind(page_size)
        .bind(offset)
        .fetch_all(pool)
        .await?;

        Ok((total, peers))
    }

    pub async fn add(
        pool: &PgPool,
        ab_id: Uuid,
        peer_id: &str,
        alias: Option<&str>,
        tags: Option<&[String]>,
        note: Option<&str>,
    ) -> sqlx::Result<Self> {
        sqlx::query_as(
            "INSERT INTO ab_peers (id, address_book_id, peer_id, alias, tags, note)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (address_book_id, peer_id) DO UPDATE SET
                alias = COALESCE($4, ab_peers.alias),
                tags = COALESCE($5, ab_peers.tags),
                note = COALESCE($6, ab_peers.note)
             RETURNING *"
        )
        .bind(Uuid::new_v4())
        .bind(ab_id)
        .bind(peer_id)
        .bind(alias)
        .bind(tags)
        .bind(note)
        .fetch_one(pool)
        .await
    }

    pub async fn update(
        pool: &PgPool,
        ab_id: Uuid,
        peer_id: &str,
        alias: Option<&str>,
        tags: Option<&[String]>,
        note: Option<&str>,
    ) -> sqlx::Result<()> {
        sqlx::query(
            "UPDATE ab_peers SET
                alias = COALESCE($3, alias),
                tags = COALESCE($4, tags),
                note = COALESCE($5, note)
             WHERE address_book_id = $1 AND peer_id = $2"
        )
        .bind(ab_id)
        .bind(peer_id)
        .bind(alias)
        .bind(tags)
        .bind(note)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn delete(pool: &PgPool, ab_id: Uuid, peer_ids: &[String]) -> sqlx::Result<()> {
        sqlx::query(
            "DELETE FROM ab_peers WHERE address_book_id = $1 AND peer_id = ANY($2)"
        )
        .bind(ab_id)
        .bind(peer_ids)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Fetch all peers for legacy personal AB (returns entire list, no pagination).
    pub async fn list_all_for_ab(pool: &PgPool, ab_id: Uuid) -> sqlx::Result<Vec<Self>> {
        sqlx::query_as("SELECT * FROM ab_peers WHERE address_book_id = $1 ORDER BY created_at")
            .bind(ab_id)
            .fetch_all(pool)
            .await
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add server/src/models/ab_peer.rs
git commit -m "feat(server): add address book peer model"
```

---

### Task 4: Address book tag model

**Files:**
- Create: `server/src/models/ab_tag.rs`

- [ ] **Step 1: Create ab_tag.rs**

```rust
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
            .bind(ab_id)
            .fetch_all(pool)
            .await
    }

    pub async fn add(pool: &PgPool, ab_id: Uuid, name: &str, color: Option<i32>) -> sqlx::Result<Self> {
        sqlx::query_as(
            "INSERT INTO ab_tags (id, address_book_id, name, color)
             VALUES ($1, $2, $3, $4)
             RETURNING *"
        )
        .bind(Uuid::new_v4())
        .bind(ab_id)
        .bind(name)
        .bind(color)
        .fetch_one(pool)
        .await
    }

    pub async fn rename(pool: &PgPool, ab_id: Uuid, old_name: &str, new_name: &str) -> sqlx::Result<()> {
        sqlx::query(
            "UPDATE ab_tags SET name = $3 WHERE address_book_id = $1 AND name = $2"
        )
        .bind(ab_id)
        .bind(old_name)
        .bind(new_name)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn update_color(pool: &PgPool, ab_id: Uuid, name: &str, color: i32) -> sqlx::Result<()> {
        sqlx::query(
            "UPDATE ab_tags SET color = $3 WHERE address_book_id = $1 AND name = $2"
        )
        .bind(ab_id)
        .bind(name)
        .bind(color)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn delete(pool: &PgPool, ab_id: Uuid, names: &[String]) -> sqlx::Result<()> {
        sqlx::query(
            "DELETE FROM ab_tags WHERE address_book_id = $1 AND name = ANY($2)"
        )
        .bind(ab_id)
        .bind(names)
        .execute(pool)
        .await?;
        Ok(())
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add server/src/models/ab_tag.rs
git commit -m "feat(server): add address book tag model"
```

---

### Task 5: Address book share model

**Files:**
- Create: `server/src/models/ab_share.rs`

- [ ] **Step 1: Create ab_share.rs**

```rust
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
            .bind(ab_id)
            .fetch_all(pool)
            .await
    }

    pub async fn share_with_user(pool: &PgPool, ab_id: Uuid, user_id: Uuid, rule: i16) -> sqlx::Result<()> {
        sqlx::query(
            "INSERT INTO ab_shares (id, address_book_id, user_id, rule)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (address_book_id, user_id) DO UPDATE SET rule = $4"
        )
        .bind(Uuid::new_v4())
        .bind(ab_id)
        .bind(user_id)
        .bind(rule)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn share_with_group(pool: &PgPool, ab_id: Uuid, group_id: Uuid, rule: i16) -> sqlx::Result<()> {
        sqlx::query(
            "INSERT INTO ab_shares (id, address_book_id, group_id, rule)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (address_book_id, group_id) DO UPDATE SET rule = $4"
        )
        .bind(Uuid::new_v4())
        .bind(ab_id)
        .bind(group_id)
        .bind(rule)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn remove(pool: &PgPool, id: Uuid) -> sqlx::Result<()> {
        sqlx::query("DELETE FROM ab_shares WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Check if a user has at least the given rule level on an address book.
    pub async fn user_has_access(pool: &PgPool, ab_id: Uuid, user_id: Uuid, min_rule: i16) -> sqlx::Result<bool> {
        let has: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                SELECT 1 FROM ab_shares WHERE address_book_id = $1 AND user_id = $2 AND rule >= $3
             )"
        )
        .bind(ab_id)
        .bind(user_id)
        .bind(min_rule)
        .fetch_one(pool)
        .await?;
        Ok(has)
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add server/src/models/ab_share.rs
git commit -m "feat(server): add address book share model"
```

---

### Task 6: Device group model

**Files:**
- Create: `server/src/models/device_group.rs`

- [ ] **Step 1: Create device_group.rs**

```rust
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
        sqlx::query_as("SELECT * FROM device_groups WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await
    }

    pub async fn create(pool: &PgPool, name: &str, note: Option<&str>) -> sqlx::Result<Self> {
        sqlx::query_as(
            "INSERT INTO device_groups (id, name, note) VALUES ($1, $2, $3) RETURNING *"
        )
        .bind(Uuid::new_v4())
        .bind(name)
        .bind(note)
        .fetch_one(pool)
        .await
    }

    pub async fn update(pool: &PgPool, id: Uuid, name: &str, note: Option<&str>) -> sqlx::Result<()> {
        sqlx::query("UPDATE device_groups SET name = $2, note = $3, updated_at = now() WHERE id = $1")
            .bind(id)
            .bind(name)
            .bind(note)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn delete(pool: &PgPool, id: Uuid) -> sqlx::Result<()> {
        sqlx::query("DELETE FROM device_groups WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn list(pool: &PgPool, page: i64, page_size: i64) -> sqlx::Result<(i64, Vec<Self>)> {
        let offset = (page - 1) * page_size;
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM device_groups")
            .fetch_one(pool)
            .await?;
        let groups = sqlx::query_as(
            "SELECT * FROM device_groups ORDER BY name LIMIT $1 OFFSET $2"
        )
        .bind(page_size)
        .bind(offset)
        .fetch_all(pool)
        .await?;
        Ok((total, groups))
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add server/src/models/device_group.rs
git commit -m "feat(server): add device group model"
```

---

### Task 7: Settings policy model

**Files:**
- Create: `server/src/models/settings_policy.rs`

- [ ] **Step 1: Create settings_policy.rs**

```rust
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
        sqlx::query_as("SELECT * FROM settings_policies WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await
    }

    pub async fn find_for_target(pool: &PgPool, target_type: &str, target_id: Option<&str>) -> sqlx::Result<Option<Self>> {
        match target_id {
            Some(tid) => {
                sqlx::query_as(
                    "SELECT * FROM settings_policies WHERE target_type = $1 AND target_id = $2"
                )
                .bind(target_type)
                .bind(tid)
                .fetch_optional(pool)
                .await
            }
            None => {
                sqlx::query_as(
                    "SELECT * FROM settings_policies WHERE target_type = $1 AND target_id IS NULL"
                )
                .bind(target_type)
                .fetch_optional(pool)
                .await
            }
        }
    }

    /// Get the effective policy for a device: device-specific > group > global, merged.
    pub async fn get_effective_for_device(
        pool: &PgPool,
        device_id: &str,
        group_id: Option<Uuid>,
    ) -> sqlx::Result<Option<serde_json::Value>> {
        let global = Self::find_for_target(pool, "global", None).await?;
        let group = match group_id {
            Some(gid) => Self::find_for_target(pool, "group", Some(&gid.to_string())).await?,
            None => None,
        };
        let device = Self::find_for_target(pool, "device", Some(device_id)).await?;

        // Merge: global < group < device (more specific wins)
        let mut merged = serde_json::Map::new();
        for policy in [global, group, device].into_iter().flatten() {
            if let serde_json::Value::Object(map) = policy.config_options {
                for (k, v) in map {
                    merged.insert(k, v);
                }
            }
        }

        if merged.is_empty() {
            Ok(None)
        } else {
            Ok(Some(serde_json::Value::Object(merged)))
        }
    }

    pub async fn upsert(
        pool: &PgPool,
        name: &str,
        target_type: &str,
        target_id: Option<&str>,
        config_options: &serde_json::Value,
        extra: Option<&serde_json::Value>,
    ) -> sqlx::Result<Self> {
        sqlx::query_as(
            "INSERT INTO settings_policies (id, name, target_type, target_id, config_options, extra)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (target_type, COALESCE(target_id, '')) DO UPDATE SET
                name = $2, config_options = $5, extra = $6, updated_at = now()
             RETURNING *"
        )
        .bind(Uuid::new_v4())
        .bind(name)
        .bind(target_type)
        .bind(target_id)
        .bind(config_options)
        .bind(extra)
        .fetch_one(pool)
        .await
    }

    pub async fn save(
        pool: &PgPool,
        id: Uuid,
        name: &str,
        config_options: &serde_json::Value,
        extra: Option<&serde_json::Value>,
    ) -> sqlx::Result<()> {
        sqlx::query(
            "UPDATE settings_policies SET name = $2, config_options = $3, extra = $4, updated_at = now() WHERE id = $1"
        )
        .bind(id)
        .bind(name)
        .bind(config_options)
        .bind(extra)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn list_all(pool: &PgPool) -> sqlx::Result<Vec<Self>> {
        sqlx::query_as("SELECT * FROM settings_policies ORDER BY target_type, name")
            .fetch_all(pool)
            .await
    }

    pub async fn delete(pool: &PgPool, id: Uuid) -> sqlx::Result<()> {
        sqlx::query("DELETE FROM settings_policies WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add server/src/models/settings_policy.rs
git commit -m "feat(server): add settings policy model with merge logic"
```

---

### Task 8: Update models/mod.rs with all new exports

**Files:**
- Modify: `server/src/models/mod.rs`

- [ ] **Step 1: Update mod.rs**

```rust
pub mod user;
pub mod session;
pub mod device;
pub mod address_book;
pub mod ab_peer;
pub mod ab_tag;
pub mod ab_share;
pub mod device_group;
pub mod settings_policy;

pub use user::{User, UserPayload};
pub use session::Session;
pub use device::Device;
pub use address_book::AddressBook;
pub use ab_peer::AbPeer;
pub use ab_tag::AbTag;
pub use ab_share::AbShare;
pub use device_group::DeviceGroup;
pub use settings_policy::SettingsPolicy;
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p rustdesk-server-console`

- [ ] **Step 3: Commit**

```bash
git add server/src/models/mod.rs
git commit -m "feat(server): export all Phase 2 models"
```

---

## Chunk 2: API Routes

### File Structure for Chunk 2

```
server/src/routes/
  mod.rs              (modify)
  ab.rs               (create)
  groups.rs           (create)
  settings.rs         (create)
  devices.rs          (modify - add heartbeat strategy push and change ID)
```

---

### Task 9: Address book API routes

**Files:**
- Create: `server/src/routes/ab.rs`

- [ ] **Step 1: Create ab.rs**

This implements all `/api/ab/*` endpoints the RustDesk client calls. Note: many "read" operations use POST (client convention).

```rust
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::app::AppState;
use crate::middleware::AuthUser;
use crate::models::{AbPeer, AbShare, AbTag, AddressBook};

#[derive(Deserialize)]
pub struct PaginationParams {
    pub current: Option<i64>,
    #[serde(rename = "pageSize")]
    pub page_size: Option<i64>,
    pub ab: Option<Uuid>,
}

#[derive(Serialize)]
pub struct PaginatedResponse<T: Serialize> {
    pub total: i64,
    pub data: Vec<T>,
}

#[derive(Serialize)]
pub struct AbSettingsResponse {
    pub max_peers: i64,
}

#[derive(Serialize)]
pub struct AbPersonalResponse {
    pub guid: String,
}

#[derive(Serialize)]
pub struct AbProfile {
    pub guid: String,
    pub name: String,
    pub owner: String,
    pub rule: i16,
}

#[derive(Deserialize)]
pub struct AddPeerRequest {
    pub id: String,
    pub alias: Option<String>,
    pub tags: Option<Vec<String>>,
    pub note: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdatePeerRequest {
    pub id: Option<String>,
    pub alias: Option<String>,
    pub tags: Option<Vec<String>>,
    pub note: Option<String>,
}

#[derive(Deserialize)]
pub struct AddTagRequest {
    pub name: String,
    pub color: Option<i32>,
}

#[derive(Deserialize)]
pub struct RenameTagRequest {
    pub old: String,
    pub new: String,
}

#[derive(Deserialize)]
pub struct UpdateTagRequest {
    pub name: String,
    pub color: i32,
}

#[derive(Deserialize)]
pub struct DeleteRequest {
    pub ids: Option<Vec<String>>,
}

/// Legacy personal AB: serialized as JSON blob.
#[derive(Serialize, Deserialize)]
pub struct LegacyAb {
    pub peers: Vec<serde_json::Value>,
    pub tags: Vec<String>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        // Legacy personal AB
        .route("/ab", get(get_legacy_ab).post(push_legacy_ab))
        // Settings and personal
        .route("/ab/settings", post(ab_settings))
        .route("/ab/personal", post(ab_personal))
        // Shared profiles
        .route("/ab/shared/profiles", post(shared_profiles))
        // Peers
        .route("/ab/peers", post(list_peers))
        .route("/ab/peer/add/{guid}", post(add_peer))
        .route("/ab/peer/update/{guid}", put(update_peer))
        .route("/ab/peer/{guid}", delete(delete_peers))
        // Tags
        .route("/ab/tags/{guid}", post(list_tags))
        .route("/ab/tag/add/{guid}", post(add_tag))
        .route("/ab/tag/rename/{guid}", put(rename_tag))
        .route("/ab/tag/update/{guid}", put(update_tag))
        .route("/ab/tag/{guid}", delete(delete_tags))
}

async fn ab_settings() -> Json<AbSettingsResponse> {
    Json(AbSettingsResponse { max_peers: 1000 })
}

async fn ab_personal(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
) -> Result<Json<AbPersonalResponse>, StatusCode> {
    let ab = AddressBook::get_or_create_personal(&state.pool, auth.user.id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(AbPersonalResponse { guid: ab.id.to_string() }))
}

async fn shared_profiles(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<AbProfile>>, StatusCode> {
    let page = params.current.unwrap_or(1).max(1);
    let page_size = params.page_size.unwrap_or(100).min(500);

    let (total, books) = AddressBook::list_shared_for_user(&state.pool, auth.user.id, page, page_size)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let data = books.iter().map(|b| AbProfile {
        guid: b.id.to_string(),
        name: b.name.clone(),
        owner: b.owner_id.to_string(),
        rule: 3, // TODO: look up actual share rule for this user
    }).collect();

    Ok(Json(PaginatedResponse { total, data }))
}

async fn list_peers(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<AbPeer>>, StatusCode> {
    let ab_id = params.ab.ok_or(StatusCode::BAD_REQUEST)?;
    let page = params.current.unwrap_or(1).max(1);
    let page_size = params.page_size.unwrap_or(100).min(500);

    let (total, peers) = AbPeer::list(&state.pool, ab_id, page, page_size)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(PaginatedResponse { total, data: peers }))
}

async fn add_peer(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(guid): Path<Uuid>,
    Json(req): Json<AddPeerRequest>,
) -> Result<StatusCode, StatusCode> {
    AbPeer::add(
        &state.pool, guid, &req.id,
        req.alias.as_deref(),
        req.tags.as_deref(),
        req.note.as_deref(),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

async fn update_peer(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(guid): Path<Uuid>,
    Json(req): Json<UpdatePeerRequest>,
) -> Result<StatusCode, StatusCode> {
    let peer_id = req.id.as_deref().ok_or(StatusCode::BAD_REQUEST)?;
    AbPeer::update(
        &state.pool, guid, peer_id,
        req.alias.as_deref(),
        req.tags.as_deref(),
        req.note.as_deref(),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

async fn delete_peers(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(guid): Path<Uuid>,
    Json(req): Json<DeleteRequest>,
) -> Result<StatusCode, StatusCode> {
    let ids = req.ids.unwrap_or_default();
    AbPeer::delete(&state.pool, guid, &ids)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

async fn list_tags(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(guid): Path<Uuid>,
) -> Result<Json<Vec<AbTag>>, StatusCode> {
    let tags = AbTag::list(&state.pool, guid)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(tags))
}

async fn add_tag(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(guid): Path<Uuid>,
    Json(req): Json<AddTagRequest>,
) -> Result<StatusCode, StatusCode> {
    AbTag::add(&state.pool, guid, &req.name, req.color)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

async fn rename_tag(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(guid): Path<Uuid>,
    Json(req): Json<RenameTagRequest>,
) -> Result<StatusCode, StatusCode> {
    AbTag::rename(&state.pool, guid, &req.old, &req.new)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

async fn update_tag(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(guid): Path<Uuid>,
    Json(req): Json<UpdateTagRequest>,
) -> Result<StatusCode, StatusCode> {
    AbTag::update_color(&state.pool, guid, &req.name, req.color)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

async fn delete_tags(
    State(state): State<AppState>,
    Extension(_auth): Extension<AuthUser>,
    Path(guid): Path<Uuid>,
    Json(req): Json<DeleteRequest>,
) -> Result<StatusCode, StatusCode> {
    let names = req.ids.unwrap_or_default();
    AbTag::delete(&state.pool, guid, &names)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

async fn get_legacy_ab(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
) -> Result<Json<LegacyAb>, StatusCode> {
    let ab = AddressBook::get_or_create_personal(&state.pool, auth.user.id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let peers = AbPeer::list_all_for_ab(&state.pool, ab.id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let tags = AbTag::list(&state.pool, ab.id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let peer_values: Vec<serde_json::Value> = peers.iter().map(|p| {
        serde_json::json!({
            "id": p.peer_id,
            "alias": p.alias,
            "tags": p.tags,
        })
    }).collect();

    let tag_names: Vec<String> = tags.into_iter().map(|t| t.name).collect();

    Ok(Json(LegacyAb { peers: peer_values, tags: tag_names }))
}

async fn push_legacy_ab(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Json(data): Json<LegacyAb>,
) -> Result<StatusCode, StatusCode> {
    let ab = AddressBook::get_or_create_personal(&state.pool, auth.user.id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Replace all peers
    sqlx::query("DELETE FROM ab_peers WHERE address_book_id = $1")
        .bind(ab.id)
        .execute(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    for peer in &data.peers {
        let peer_id = peer.get("id").and_then(|v| v.as_str()).unwrap_or_default();
        let alias = peer.get("alias").and_then(|v| v.as_str());
        let tags: Option<Vec<String>> = peer.get("tags").and_then(|v| {
            v.as_array().map(|arr| arr.iter().filter_map(|t| t.as_str().map(String::from)).collect())
        });
        let _ = AbPeer::add(&state.pool, ab.id, peer_id, alias, tags.as_deref(), None).await;
    }

    // Replace all tags
    sqlx::query("DELETE FROM ab_tags WHERE address_book_id = $1")
        .bind(ab.id)
        .execute(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    for tag_name in &data.tags {
        let _ = AbTag::add(&state.pool, ab.id, tag_name, None).await;
    }

    Ok(StatusCode::OK)
}
```

- [ ] **Step 2: Commit**

```bash
git add server/src/routes/ab.rs
git commit -m "feat(server): add address book API routes"
```

---

### Task 10: Device group API routes

**Files:**
- Create: `server/src/routes/groups.rs`

- [ ] **Step 1: Create groups.rs**

```rust
use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::app::AppState;
use crate::models::DeviceGroup;

#[derive(Deserialize)]
pub struct PaginationParams {
    pub current: Option<i64>,
    #[serde(rename = "pageSize")]
    pub page_size: Option<i64>,
}

#[derive(Serialize)]
pub struct PaginatedResponse<T: Serialize> {
    pub total: i64,
    pub data: Vec<T>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/device-group/accessible", get(list_groups))
}

async fn list_groups(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<DeviceGroup>>, StatusCode> {
    let page = params.current.unwrap_or(1).max(1);
    let page_size = params.page_size.unwrap_or(100).min(500);

    let (total, groups) = DeviceGroup::list(&state.pool, page, page_size)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(PaginatedResponse { total, data: groups }))
}
```

- [ ] **Step 2: Commit**

```bash
git add server/src/routes/groups.rs
git commit -m "feat(server): add device group API routes"
```

---

### Task 11: Enhance heartbeat with strategy push and add change ID / devices/cli

**Files:**
- Modify: `server/src/routes/devices.rs`

- [ ] **Step 1: Update heartbeat handler to include strategy**

In `server/src/routes/devices.rs`, update the `heartbeat` function to look up the device's group and fetch effective policy.

Add at the top of the file:
```rust
use crate::models::{Device, SettingsPolicy};
```

Replace the `heartbeat` function body:
```rust
async fn heartbeat(
    State(state): State<AppState>,
    Json(req): Json<HeartbeatRequest>,
) -> Result<Json<HeartbeatResponse>, StatusCode> {
    let device_id = req.id.as_deref().ok_or(StatusCode::BAD_REQUEST)?;

    Device::upsert_from_heartbeat(&state.pool, device_id, None)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let device = Device::find_by_id(&state.pool, device_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let needs_sysinfo = device
        .as_ref()
        .map(|d| d.hostname.is_none())
        .unwrap_or(true);

    // Fetch effective strategy for this device
    let group_id = device.as_ref().and_then(|d| d.device_group_id);
    let strategy = SettingsPolicy::get_effective_for_device(&state.pool, device_id, group_id)
        .await
        .unwrap_or(None);

    let strategy_response = strategy.map(|config_options| {
        serde_json::json!({
            "config_options": config_options,
            "extra": {}
        })
    });

    Ok(Json(HeartbeatResponse {
        sysinfo: if needs_sysinfo { Some(true) } else { None },
        strategy: strategy_response,
        ..Default::default()
    }))
}
```

- [ ] **Step 2: Add /api/devices/cli endpoint for change ID**

Add these structs and handlers to `devices.rs`:

```rust
#[derive(Deserialize)]
pub struct DeviceCliRequest {
    pub id: Option<String>,
    pub action: Option<String>,
    pub custom_id: Option<String>,
}

#[derive(Serialize)]
pub struct DeviceCliResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
```

Add to the `public_routes` function:
```rust
        .route("/devices/cli", post(devices_cli))
```

Add the handler:
```rust
async fn devices_cli(
    State(state): State<AppState>,
    Json(req): Json<DeviceCliRequest>,
) -> Result<Json<DeviceCliResponse>, StatusCode> {
    let device_id = req.id.as_deref().ok_or(StatusCode::BAD_REQUEST)?;

    if let Some(custom_id) = &req.custom_id {
        // Change ID operation
        if custom_id.is_empty() {
            // Clear custom ID
            sqlx::query("UPDATE devices SET custom_id = NULL, updated_at = now() WHERE id = $1")
                .bind(device_id)
                .execute(&state.pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        } else {
            // Check if custom_id is already taken
            let existing: Option<(String,)> = sqlx::query_as(
                "SELECT id FROM devices WHERE custom_id = $1 AND id != $2"
            )
            .bind(custom_id)
            .bind(device_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            if existing.is_some() {
                return Ok(Json(DeviceCliResponse {
                    error: Some("ID already taken".into()),
                }));
            }

            sqlx::query("UPDATE devices SET custom_id = $1, updated_at = now() WHERE id = $2")
                .bind(custom_id)
                .bind(device_id)
                .execute(&state.pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }

    Ok(Json(DeviceCliResponse { error: None }))
}
```

- [ ] **Step 3: Commit**

```bash
git add server/src/routes/devices.rs
git commit -m "feat(server): add strategy push to heartbeat and devices/cli for change ID"
```

---

### Task 12: Update routes/mod.rs to register new route modules

**Files:**
- Modify: `server/src/routes/mod.rs`

- [ ] **Step 1: Update mod.rs**

```rust
pub mod auth;
pub mod console;
pub mod devices;
pub mod users;
pub mod ab;
pub mod groups;

use axum::Router;
use crate::app::AppState;

pub fn public_api_routes() -> Router<AppState> {
    Router::new()
        .merge(auth::public_routes())
        .merge(devices::public_routes())
}

pub fn authed_api_routes() -> Router<AppState> {
    Router::new()
        .merge(auth::authed_routes())
        .merge(devices::routes())
        .merge(users::routes())
        .merge(ab::routes())
        .merge(groups::routes())
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p rustdesk-server-console`

- [ ] **Step 3: Commit**

```bash
git add server/src/routes/mod.rs
git commit -m "feat(server): register address book and group API routes"
```

---

## Chunk 3: Console Pages and Settings

### File Structure for Chunk 3

```
server/src/
  templates/
    groups/
      list.html           (create)
      detail.html         (create)
    address_books/
      list.html           (create)
      detail.html         (create)
    settings/
      global.html         (create)
  routes/
    console.rs            (modify - add group, AB, and settings page handlers)
```

---

### Task 13: Group and address book templates

**Files:**
- Create: `server/src/templates/groups/list.html`
- Create: `server/src/templates/groups/detail.html`
- Create: `server/src/templates/address_books/list.html`
- Create: `server/src/templates/address_books/detail.html`
- Create: `server/src/templates/settings/global.html`

- [ ] **Step 1: Create groups/list.html**

```html
{% extends "layout.html" %}
{% block title %}Groups - RustDesk Console{% endblock %}
{% block heading %}Device Groups{% endblock %}
{% block content %}
<div style="display: flex; justify-content: space-between; margin-bottom: 1rem;">
    <input type="search" placeholder="Search groups..."
           hx-get="/console/groups/table" hx-trigger="keyup changed delay:300ms"
           hx-target="#group-table" name="search" style="max-width: 300px;">
    <button hx-get="/console/groups/new-form" hx-target="#new-group-form" hx-swap="innerHTML">Add Group</button>
</div>
<div id="new-group-form"></div>
<table>
    <thead><tr><th>Name</th><th>Note</th><th>Created</th><th>Actions</th></tr></thead>
    <tbody id="group-table" hx-get="/console/groups/table" hx-trigger="load" hx-swap="innerHTML"></tbody>
</table>
{% endblock %}
```

- [ ] **Step 2: Create groups/detail.html**

```html
{% extends "layout.html" %}
{% block title %}Group: {{ group.name }} - RustDesk Console{% endblock %}
{% block heading %}Group: {{ group.name }}{% endblock %}
{% block content %}
<form method="POST" action="/console/groups/{{ group.id }}">
    <label for="name">Name</label>
    <input type="text" id="name" name="name" value="{{ group.name }}" required>
    <label for="note">Note</label>
    <textarea id="note" name="note">{{ group.note.as_deref().unwrap_or_default() }}</textarea>
    <div style="display: flex; gap: 1rem;">
        <button type="submit">Save</button>
        <button type="button" class="secondary"
                hx-post="/console/groups/{{ group.id }}/delete"
                hx-confirm="Delete this group?"
                hx-target="body">Delete</button>
    </div>
</form>
<h3>Devices in Group</h3>
<div hx-get="/console/groups/{{ group.id }}/devices" hx-trigger="load" hx-swap="innerHTML">Loading...</div>
{% endblock %}
```

- [ ] **Step 3: Create address_books/list.html**

```html
{% extends "layout.html" %}
{% block title %}Address Books - RustDesk Console{% endblock %}
{% block heading %}Address Books{% endblock %}
{% block content %}
<table>
    <thead><tr><th>Name</th><th>Owner</th><th>Personal</th><th>Created</th><th>Actions</th></tr></thead>
    <tbody id="ab-table" hx-get="/console/address-books/table" hx-trigger="load" hx-swap="innerHTML"></tbody>
</table>
{% endblock %}
```

- [ ] **Step 4: Create address_books/detail.html**

```html
{% extends "layout.html" %}
{% block title %}Address Book: {{ ab.name }} - RustDesk Console{% endblock %}
{% block heading %}Address Book: {{ ab.name }}{% endblock %}
{% block content %}
<h3>Peers</h3>
<table>
    <thead><tr><th>Peer ID</th><th>Alias</th><th>Tags</th><th>Note</th></tr></thead>
    <tbody hx-get="/console/address-books/{{ ab.id }}/peers" hx-trigger="load" hx-swap="innerHTML"></tbody>
</table>
<h3>Tags</h3>
<div hx-get="/console/address-books/{{ ab.id }}/tags" hx-trigger="load" hx-swap="innerHTML">Loading...</div>
<h3>Sharing</h3>
<div hx-get="/console/address-books/{{ ab.id }}/shares" hx-trigger="load" hx-swap="innerHTML">Loading...</div>
{% endblock %}
```

- [ ] **Step 5: Create settings/global.html**

```html
{% extends "layout.html" %}
{% block title %}Settings - RustDesk Console{% endblock %}
{% block heading %}Centralized Settings{% endblock %}
{% block content %}
<h3>Global Policy</h3>
<form method="POST" action="/console/settings">
    <input type="hidden" name="target_type" value="global">
    <label for="name">Policy Name</label>
    <input type="text" id="name" name="name" value="{{ name }}" required>
    <label for="config_options">Config Options (JSON)</label>
    <textarea id="config_options" name="config_options" rows="10" style="font-family: monospace;">{{ config_options }}</textarea>
    <button type="submit">Save</button>
</form>
<h3>Group Policies</h3>
<div hx-get="/console/settings/group-policies" hx-trigger="load" hx-swap="innerHTML">Loading...</div>
{% endblock %}
```

- [ ] **Step 6: Commit**

```bash
git add server/src/templates/groups/ server/src/templates/address_books/ server/src/templates/settings/
git commit -m "feat(server): add templates for groups, address books, and settings pages"
```

---

### Task 14: Add console handlers for groups, address books, and settings

**Files:**
- Modify: `server/src/routes/console.rs`

- [ ] **Step 1: Add template structs and imports at the top of console.rs**

Add these template structs alongside the existing ones:

```rust
use crate::models::{AddressBook, AbPeer, AbTag, AbShare, DeviceGroup, SettingsPolicy};

#[derive(Template)]
#[template(path = "groups/list.html")]
struct GroupsListTemplate {}

#[derive(Template)]
#[template(path = "groups/detail.html")]
struct GroupDetailTemplate {
    group: DeviceGroup,
}

#[derive(Template)]
#[template(path = "address_books/list.html")]
struct AbListTemplate {}

#[derive(Template)]
#[template(path = "address_books/detail.html")]
struct AbDetailTemplate {
    ab: AddressBook,
}

#[derive(Template)]
#[template(path = "settings/global.html")]
struct SettingsTemplate {
    name: String,
    config_options: String,
}

#[derive(Deserialize)]
pub struct GroupForm {
    pub name: String,
    pub note: Option<String>,
}

#[derive(Deserialize)]
pub struct SettingsForm {
    pub target_type: String,
    pub name: String,
    pub config_options: String,
}
```

- [ ] **Step 2: Add routes to authed_routes()**

Add these to the `authed_routes` function's Router:

```rust
        .route("/console/groups", get(groups_list))
        .route("/console/groups/table", get(groups_table_fragment))
        .route("/console/groups/new-form", get(group_new_form))
        .route("/console/groups/new", post(group_create))
        .route("/console/groups/{id}", get(group_detail).post(group_update))
        .route("/console/groups/{id}/delete", post(group_delete))
        .route("/console/groups/{id}/devices", get(group_devices_fragment))
        .route("/console/address-books", get(ab_list))
        .route("/console/address-books/table", get(ab_table_fragment))
        .route("/console/address-books/{guid}", get(ab_detail))
        .route("/console/address-books/{guid}/peers", get(ab_peers_fragment))
        .route("/console/address-books/{guid}/tags", get(ab_tags_fragment))
        .route("/console/address-books/{guid}/shares", get(ab_shares_fragment))
        .route("/console/settings", get(settings_page).post(settings_save))
        .route("/console/settings/group-policies", get(settings_group_policies_fragment))
```

- [ ] **Step 3: Add handler functions**

```rust
// -- Group handlers --

async fn groups_list() -> Html<String> {
    Html(GroupsListTemplate {}.render().unwrap_or_default())
}

async fn groups_table_fragment(State(state): State<AppState>) -> Html<String> {
    let (_, groups) = DeviceGroup::list(&state.pool, 1, 100).await.unwrap_or_default();
    let mut html = String::new();
    for g in &groups {
        html.push_str(&format!(
            "<tr><td><a href=\"/console/groups/{}\">{}</a></td><td>{}</td><td>{}</td><td><a href=\"/console/groups/{}\">Edit</a></td></tr>",
            g.id, g.name, g.note.as_deref().unwrap_or("-"), g.created_at.format("%Y-%m-%d"), g.id
        ));
    }
    if groups.is_empty() {
        html.push_str("<tr><td colspan=\"4\">No groups</td></tr>");
    }
    Html(html)
}

async fn group_new_form() -> Html<String> {
    Html(r#"<form method="POST" action="/console/groups/new" style="margin-bottom: 1rem;">
        <div style="display: flex; gap: 0.5rem; align-items: end;">
            <input type="text" name="name" placeholder="Group name" required style="margin-bottom: 0;">
            <input type="text" name="note" placeholder="Note (optional)" style="margin-bottom: 0;">
            <button type="submit">Create</button>
        </div>
    </form>"#.to_string())
}

async fn group_create(State(state): State<AppState>, Form(form): Form<GroupForm>) -> Response {
    let _ = DeviceGroup::create(&state.pool, &form.name, form.note.as_deref()).await;
    Redirect::to("/console/groups").into_response()
}

async fn group_detail(State(state): State<AppState>, Path(id): Path<Uuid>) -> Result<Html<String>, StatusCode> {
    let group = DeviceGroup::find_by_id(&state.pool, id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Html(GroupDetailTemplate { group }.render().unwrap_or_default()))
}

async fn group_update(State(state): State<AppState>, Path(id): Path<Uuid>, Form(form): Form<GroupForm>) -> Response {
    let _ = DeviceGroup::update(&state.pool, id, &form.name, form.note.as_deref()).await;
    Redirect::to(&format!("/console/groups/{id}")).into_response()
}

async fn group_delete(State(state): State<AppState>, Path(id): Path<Uuid>) -> Response {
    let _ = DeviceGroup::delete(&state.pool, id).await;
    Redirect::to("/console/groups").into_response()
}

async fn group_devices_fragment(State(state): State<AppState>, Path(id): Path<Uuid>) -> Html<String> {
    let devices: Vec<Device> = sqlx::query_as("SELECT * FROM devices WHERE device_group_id = $1 ORDER BY id")
        .bind(id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();

    let mut html = String::from("<table><thead><tr><th>ID</th><th>Hostname</th><th>Status</th></tr></thead><tbody>");
    for d in &devices {
        let status = match d.status { 1 => "Online", 0 => "Disabled", _ => "Offline" };
        html.push_str(&format!(
            "<tr><td><a href=\"/console/devices/{}\">{}</a></td><td>{}</td><td>{}</td></tr>",
            d.id, d.id, d.hostname.as_deref().unwrap_or("-"), status
        ));
    }
    if devices.is_empty() {
        html.push_str("<tr><td colspan=\"3\">No devices in this group</td></tr>");
    }
    html.push_str("</tbody></table>");
    Html(html)
}

// -- Address book console handlers --

async fn ab_list() -> Html<String> {
    Html(AbListTemplate {}.render().unwrap_or_default())
}

async fn ab_table_fragment(State(state): State<AppState>) -> Html<String> {
    let (_, books) = AddressBook::list_all(&state.pool, 1, 100).await.unwrap_or_default();
    let mut html = String::new();
    for b in &books {
        let personal = if b.is_personal { "Yes" } else { "No" };
        html.push_str(&format!(
            "<tr><td><a href=\"/console/address-books/{}\">{}</a></td><td>{}</td><td>{}</td><td>{}</td><td><a href=\"/console/address-books/{}\">View</a></td></tr>",
            b.id, b.name, b.owner_id, personal, b.created_at.format("%Y-%m-%d"), b.id
        ));
    }
    if books.is_empty() {
        html.push_str("<tr><td colspan=\"5\">No address books</td></tr>");
    }
    Html(html)
}

async fn ab_detail(State(state): State<AppState>, Path(guid): Path<Uuid>) -> Result<Html<String>, StatusCode> {
    let ab = AddressBook::find_by_id(&state.pool, guid).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Html(AbDetailTemplate { ab }.render().unwrap_or_default()))
}

async fn ab_peers_fragment(State(state): State<AppState>, Path(guid): Path<Uuid>) -> Html<String> {
    let (_, peers) = AbPeer::list(&state.pool, guid, 1, 200).await.unwrap_or_default();
    let mut html = String::new();
    for p in &peers {
        let tags_str = p.tags.as_ref().map(|t| t.join(", ")).unwrap_or_default();
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            p.peer_id, p.alias.as_deref().unwrap_or("-"), tags_str, p.note.as_deref().unwrap_or("-")
        ));
    }
    if peers.is_empty() {
        html.push_str("<tr><td colspan=\"4\">No peers</td></tr>");
    }
    Html(html)
}

async fn ab_tags_fragment(State(state): State<AppState>, Path(guid): Path<Uuid>) -> Html<String> {
    let tags = AbTag::list(&state.pool, guid).await.unwrap_or_default();
    let mut html = String::new();
    for t in &tags {
        html.push_str(&format!("<span style=\"margin-right: 0.5rem; padding: 0.2rem 0.5rem; background: var(--pico-card-background-color); border-radius: 4px;\">{}</span>", t.name));
    }
    if tags.is_empty() {
        html.push_str("No tags");
    }
    Html(html)
}

async fn ab_shares_fragment(State(state): State<AppState>, Path(guid): Path<Uuid>) -> Html<String> {
    let shares = AbShare::list_for_ab(&state.pool, guid).await.unwrap_or_default();
    let mut html = String::from("<table><thead><tr><th>Shared With</th><th>Rule</th></tr></thead><tbody>");
    for s in &shares {
        let who = s.user_id.map(|u| format!("User: {u}"))
            .or_else(|| s.group_id.map(|g| format!("Group: {g}")))
            .unwrap_or_else(|| "-".into());
        let rule = match s.rule { 1 => "Read", 2 => "Read/Write", 3 => "Full Control", _ => "Unknown" };
        html.push_str(&format!("<tr><td>{}</td><td>{}</td></tr>", who, rule));
    }
    if shares.is_empty() {
        html.push_str("<tr><td colspan=\"2\">Not shared</td></tr>");
    }
    html.push_str("</tbody></table>");
    Html(html)
}

// -- Settings console handlers --

async fn settings_page(State(state): State<AppState>) -> Html<String> {
    let global = SettingsPolicy::find_for_target(&state.pool, "global", None).await.ok().flatten();
    let name = global.as_ref().map(|p| p.name.clone()).unwrap_or_else(|| "Global Policy".into());
    let config_options = global
        .as_ref()
        .map(|p| serde_json::to_string_pretty(&p.config_options).unwrap_or_default())
        .unwrap_or_else(|| "{}".into());
    Html(SettingsTemplate { name, config_options }.render().unwrap_or_default())
}

async fn settings_save(State(state): State<AppState>, Form(form): Form<SettingsForm>) -> Response {
    let config: serde_json::Value = serde_json::from_str(&form.config_options)
        .unwrap_or(serde_json::json!({}));

    let existing = SettingsPolicy::find_for_target(&state.pool, &form.target_type, None).await.ok().flatten();
    match existing {
        Some(policy) => {
            let _ = SettingsPolicy::save(&state.pool, policy.id, &form.name, &config, None).await;
        }
        None => {
            let _ = SettingsPolicy::upsert(&state.pool, &form.name, &form.target_type, None, &config, None).await;
        }
    }
    Redirect::to("/console/settings").into_response()
}

async fn settings_group_policies_fragment(State(state): State<AppState>) -> Html<String> {
    let policies = SettingsPolicy::list_all(&state.pool).await.unwrap_or_default();
    let group_policies: Vec<_> = policies.iter().filter(|p| p.target_type == "group").collect();

    let mut html = String::from("<table><thead><tr><th>Name</th><th>Target Group</th><th>Updated</th></tr></thead><tbody>");
    for p in &group_policies {
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td></tr>",
            p.name, p.target_id.as_deref().unwrap_or("-"), p.updated_at.format("%Y-%m-%d %H:%M")
        ));
    }
    if group_policies.is_empty() {
        html.push_str("<tr><td colspan=\"3\">No group policies</td></tr>");
    }
    html.push_str("</tbody></table>");
    Html(html)
}
```

- [ ] **Step 4: Update sidebar in layout.html**

Add Groups, Address Books, and Settings links to the sidebar in `server/src/templates/layout.html`:

```html
    <nav class="sidebar">
        <h4>RustDesk</h4>
        <a href="/console/" hx-boost="true">Dashboard</a>
        <a href="/console/users" hx-boost="true">Users</a>
        <a href="/console/devices" hx-boost="true">Devices</a>
        <a href="/console/groups" hx-boost="true">Groups</a>
        <a href="/console/address-books" hx-boost="true">Address Books</a>
        <a href="/console/settings" hx-boost="true">Settings</a>
        <hr>
        <a href="/console/logout">Logout</a>
    </nav>
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo check -p rustdesk-server-console`

- [ ] **Step 6: Commit**

```bash
git add server/src/routes/console.rs server/src/templates/layout.html
git commit -m "feat(server): add console pages for groups, address books, and settings"
```

---

### Task 15: Final compilation and verification

- [ ] **Step 1: Full compilation**

Run: `cargo check -p rustdesk-server-console`
Expected: Clean compilation

- [ ] **Step 2: Build binary**

Run: `cargo build -p rustdesk-server-console`
Expected: Binary builds successfully

- [ ] **Step 3: Verify all new files exist**

```bash
find server/src/models -name "*.rs" | sort
find server/src/routes -name "*.rs" | sort
find server/src/templates -name "*.html" | sort
```

- [ ] **Step 4: Commit any remaining changes**
