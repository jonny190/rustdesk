# Management Server Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a working management server with local auth, user CRUD, device registration/listing, and an HTMX web console -- compatible with existing RustDesk client API calls.

**Architecture:** New `server/` workspace crate using Axum + SQLx (PostgreSQL) + Askama templates + HTMX. Shares `hbb_common` for protobuf types. Two route groups: `/api/*` (JSON, bearer token auth) for RustDesk clients, `/console/*` (HTML, cookie auth) for browser admin.

**Tech Stack:** Rust 1.75+, Axum 0.8, SQLx 0.8, Askama 0.13, HTMX 2.0, Pico CSS, PostgreSQL 15+, argon2, tokio

**Spec:** `docs/superpowers/specs/2026-03-16-management-server-design.md`

---

## Chunk 1: Project Skeleton and Database

### File Structure for Chunk 1

```
server/
  Cargo.toml
  src/
    main.rs
    config.rs
    db/
      mod.rs
    db/migrations/
      001_initial.sql
```

Also modify: `Cargo.toml` (root workspace)

---

### Task 1: Scaffold the server crate

**Files:**
- Modify: `Cargo.toml` (root, add workspace member)
- Create: `server/Cargo.toml`
- Create: `server/src/main.rs`

- [ ] **Step 1: Add server to workspace members**

In the root `Cargo.toml`, find the `[workspace]` section (around line 206) and add `"server"` to the members list:

```toml
[workspace]
members = ["libs/scrap", "libs/hbb_common", "libs/enigo", "libs/clipboard", "libs/virtual_display", "libs/virtual_display/dylib", "libs/portable", "libs/remote_printer", "server"]
```

- [ ] **Step 2: Create server/Cargo.toml**

```toml
[package]
name = "rustdesk-server-console"
version = "0.1.0"
edition = "2021"
rust-version = "1.75"

[[bin]]
name = "rustdesk-server-console"
path = "src/main.rs"

[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["cors", "fs"] }
sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "uuid", "chrono", "json"] }
askama = { version = "0.13", features = ["with-axum"] }
askama_axum = "0.4"
tower-cookies = "0.10"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
argon2 = "0.5"
rand = "0.8"
hex = "0.4"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
anyhow = "1"
hbb_common = { path = "../libs/hbb_common" }
```

- [ ] **Step 3: Create server/src/main.rs with minimal startup**

```rust
use anyhow::Result;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    tracing::info!("rustdesk-server-console starting");
    Ok(())
}
```

- [ ] **Step 4: Verify it compiles**

Run: `cd /mnt/d/rustdesk && cargo check -p rustdesk-server-console`
Expected: Compiles successfully

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml server/
git commit -m "feat(server): scaffold server crate in workspace"
```

---

### Task 2: Config module

**Files:**
- Create: `server/src/config.rs`
- Modify: `server/src/main.rs`

- [ ] **Step 1: Create config.rs**

```rust
use std::env;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub listen_addr: String,
    pub database_url: String,
    pub secret_key: String,
    pub hbbs_url: String,
    pub session_expiry_days: i64,
}

impl ServerConfig {
    pub fn from_env() -> Self {
        Self {
            listen_addr: env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:21114".into()),
            database_url: env::var("DATABASE_URL").expect("DATABASE_URL must be set"),
            secret_key: env::var("SECRET_KEY").expect("SECRET_KEY must be set"),
            hbbs_url: env::var("HBBS_URL").unwrap_or_default(),
            session_expiry_days: env::var("SESSION_EXPIRY_DAYS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(7),
        }
    }
}
```

- [ ] **Step 2: Wire config into main.rs**

```rust
mod config;

use anyhow::Result;
use config::ServerConfig;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = ServerConfig::from_env();
    tracing::info!("rustdesk-server-console starting on {}", config.listen_addr);
    Ok(())
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cd /mnt/d/rustdesk && cargo check -p rustdesk-server-console`
Expected: Compiles

- [ ] **Step 4: Commit**

```bash
git add server/src/config.rs server/src/main.rs
git commit -m "feat(server): add config module with env var loading"
```

---

### Task 3: Database connection and migrations

**Files:**
- Create: `server/src/db/mod.rs`
- Create: `server/src/db/migrations/001_initial.sql`
- Modify: `server/src/main.rs`

- [ ] **Step 1: Create the initial migration SQL**

Create `server/src/db/migrations/001_initial.sql`:

```sql
-- Users and auth
CREATE TABLE users (
    id UUID PRIMARY KEY,
    username VARCHAR UNIQUE NOT NULL,
    email VARCHAR UNIQUE,
    password_hash VARCHAR,
    is_admin BOOLEAN DEFAULT false,
    status SMALLINT DEFAULT 1,
    oidc_provider VARCHAR,
    oidc_subject VARCHAR,
    totp_secret VARCHAR,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(oidc_provider, oidc_subject)
);

CREATE TABLE sessions (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    access_token VARCHAR UNIQUE NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_sessions_token ON sessions(access_token);
CREATE INDEX idx_sessions_expires ON sessions(expires_at);

-- Devices
CREATE TABLE device_groups (
    id UUID PRIMARY KEY,
    name VARCHAR NOT NULL,
    note TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE devices (
    id VARCHAR PRIMARY KEY,
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    device_group_id UUID REFERENCES device_groups(id) ON DELETE SET NULL,
    hostname VARCHAR,
    os VARCHAR,
    version VARCHAR,
    cpu VARCHAR,
    memory VARCHAR,
    ip VARCHAR,
    custom_id VARCHAR UNIQUE,
    note TEXT,
    status SMALLINT DEFAULT 2,
    last_heartbeat TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_devices_user ON devices(user_id);
CREATE INDEX idx_devices_group ON devices(device_group_id);
```

- [ ] **Step 2: Create db/mod.rs**

```rust
use anyhow::Result;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

pub async fn init_pool(database_url: &str) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await?;

    run_migrations(&pool).await?;
    Ok(pool)
}

async fn run_migrations(pool: &PgPool) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS _migrations (
            name VARCHAR PRIMARY KEY,
            applied_at TIMESTAMPTZ NOT NULL DEFAULT now()
        )"
    )
    .execute(pool)
    .await?;

    let migrations = vec![
        ("001_initial", include_str!("migrations/001_initial.sql")),
    ];

    for (name, sql) in migrations {
        let applied: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM _migrations WHERE name = $1)"
        )
        .bind(name)
        .fetch_one(pool)
        .await?;

        if !applied {
            tracing::info!("Applying migration: {name}");
            sqlx::raw_sql(sql).execute(pool).await?;
            sqlx::query("INSERT INTO _migrations (name) VALUES ($1)")
                .bind(name)
                .execute(pool)
                .await?;
        }
    }

    Ok(())
}
```

- [ ] **Step 3: Wire database into main.rs**

```rust
mod config;
mod db;

use anyhow::Result;
use config::ServerConfig;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = ServerConfig::from_env();
    tracing::info!("rustdesk-server-console starting on {}", config.listen_addr);

    let pool = db::init_pool(&config.database_url).await?;
    tracing::info!("Database connected and migrations applied");

    Ok(())
}
```

- [ ] **Step 4: Verify it compiles**

Run: `cd /mnt/d/rustdesk && cargo check -p rustdesk-server-console`
Expected: Compiles

- [ ] **Step 5: Commit**

```bash
git add server/src/db/
git commit -m "feat(server): add database connection pool and initial migration"
```

---

## Chunk 2: Models and Auth Middleware

### File Structure for Chunk 2

```
server/src/
  models/
    mod.rs
    user.rs
    session.rs
    device.rs
  middleware/
    mod.rs
    auth.rs
```

---

### Task 4: User model

**Files:**
- Create: `server/src/models/mod.rs`
- Create: `server/src/models/user.rs`

- [ ] **Step 1: Create models/user.rs**

```rust
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

/// The payload shape the RustDesk client expects.
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

    pub async fn update_status(pool: &PgPool, id: Uuid, status: i16) -> sqlx::Result<()> {
        sqlx::query("UPDATE users SET status = $1, updated_at = now() WHERE id = $2")
            .bind(status)
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn delete(pool: &PgPool, id: Uuid) -> sqlx::Result<()> {
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }
}
```

- [ ] **Step 2: Create models/mod.rs**

```rust
pub mod user;
pub mod session;
pub mod device;

pub use user::{User, UserPayload};
pub use session::Session;
pub use device::Device;
```

- [ ] **Step 3: Verify it compiles** (will have missing module errors; that's fine, we'll add them next)

We'll add session and device in the following steps. For now, comment out the missing imports in mod.rs and verify user.rs compiles. Actually, let's create stubs first.

- [ ] **Step 4: Commit**

```bash
git add server/src/models/
git commit -m "feat(server): add user model with CRUD operations"
```

---

### Task 5: Session model

**Files:**
- Create: `server/src/models/session.rs`

- [ ] **Step 1: Create models/session.rs**

```rust
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
```

- [ ] **Step 2: Verify it compiles**

Run: `cd /mnt/d/rustdesk && cargo check -p rustdesk-server-console`
Expected: Compiles

- [ ] **Step 3: Commit**

```bash
git add server/src/models/session.rs
git commit -m "feat(server): add session model with token generation"
```

---

### Task 6: Device model

**Files:**
- Create: `server/src/models/device.rs`

- [ ] **Step 1: Create models/device.rs**

```rust
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
    pub async fn upsert_from_heartbeat(
        pool: &PgPool,
        id: &str,
        ip: Option<&str>,
    ) -> sqlx::Result<()> {
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
        pool: &PgPool,
        id: &str,
        hostname: &str,
        os: &str,
        version: &str,
        cpu: &str,
        memory: &str,
    ) -> sqlx::Result<bool> {
        let result = sqlx::query(
            "INSERT INTO devices (id, hostname, os, version, cpu, memory, status)
             VALUES ($1, $2, $3, $4, $5, $6, 1)
             ON CONFLICT (id) DO UPDATE SET
                hostname = $2, os = $3, version = $4, cpu = $5, memory = $6,
                updated_at = now()"
        )
        .bind(id)
        .bind(hostname)
        .bind(os)
        .bind(version)
        .bind(cpu)
        .bind(memory)
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

    pub async fn list(
        pool: &PgPool,
        page: i64,
        page_size: i64,
        status_filter: Option<i16>,
    ) -> sqlx::Result<(i64, Vec<Self>)> {
        let (count_sql, list_sql) = match status_filter {
            Some(_) => (
                "SELECT COUNT(*) FROM devices WHERE status = $1",
                "SELECT * FROM devices WHERE status = $3 ORDER BY created_at DESC LIMIT $1 OFFSET $2",
            ),
            None => (
                "SELECT COUNT(*) FROM devices",
                "SELECT * FROM devices ORDER BY created_at DESC LIMIT $1 OFFSET $2",
            ),
        };

        let total: i64 = match status_filter {
            Some(s) => sqlx::query_scalar(count_sql).bind(s).fetch_one(pool).await?,
            None => sqlx::query_scalar(count_sql).fetch_one(pool).await?,
        };

        let offset = (page - 1) * page_size;
        let devices: Vec<Self> = match status_filter {
            Some(s) => {
                sqlx::query_as(list_sql)
                    .bind(page_size)
                    .bind(offset)
                    .bind(s)
                    .fetch_all(pool)
                    .await?
            }
            None => {
                sqlx::query_as(
                    "SELECT * FROM devices ORDER BY created_at DESC LIMIT $1 OFFSET $2"
                )
                .bind(page_size)
                .bind(offset)
                .fetch_all(pool)
                .await?
            }
        };

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
```

- [ ] **Step 2: Ensure models/mod.rs has all three modules**

```rust
pub mod user;
pub mod session;
pub mod device;

pub use user::{User, UserPayload};
pub use session::Session;
pub use device::Device;
```

- [ ] **Step 3: Add `mod models;` to main.rs**

Update main.rs to include:

```rust
mod config;
mod db;
mod models;
```

- [ ] **Step 4: Verify it compiles**

Run: `cd /mnt/d/rustdesk && cargo check -p rustdesk-server-console`
Expected: Compiles

- [ ] **Step 5: Commit**

```bash
git add server/src/models/
git commit -m "feat(server): add device model with heartbeat upsert and stale detection"
```

---

### Task 7: Auth middleware

**Files:**
- Create: `server/src/middleware/mod.rs`
- Create: `server/src/middleware/auth.rs`

- [ ] **Step 1: Create middleware/auth.rs**

```rust
use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use sqlx::PgPool;

use crate::models::{Session, User};

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user: User,
    pub token: String,
}

/// Extracts bearer token from Authorization header, validates session.
/// Stores AuthUser in request extensions.
pub async fn api_auth(
    State(pool): State<PgPool>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let token = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let session = Session::find_by_token(&pool, token)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let user = User::find_by_id(&pool, session.user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if user.status == 0 {
        return Err(StatusCode::FORBIDDEN);
    }

    req.extensions_mut().insert(AuthUser {
        user,
        token: token.to_string(),
    });

    Ok(next.run(req).await)
}

/// Extracts session cookie, validates session.
/// Stores AuthUser in request extensions.
pub async fn console_auth(
    State(pool): State<PgPool>,
    mut req: Request,
    next: Next,
) -> Response {
    let redirect = || axum::response::Redirect::to("/console/login").into_response();

    let cookie_header = req
        .headers()
        .get("Cookie")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let session_id = match parse_cookie(cookie_header, "session") {
        Some(id) => id,
        None => return redirect(),
    };

    let session = match Session::find_by_token(&pool, &session_id).await {
        Ok(Some(s)) => s,
        _ => return redirect(),
    };

    let user = match User::find_by_id(&pool, session.user_id).await {
        Ok(Some(u)) if u.status != 0 => u,
        _ => return redirect(),
    };

    req.extensions_mut().insert(AuthUser {
        user,
        token: session_id,
    });

    next.run(req).await
}

fn parse_cookie(header: &str, name: &str) -> Option<String> {
    header
        .split(';')
        .filter_map(|pair| {
            let mut parts = pair.trim().splitn(2, '=');
            let key = parts.next()?.trim();
            let val = parts.next()?.trim();
            if key == name { Some(val.to_string()) } else { None }
        })
        .next()
}
```

- [ ] **Step 2: Create middleware/mod.rs**

```rust
pub mod auth;

pub use auth::{api_auth, console_auth, AuthUser};
```

- [ ] **Step 3: Add `mod middleware;` to main.rs**

- [ ] **Step 4: Verify it compiles**

Run: `cd /mnt/d/rustdesk && cargo check -p rustdesk-server-console`
Expected: Compiles

- [ ] **Step 5: Commit**

```bash
git add server/src/middleware/
git commit -m "feat(server): add auth middleware for API bearer tokens and console cookies"
```

---

## Chunk 3: API Routes (Client-Facing)

### File Structure for Chunk 3

```
server/src/
  routes/
    mod.rs
    auth.rs
    devices.rs
    users.rs
  app.rs
```

---

### Task 8: App state and router assembly

**Files:**
- Create: `server/src/app.rs`
- Create: `server/src/routes/mod.rs`
- Modify: `server/src/main.rs`

- [ ] **Step 1: Create app.rs with shared state and router**

```rust
use axum::{middleware as axum_mw, Router};
use sqlx::PgPool;
use tower_http::cors::CorsLayer;

use crate::config::ServerConfig;
use crate::middleware;
use crate::routes;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: ServerConfig,
}

pub fn create_router(state: AppState) -> Router {
    let public_api = routes::public_api_routes();
    let authed_api = routes::authed_api_routes()
        .layer(axum_mw::from_fn_with_state(state.pool.clone(), middleware::api_auth));

    let api = Router::new()
        .merge(public_api)
        .merge(authed_api)
        .layer(CorsLayer::permissive());

    Router::new()
        .nest("/api", api)
        .with_state(state)
}
```

- [ ] **Step 2: Create routes/mod.rs**

```rust
pub mod auth;
pub mod devices;
pub mod users;

use axum::Router;

use crate::app::AppState;

pub fn public_api_routes() -> Router<AppState> {
    Router::new().merge(auth::public_routes())
}

pub fn authed_api_routes() -> Router<AppState> {
    Router::new()
        .merge(auth::authed_routes())
        .merge(devices::routes())
        .merge(users::routes())
}
```

- [ ] **Step 3: Update main.rs to start the HTTP server**

```rust
mod app;
mod config;
mod db;
mod middleware;
mod models;
mod routes;

use anyhow::Result;
use config::ServerConfig;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = ServerConfig::from_env();
    let listen_addr = config.listen_addr.clone();
    tracing::info!("rustdesk-server-console starting on {listen_addr}");

    let pool = db::init_pool(&config.database_url).await?;
    tracing::info!("Database connected and migrations applied");

    let state = app::AppState { pool: pool.clone(), config };
    let router = app::create_router(state);

    // Background task: clean expired sessions every hour
    let cleanup_pool = pool.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            interval.tick().await;
            if let Ok(n) = models::Session::delete_expired(&cleanup_pool).await {
                if n > 0 {
                    tracing::info!("Cleaned up {n} expired sessions");
                }
            }
        }
    });

    // Background task: mark stale devices offline every 30 seconds
    let stale_pool = pool.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            let _ = models::Device::mark_offline_stale(&stale_pool, 90).await;
        }
    });

    let listener = tokio::net::TcpListener::bind(&listen_addr).await?;
    tracing::info!("Listening on {listen_addr}");
    axum::serve(listener, router).await?;

    Ok(())
}
```

- [ ] **Step 4: Verify it compiles**

Run: `cd /mnt/d/rustdesk && cargo check -p rustdesk-server-console`
Expected: Will fail until route files are created (next steps)

- [ ] **Step 5: Commit** (after routes are added in next tasks)

---

### Task 9: Auth API routes (/api/login, /api/logout, /api/currentUser)

**Files:**
- Create: `server/src/routes/auth.rs`

- [ ] **Step 1: Create routes/auth.rs**

```rust
use axum::{
    extract::{Extension, State},
    http::StatusCode,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::app::AppState;
use crate::middleware::AuthUser;
use crate::models::{Session, User, UserPayload};

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    pub id: Option<String>,
    pub uuid: Option<String>,
}

#[derive(Serialize)]
pub struct AuthBody {
    pub r#type: String,
    pub access_token: String,
    pub user: UserPayload,
}

pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/login-options", axum::routing::get(login_options))
}

pub fn authed_routes() -> Router<AppState> {
    Router::new()
        .route("/currentUser", post(current_user))
        .route("/logout", post(logout))
}

async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<AuthBody>, StatusCode> {
    let user = User::find_by_username(&state.pool, &req.username)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if user.status == 0 {
        return Err(StatusCode::FORBIDDEN);
    }

    let password_hash = user.password_hash.as_deref().ok_or(StatusCode::UNAUTHORIZED)?;

    let parsed_hash = argon2::PasswordHash::new(password_hash)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    use argon2::PasswordVerifier;
    argon2::Argon2::default()
        .verify_password(req.password.as_bytes(), &parsed_hash)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let session = Session::create(&state.pool, user.id, state.config.session_expiry_days)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(AuthBody {
        r#type: "access_token".into(),
        access_token: session.access_token,
        user: UserPayload::from(&user),
    }))
}

async fn current_user(
    Extension(auth): Extension<AuthUser>,
) -> Json<UserPayload> {
    Json(UserPayload::from(&auth.user))
}

async fn logout(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
) -> StatusCode {
    let _ = Session::delete_by_token(&state.pool, &auth.token).await;
    StatusCode::OK
}

async fn login_options() -> Json<Vec<String>> {
    // Phase 3: query oidc_providers table for enabled providers
    Json(vec![])
}
```

- [ ] **Step 2: Add `use argon2::PasswordHasher;` import at top and `password-hash` feature**

Add to `server/Cargo.toml` dependencies:

```toml
argon2 = { version = "0.5", features = ["password-hash"] }
```

Wait -- argon2 0.5 already includes password-hash. The `use argon2::PasswordVerifier;` is inline in the function. This should work as-is.

- [ ] **Step 3: Verify it compiles** (will need devices/users route stubs)

---

### Task 10: Device sync API routes (/api/heartbeat, /api/sysinfo, /api/sysinfo_ver)

**Files:**
- Create: `server/src/routes/devices.rs`

- [ ] **Step 1: Create routes/devices.rs**

```rust
use axum::{
    extract::State,
    http::StatusCode,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::app::AppState;
use crate::models::Device;

#[derive(Deserialize)]
pub struct HeartbeatRequest {
    pub id: Option<String>,
    pub uuid: Option<String>,
    pub ver: Option<i64>,
    pub conns: Option<Vec<i64>>,
    pub modified_at: Option<i64>,
}

#[derive(Serialize, Default)]
pub struct HeartbeatResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sysinfo: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disconnect: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy: Option<serde_json::Value>,
}

#[derive(Deserialize)]
pub struct SysinfoRequest {
    pub id: Option<String>,
    pub hostname: Option<String>,
    pub os: Option<String>,
    pub version: Option<String>,
    pub cpu: Option<String>,
    pub memory: Option<String>,
}

pub fn routes() -> Router<AppState> {
    // These are public routes (device-authenticated by ID, not bearer token).
    // They are nested under /api but don't go through bearer auth middleware.
    // We handle this by registering them as public routes.
    Router::new()
}

pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/heartbeat", post(heartbeat))
        .route("/sysinfo", post(sysinfo))
        .route("/sysinfo_ver", post(sysinfo_ver))
}

async fn heartbeat(
    State(state): State<AppState>,
    Json(req): Json<HeartbeatRequest>,
) -> Result<Json<HeartbeatResponse>, StatusCode> {
    let device_id = req.id.as_deref().ok_or(StatusCode::BAD_REQUEST)?;

    Device::upsert_from_heartbeat(&state.pool, device_id, None)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Check if we need sysinfo from this device
    let device = Device::find_by_id(&state.pool, device_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let needs_sysinfo = device
        .as_ref()
        .map(|d| d.hostname.is_none())
        .unwrap_or(true);

    Ok(Json(HeartbeatResponse {
        sysinfo: if needs_sysinfo { Some(true) } else { None },
        ..Default::default()
    }))
}

async fn sysinfo(
    State(state): State<AppState>,
    Json(req): Json<SysinfoRequest>,
) -> Result<String, StatusCode> {
    let id = req.id.as_deref().ok_or(StatusCode::BAD_REQUEST)?;

    let updated = Device::upsert_sysinfo(
        &state.pool,
        id,
        req.hostname.as_deref().unwrap_or(""),
        req.os.as_deref().unwrap_or(""),
        req.version.as_deref().unwrap_or(""),
        req.cpu.as_deref().unwrap_or(""),
        req.memory.as_deref().unwrap_or(""),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if updated {
        Ok("SYSINFO_UPDATED".into())
    } else {
        Ok("ID_NOT_FOUND".into())
    }
}

async fn sysinfo_ver(
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Result<String, StatusCode> {
    let id = req.get("id").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;

    let device = Device::find_by_id(&state.pool, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Return updated_at timestamp as version string
    match device {
        Some(d) => Ok(d.updated_at.timestamp().to_string()),
        None => Ok("0".into()),
    }
}
```

- [ ] **Step 2: Update routes/mod.rs to include device public routes**

```rust
pub mod auth;
pub mod devices;
pub mod users;

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
}
```

- [ ] **Step 3: Verify it compiles** (still need users routes stub)

---

### Task 11: Users API route (/api/users, /api/peers)

**Files:**
- Create: `server/src/routes/users.rs`

- [ ] **Step 1: Create routes/users.rs**

```rust
use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::app::AppState;
use crate::models::{Device, User, UserPayload};

#[derive(Deserialize)]
pub struct PaginationParams {
    pub current: Option<i64>,
    #[serde(rename = "pageSize")]
    pub page_size: Option<i64>,
    pub accessible: Option<String>,
    pub status: Option<i16>,
}

#[derive(Serialize)]
pub struct PaginatedResponse<T: Serialize> {
    pub total: i64,
    pub data: Vec<T>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/users", get(list_users))
        .route("/peers", get(list_peers))
}

async fn list_users(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<UserPayload>>, axum::http::StatusCode> {
    let page = params.current.unwrap_or(1).max(1);
    let page_size = params.page_size.unwrap_or(100).min(500);

    let (total, users) = User::list(&state.pool, page, page_size)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let data = users.iter().map(UserPayload::from).collect();

    Ok(Json(PaginatedResponse { total, data }))
}

async fn list_peers(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<Device>>, axum::http::StatusCode> {
    let page = params.current.unwrap_or(1).max(1);
    let page_size = params.page_size.unwrap_or(100).min(500);

    let (total, devices) = Device::list(&state.pool, page, page_size, params.status)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(PaginatedResponse { total, data: devices }))
}
```

- [ ] **Step 2: Verify everything compiles**

Run: `cd /mnt/d/rustdesk && cargo check -p rustdesk-server-console`
Expected: Compiles successfully

- [ ] **Step 3: Commit all route files**

```bash
git add server/src/app.rs server/src/routes/ server/src/main.rs
git commit -m "feat(server): add API routes for auth, device sync, and user/peer listing"
```

---

## Chunk 4: First-Run Setup and Web Console

### File Structure for Chunk 4

```
server/src/
  routes/
    console.rs
  templates/
    layout.html
    login.html
    setup.html
    dashboard.html
    users/
      list.html
      detail.html
    devices/
      list.html
      detail.html
  static/
    htmx.min.js        (vendored)
    pico.min.css        (vendored)
```

---

### Task 12: Vendor static assets

**Files:**
- Create: `server/src/static/htmx.min.js`
- Create: `server/src/static/pico.min.css`

- [ ] **Step 1: Download HTMX 2.0**

Run: `curl -sL https://unpkg.com/htmx.org@2.0.4/dist/htmx.min.js -o /mnt/d/rustdesk/server/src/static/htmx.min.js`

- [ ] **Step 2: Download Pico CSS**

Run: `curl -sL https://unpkg.com/@picocss/pico@2/css/pico.min.css -o /mnt/d/rustdesk/server/src/static/pico.min.css`

- [ ] **Step 3: Commit vendored assets**

```bash
git add server/src/static/
git commit -m "chore(server): vendor HTMX 2.0 and Pico CSS"
```

---

### Task 13: Base layout template

**Files:**
- Create: `server/src/templates/layout.html`

- [ ] **Step 1: Create layout.html**

```html
<!DOCTYPE html>
<html lang="en" data-theme="light">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>{% block title %}RustDesk Console{% endblock %}</title>
    <link rel="stylesheet" href="/static/pico.min.css">
    <script src="/static/htmx.min.js"></script>
    <style>
        body { display: flex; min-height: 100vh; margin: 0; }
        nav.sidebar { width: 220px; padding: 1rem; background: var(--pico-card-background-color); border-right: 1px solid var(--pico-muted-border-color); }
        nav.sidebar a { display: block; padding: 0.5rem; text-decoration: none; border-radius: 4px; margin-bottom: 0.25rem; }
        nav.sidebar a:hover, nav.sidebar a.active { background: var(--pico-primary-focus); }
        main { flex: 1; padding: 2rem; overflow-y: auto; }
        .topbar { display: flex; justify-content: space-between; align-items: center; margin-bottom: 2rem; padding-bottom: 1rem; border-bottom: 1px solid var(--pico-muted-border-color); }
    </style>
</head>
<body>
    {% block sidebar %}
    <nav class="sidebar">
        <h4>RustDesk</h4>
        <a href="/console/" hx-boost="true">Dashboard</a>
        <a href="/console/users" hx-boost="true">Users</a>
        <a href="/console/devices" hx-boost="true">Devices</a>
        <hr>
        <a href="/console/logout">Logout</a>
    </nav>
    {% endblock %}
    <main>
        <div class="topbar">
            <h2>{% block heading %}{% endblock %}</h2>
            <span>{% block topbar_right %}{% endblock %}</span>
        </div>
        <div id="content">
            {% block content %}{% endblock %}
        </div>
    </main>
</body>
</html>
```

- [ ] **Step 2: Commit**

```bash
git add server/src/templates/layout.html
git commit -m "feat(server): add base layout template with sidebar navigation"
```

---

### Task 14: Login and setup pages

**Files:**
- Create: `server/src/templates/login.html`
- Create: `server/src/templates/setup.html`

- [ ] **Step 1: Create login.html**

```html
<!DOCTYPE html>
<html lang="en" data-theme="light">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Login - RustDesk Console</title>
    <link rel="stylesheet" href="/static/pico.min.css">
    <style>
        body { display: flex; justify-content: center; align-items: center; min-height: 100vh; }
        article { width: 100%; max-width: 400px; }
    </style>
</head>
<body>
    <article>
        <h2>RustDesk Console</h2>
        {% if error.is_some() %}
        <p role="alert" style="color: var(--pico-del-color);">{{ error.as_deref().unwrap_or("") }}</p>
        {% endif %}
        <form method="POST" action="/console/login">
            <label for="username">Username</label>
            <input type="text" id="username" name="username" required autofocus>
            <label for="password">Password</label>
            <input type="password" id="password" name="password" required>
            <button type="submit">Login</button>
        </form>
    </article>
</body>
</html>
```

- [ ] **Step 2: Create setup.html**

```html
<!DOCTYPE html>
<html lang="en" data-theme="light">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Setup - RustDesk Console</title>
    <link rel="stylesheet" href="/static/pico.min.css">
    <style>
        body { display: flex; justify-content: center; align-items: center; min-height: 100vh; }
        article { width: 100%; max-width: 500px; }
    </style>
</head>
<body>
    <article>
        <h2>Initial Setup</h2>
        <p>Create the first admin account to get started.</p>
        {% if error.is_some() %}
        <p role="alert" style="color: var(--pico-del-color);">{{ error.as_deref().unwrap_or("") }}</p>
        {% endif %}
        <form method="POST" action="/console/setup">
            <label for="username">Admin Username</label>
            <input type="text" id="username" name="username" required autofocus>
            <label for="email">Email (optional)</label>
            <input type="email" id="email" name="email">
            <label for="password">Password</label>
            <input type="password" id="password" name="password" required minlength="8">
            <label for="password_confirm">Confirm Password</label>
            <input type="password" id="password_confirm" name="password_confirm" required minlength="8">
            <button type="submit">Create Admin Account</button>
        </form>
    </article>
</body>
</html>
```

- [ ] **Step 3: Commit**

```bash
git add server/src/templates/login.html server/src/templates/setup.html
git commit -m "feat(server): add login and first-run setup page templates"
```

---

### Task 15: Dashboard, users, and devices page templates

**Files:**
- Create: `server/src/templates/dashboard.html`
- Create: `server/src/templates/users/list.html`
- Create: `server/src/templates/users/detail.html`
- Create: `server/src/templates/devices/list.html`
- Create: `server/src/templates/devices/detail.html`

- [ ] **Step 1: Create dashboard.html**

```html
{% extends "layout.html" %}

{% block title %}Dashboard - RustDesk Console{% endblock %}
{% block heading %}Dashboard{% endblock %}

{% block content %}
<div style="display: grid; grid-template-columns: repeat(3, 1fr); gap: 1rem; margin-bottom: 2rem;">
    <article>
        <h3 hx-get="/console/stats/users" hx-trigger="load" hx-swap="innerHTML">-</h3>
        <p>Total Users</p>
    </article>
    <article>
        <h3 hx-get="/console/stats/devices-online" hx-trigger="load, every 10s" hx-swap="innerHTML">-</h3>
        <p>Online Devices</p>
    </article>
    <article>
        <h3 hx-get="/console/stats/devices-total" hx-trigger="load" hx-swap="innerHTML">-</h3>
        <p>Total Devices</p>
    </article>
</div>
{% endblock %}
```

- [ ] **Step 2: Create users/list.html**

```html
{% extends "layout.html" %}

{% block title %}Users - RustDesk Console{% endblock %}
{% block heading %}Users{% endblock %}

{% block content %}
<div style="display: flex; justify-content: space-between; margin-bottom: 1rem;">
    <input type="search" placeholder="Search users..."
           hx-get="/console/users/table" hx-trigger="keyup changed delay:300ms"
           hx-target="#user-table" name="search" style="max-width: 300px;">
    <a href="/console/users/new" role="button">Add User</a>
</div>
<table>
    <thead>
        <tr>
            <th>Username</th>
            <th>Email</th>
            <th>Role</th>
            <th>Status</th>
            <th>Created</th>
            <th>Actions</th>
        </tr>
    </thead>
    <tbody id="user-table" hx-get="/console/users/table" hx-trigger="load" hx-swap="innerHTML">
    </tbody>
</table>
<div id="user-pagination"></div>
{% endblock %}
```

- [ ] **Step 3: Create users/detail.html**

```html
{% extends "layout.html" %}

{% block title %}User: {{ user.username }} - RustDesk Console{% endblock %}
{% block heading %}User: {{ user.username }}{% endblock %}

{% block content %}
<form method="POST" action="/console/users/{{ user.id }}">
    <label for="username">Username</label>
    <input type="text" id="username" name="username" value="{{ user.username }}" required>
    <label for="email">Email</label>
    <input type="email" id="email" name="email" value="{{ user.email.as_deref().unwrap_or("") }}">
    <fieldset>
        <label>
            <input type="checkbox" name="is_admin" {% if user.is_admin %}checked{% endif %}>
            Administrator
        </label>
    </fieldset>
    <label for="status">Status</label>
    <select id="status" name="status">
        <option value="1" {% if user.status == 1 %}selected{% endif %}>Normal</option>
        <option value="0" {% if user.status == 0 %}selected{% endif %}>Disabled</option>
    </select>
    <div style="display: flex; gap: 1rem;">
        <button type="submit">Save</button>
        <button type="button" class="secondary"
                hx-delete="/console/users/{{ user.id }}"
                hx-confirm="Delete this user?"
                hx-target="body">Delete</button>
    </div>
</form>
{% endblock %}
```

- [ ] **Step 4: Create devices/list.html**

```html
{% extends "layout.html" %}

{% block title %}Devices - RustDesk Console{% endblock %}
{% block heading %}Devices{% endblock %}

{% block content %}
<input type="search" placeholder="Search devices..."
       hx-get="/console/devices/table" hx-trigger="keyup changed delay:300ms"
       hx-target="#device-table" name="search" style="max-width: 300px; margin-bottom: 1rem;">
<table>
    <thead>
        <tr>
            <th>ID</th>
            <th>Hostname</th>
            <th>OS</th>
            <th>Status</th>
            <th>Last Seen</th>
            <th>Actions</th>
        </tr>
    </thead>
    <tbody id="device-table" hx-get="/console/devices/table" hx-trigger="load" hx-swap="innerHTML">
    </tbody>
</table>
<div id="device-pagination"></div>
{% endblock %}
```

- [ ] **Step 5: Create devices/detail.html**

```html
{% extends "layout.html" %}

{% block title %}Device: {{ device.id }} - RustDesk Console{% endblock %}
{% block heading %}Device: {{ device.id }}{% endblock %}

{% block content %}
<div style="display: grid; grid-template-columns: 1fr 1fr; gap: 2rem;">
    <article>
        <h4>System Info</h4>
        <dl>
            <dt>Hostname</dt><dd>{{ device.hostname.as_deref().unwrap_or("-") }}</dd>
            <dt>OS</dt><dd>{{ device.os.as_deref().unwrap_or("-") }}</dd>
            <dt>Version</dt><dd>{{ device.version.as_deref().unwrap_or("-") }}</dd>
            <dt>CPU</dt><dd>{{ device.cpu.as_deref().unwrap_or("-") }}</dd>
            <dt>Memory</dt><dd>{{ device.memory.as_deref().unwrap_or("-") }}</dd>
            <dt>IP</dt><dd>{{ device.ip.as_deref().unwrap_or("-") }}</dd>
            <dt>Status</dt><dd>{% if device.status == 1 %}Online{% elif device.status == 0 %}Disabled{% else %}Offline{% endif %}</dd>
        </dl>
    </article>
    <article>
        <h4>Settings</h4>
        <form method="POST" action="/console/devices/{{ device.id }}">
            <label for="note">Note</label>
            <textarea id="note" name="note">{{ device.note.as_deref().unwrap_or("") }}</textarea>
            <label for="custom_id">Custom ID</label>
            <input type="text" id="custom_id" name="custom_id" value="{{ device.custom_id.as_deref().unwrap_or("") }}">
            <button type="submit">Save</button>
        </form>
    </article>
</div>
{% endblock %}
```

- [ ] **Step 6: Commit**

```bash
git add server/src/templates/dashboard.html server/src/templates/users/ server/src/templates/devices/
git commit -m "feat(server): add dashboard, user, and device page templates"
```

---

### Task 16: Console routes (HTML handlers)

**Files:**
- Create: `server/src/routes/console.rs`
- Modify: `server/src/routes/mod.rs`
- Modify: `server/src/app.rs`

- [ ] **Step 1: Create routes/console.rs**

```rust
use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHasher};
use askama::Template;
use axum::{
    extract::{Extension, Path, Query, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Form, Router,
};
use rand::rngs::OsRng;
use serde::Deserialize;
use uuid::Uuid;

use crate::app::AppState;
use crate::middleware::{self, AuthUser};
use crate::models::{Device, Session, User};

// -- Template structs --

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate {
    error: Option<String>,
}

#[derive(Template)]
#[template(path = "setup.html")]
struct SetupTemplate {
    error: Option<String>,
}

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate {}

#[derive(Template)]
#[template(path = "users/list.html")]
struct UsersListTemplate {}

#[derive(Template)]
#[template(path = "users/detail.html")]
struct UserDetailTemplate {
    user: User,
}

#[derive(Template)]
#[template(path = "devices/list.html")]
struct DevicesListTemplate {}

#[derive(Template)]
#[template(path = "devices/detail.html")]
struct DeviceDetailTemplate {
    device: Device,
}

// -- Form structs --

#[derive(Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct SetupForm {
    pub username: String,
    pub email: Option<String>,
    pub password: String,
    pub password_confirm: String,
}

// -- Route registration --

pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/console/login", get(login_page).post(login_submit))
        .route("/console/setup", get(setup_page).post(setup_submit))
}

pub fn authed_routes(pool: sqlx::PgPool) -> Router<AppState> {
    Router::new()
        .route("/console/", get(dashboard))
        .route("/console/logout", get(logout))
        .route("/console/users", get(users_list))
        .route("/console/users/new", get(user_new_page).post(user_create))
        .route("/console/users/{id}", get(user_detail).post(user_update))
        .route("/console/users/{id}/delete", post(user_delete))
        .route("/console/devices", get(devices_list))
        .route("/console/devices/{id}", get(device_detail).post(device_update))
        .route("/console/stats/users", get(stats_users))
        .route("/console/stats/devices-online", get(stats_devices_online))
        .route("/console/stats/devices-total", get(stats_devices_total))
        .route("/console/users/table", get(users_table_fragment))
        .route("/console/devices/table", get(devices_table_fragment))
        .layer(axum::middleware::from_fn_with_state(pool, middleware::console_auth))
}

// -- Handlers --

async fn login_page(State(state): State<AppState>) -> Response {
    // Redirect to setup if no users exist
    if let Ok(count) = User::count(&state.pool).await {
        if count == 0 {
            return Redirect::to("/console/setup").into_response();
        }
    }
    Html(LoginTemplate { error: None }.render().unwrap_or_default()).into_response()
}

async fn login_submit(
    State(state): State<AppState>,
    Form(form): Form<LoginForm>,
) -> Response {
    let render_error = |msg: &str| {
        Html(LoginTemplate { error: Some(msg.to_string()) }.render().unwrap_or_default()).into_response()
    };

    let user = match User::find_by_username(&state.pool, &form.username).await {
        Ok(Some(u)) => u,
        _ => return render_error("Invalid username or password"),
    };

    if user.status == 0 {
        return render_error("Account is disabled");
    }

    let password_hash = match &user.password_hash {
        Some(h) => h.clone(),
        None => return render_error("Invalid username or password"),
    };

    let parsed_hash = match argon2::PasswordHash::new(&password_hash) {
        Ok(h) => h,
        Err(_) => return render_error("Internal error"),
    };

    use argon2::PasswordVerifier;
    if Argon2::default().verify_password(form.password.as_bytes(), &parsed_hash).is_err() {
        return render_error("Invalid username or password");
    }

    let session = match Session::create(&state.pool, user.id, state.config.session_expiry_days).await {
        Ok(s) => s,
        Err(_) => return render_error("Internal error"),
    };

    let cookie = format!(
        "session={}; HttpOnly; Path=/; SameSite=Lax; Max-Age={}",
        session.access_token,
        state.config.session_expiry_days * 86400
    );

    (
        StatusCode::SEE_OTHER,
        [(header::SET_COOKIE, cookie), (header::LOCATION, "/console/".into())],
    ).into_response()
}

async fn setup_page(State(state): State<AppState>) -> Response {
    if let Ok(count) = User::count(&state.pool).await {
        if count > 0 {
            return Redirect::to("/console/login").into_response();
        }
    }
    Html(SetupTemplate { error: None }.render().unwrap_or_default()).into_response()
}

async fn setup_submit(
    State(state): State<AppState>,
    Form(form): Form<SetupForm>,
) -> Response {
    let render_error = |msg: &str| {
        Html(SetupTemplate { error: Some(msg.to_string()) }.render().unwrap_or_default()).into_response()
    };

    // Guard: no setup if users exist
    if let Ok(count) = User::count(&state.pool).await {
        if count > 0 {
            return Redirect::to("/console/login").into_response();
        }
    }

    if form.password != form.password_confirm {
        return render_error("Passwords do not match");
    }

    if form.password.len() < 8 {
        return render_error("Password must be at least 8 characters");
    }

    if form.username.trim().is_empty() {
        return render_error("Username is required");
    }

    let salt = SaltString::generate(&mut OsRng);
    let password_hash = match Argon2::default().hash_password(form.password.as_bytes(), &salt) {
        Ok(h) => h.to_string(),
        Err(_) => return render_error("Internal error"),
    };

    let email = form.email.as_deref().filter(|e| !e.is_empty());

    match User::create(&state.pool, form.username.trim(), email, &password_hash, true).await {
        Ok(_) => Redirect::to("/console/login").into_response(),
        Err(e) => render_error(&format!("Failed to create admin: {e}")),
    }
}

async fn logout(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
) -> Response {
    let _ = Session::delete_by_token(&state.pool, &auth.token).await;
    let cookie = "session=; HttpOnly; Path=/; Max-Age=0";
    (
        StatusCode::SEE_OTHER,
        [(header::SET_COOKIE, cookie.to_string()), (header::LOCATION, "/console/login".into())],
    ).into_response()
}

async fn dashboard() -> Html<String> {
    Html(DashboardTemplate {}.render().unwrap_or_default())
}

async fn users_list() -> Html<String> {
    Html(UsersListTemplate {}.render().unwrap_or_default())
}

async fn user_detail(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Html<String>, StatusCode> {
    let user = User::find_by_id(&state.pool, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Html(UserDetailTemplate { user }.render().unwrap_or_default()))
}

async fn devices_list() -> Html<String> {
    Html(DevicesListTemplate {}.render().unwrap_or_default())
}

async fn device_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Html<String>, StatusCode> {
    let device = Device::find_by_id(&state.pool, &id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Html(DeviceDetailTemplate { device }.render().unwrap_or_default()))
}

// -- User CRUD handlers --

#[derive(Deserialize)]
pub struct UserForm {
    pub username: String,
    pub email: Option<String>,
    pub password: Option<String>,
    pub is_admin: Option<String>,
    pub status: i16,
}

async fn user_new_page() -> Html<String> {
    Html(SetupTemplate { error: None }.render().unwrap_or_default())
    // Reuse setup template structure or create a dedicated new-user template.
    // For now, the user creation is handled via the same form pattern as setup.
}

async fn user_create(
    State(state): State<AppState>,
    Form(form): Form<UserForm>,
) -> Response {
    if form.username.trim().is_empty() {
        return Redirect::to("/console/users").into_response();
    }

    let password = form.password.as_deref().unwrap_or("changeme");
    let salt = SaltString::generate(&mut OsRng);
    let password_hash = match Argon2::default().hash_password(password.as_bytes(), &salt) {
        Ok(h) => h.to_string(),
        Err(_) => return Redirect::to("/console/users").into_response(),
    };

    let email = form.email.as_deref().filter(|e| !e.is_empty());
    let is_admin = form.is_admin.is_some();

    let _ = User::create(&state.pool, form.username.trim(), email, &password_hash, is_admin).await;
    Redirect::to("/console/users").into_response()
}

async fn user_update(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Form(form): Form<UserForm>,
) -> Response {
    let is_admin = form.is_admin.is_some();
    let email = form.email.as_deref().filter(|e| !e.is_empty());

    let _ = sqlx::query(
        "UPDATE users SET username = $1, email = $2, is_admin = $3, status = $4, updated_at = now() WHERE id = $5"
    )
    .bind(form.username.trim())
    .bind(email)
    .bind(is_admin)
    .bind(form.status)
    .bind(id)
    .execute(&state.pool)
    .await;

    Redirect::to(&format!("/console/users/{id}")).into_response()
}

async fn user_delete(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Response {
    let _ = User::delete(&state.pool, id).await;
    Redirect::to("/console/users").into_response()
}

async fn device_update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Form(form): Form<DeviceUpdateForm>,
) -> Response {
    let _ = sqlx::query(
        "UPDATE devices SET note = $1, custom_id = NULLIF($2, ''), updated_at = now() WHERE id = $3"
    )
    .bind(&form.note)
    .bind(&form.custom_id)
    .bind(&id)
    .execute(&state.pool)
    .await;

    Redirect::to(&format!("/console/devices/{id}")).into_response()
}

#[derive(Deserialize)]
pub struct DeviceUpdateForm {
    pub note: Option<String>,
    pub custom_id: Option<String>,
}

// -- HTMX fragment endpoints --

#[derive(Deserialize)]
pub struct SearchParams {
    pub search: Option<String>,
    pub page: Option<i64>,
}

async fn stats_users(State(state): State<AppState>) -> String {
    User::count(&state.pool).await.map(|c| c.to_string()).unwrap_or_else(|_| "-".into())
}

async fn stats_devices_online(State(state): State<AppState>) -> String {
    let result: Result<i64, _> = sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE status = 1")
        .fetch_one(&state.pool)
        .await;
    result.map(|c| c.to_string()).unwrap_or_else(|_| "-".into())
}

async fn stats_devices_total(State(state): State<AppState>) -> String {
    let result: Result<i64, _> = sqlx::query_scalar("SELECT COUNT(*) FROM devices")
        .fetch_one(&state.pool)
        .await;
    result.map(|c| c.to_string()).unwrap_or_else(|_| "-".into())
}

async fn users_table_fragment(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> Html<String> {
    let page = params.page.unwrap_or(1).max(1);
    let (_, users) = User::list(&state.pool, page, 50, params.search.as_deref()).await.unwrap_or_default();

    let mut html = String::new();
    for u in &users {
        let status = match u.status {
            1 => "Normal",
            0 => "Disabled",
            _ => "Unverified",
        };
        let role = if u.is_admin { "Admin" } else { "User" };
        html.push_str(&format!(
            "<tr><td><a href=\"/console/users/{}\">{}</a></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td><a href=\"/console/users/{}\">Edit</a></td></tr>",
            u.id, u.username, u.email.as_deref().unwrap_or("-"), role, status, u.created_at.format("%Y-%m-%d"), u.id
        ));
    }
    if users.is_empty() {
        html.push_str("<tr><td colspan=\"6\">No users found</td></tr>");
    }
    Html(html)
}

async fn devices_table_fragment(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> Html<String> {
    let page = params.page.unwrap_or(1).max(1);
    let (_, devices) = Device::list(&state.pool, page, 50, None).await.unwrap_or_default();

    let mut html = String::new();
    for d in &devices {
        let status = match d.status {
            1 => "<mark>Online</mark>",
            0 => "Disabled",
            _ => "Offline",
        };
        let last_seen = d.last_heartbeat
            .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "-".into());
        html.push_str(&format!(
            "<tr><td><a href=\"/console/devices/{}\">{}</a></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td><a href=\"/console/devices/{}\">View</a></td></tr>",
            d.id, d.id, d.hostname.as_deref().unwrap_or("-"), d.os.as_deref().unwrap_or("-"), status, last_seen, d.id
        ));
    }
    if devices.is_empty() {
        html.push_str("<tr><td colspan=\"6\">No devices registered</td></tr>");
    }
    Html(html)
}
```

- [ ] **Step 2: Update routes/mod.rs to include console routes**

```rust
pub mod auth;
pub mod console;
pub mod devices;
pub mod users;

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
}
```

- [ ] **Step 3: Update app.rs to include console routes and static file serving**

```rust
use axum::{middleware as axum_mw, Router};
use sqlx::PgPool;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use crate::config::ServerConfig;
use crate::middleware;
use crate::routes;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: ServerConfig,
}

pub fn create_router(state: AppState) -> Router {
    let public_api = routes::public_api_routes();
    let authed_api = routes::authed_api_routes()
        .layer(axum_mw::from_fn_with_state(state.pool.clone(), middleware::api_auth));

    let api = Router::new()
        .merge(public_api)
        .merge(authed_api)
        .layer(CorsLayer::permissive());

    let console_public = routes::console::public_routes();
    let console_authed = routes::console::authed_routes(state.pool.clone());

    Router::new()
        .nest("/api", api)
        .merge(console_public)
        .merge(console_authed)
        .nest_service("/static", ServeDir::new(
            std::env::var("STATIC_DIR").unwrap_or_else(|_| "server/src/static".into())
        ))
        .with_state(state)
}
```

- [ ] **Step 4: Verify everything compiles**

Run: `cd /mnt/d/rustdesk && cargo check -p rustdesk-server-console`
Expected: Compiles successfully

- [ ] **Step 5: Commit**

```bash
git add server/src/routes/console.rs server/src/routes/mod.rs server/src/app.rs
git commit -m "feat(server): add web console routes with login, setup, dashboard, user and device pages"
```

---

## Chunk 5: Integration Test and Final Wiring

### Task 17: End-to-end smoke test

**Files:**
- Create: `server/tests/smoke_test.rs`

- [ ] **Step 1: Create smoke test**

This test requires a running PostgreSQL instance. Set `TEST_DATABASE_URL` env var.

```rust
//! Smoke tests for the management server.
//! Requires: TEST_DATABASE_URL env var pointing to a test PostgreSQL database.
//! Tests must run serially (they share a database). Use:
//!   cargo test -p rustdesk-server-console -- --test-threads=1

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

async fn setup() -> (axum::Router, sqlx::PgPool) {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be set for integration tests");

    let pool = sqlx::PgPool::connect(&database_url).await.unwrap();

    // Clean slate
    sqlx::raw_sql("DROP TABLE IF EXISTS _migrations, sessions, devices, device_groups, users CASCADE")
        .execute(&pool)
        .await
        .unwrap();

    // Re-run migrations
    rustdesk_server_console::db::init_pool_from_existing(pool.clone()).await.unwrap();

    let config = rustdesk_server_console::config::ServerConfig {
        listen_addr: "127.0.0.1:0".into(),
        database_url,
        secret_key: "test-secret".into(),
        hbbs_url: String::new(),
        session_expiry_days: 1,
    };

    let state = rustdesk_server_console::app::AppState { pool: pool.clone(), config };
    let router = rustdesk_server_console::app::create_router(state);

    (router, pool)
}

#[tokio::test]
async fn test_setup_page_shown_when_no_users() {
    let (app, _pool) = setup().await;

    let response = app
        .oneshot(Request::get("/console/login").body(Body::empty()).unwrap())
        .await
        .unwrap();

    // Should redirect to setup
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        response.headers().get("location").unwrap().to_str().unwrap(),
        "/console/setup"
    );
}

#[tokio::test]
async fn test_login_returns_unauthorized_for_bad_credentials() {
    let (app, _pool) = setup().await;

    let response = app
        .oneshot(
            Request::post("/api/login")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"username":"nope","password":"nope"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_heartbeat_registers_device() {
    let (app, pool) = setup().await;

    let response = app
        .oneshot(
            Request::post("/api/heartbeat")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"id":"test123","uuid":"abc"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Verify device was created
    let device: Option<(String,)> =
        sqlx::query_as("SELECT id FROM devices WHERE id = 'test123'")
            .fetch_optional(&pool)
            .await
            .unwrap();
    assert!(device.is_some());
}
```

- [ ] **Step 2: Expose necessary modules as library for tests**

Create `server/src/lib.rs`:

```rust
pub mod app;
pub mod config;
pub mod db;
pub mod middleware;
pub mod models;
pub mod routes;
```

Update `server/src/main.rs` to use lib:

```rust
use anyhow::Result;
use rustdesk_server_console::app;
use rustdesk_server_console::config::ServerConfig;
use rustdesk_server_console::db;
use rustdesk_server_console::models;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = ServerConfig::from_env();
    let listen_addr = config.listen_addr.clone();
    tracing::info!("rustdesk-server-console starting on {listen_addr}");

    let pool = db::init_pool(&config.database_url).await?;
    tracing::info!("Database connected and migrations applied");

    let state = app::AppState { pool: pool.clone(), config };
    let router = app::create_router(state);

    // Background task: clean expired sessions every hour
    let cleanup_pool = pool.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            interval.tick().await;
            if let Ok(n) = models::Session::delete_expired(&cleanup_pool).await {
                if n > 0 {
                    tracing::info!("Cleaned up {n} expired sessions");
                }
            }
        }
    });

    // Background task: mark stale devices offline every 30 seconds
    let stale_pool = pool.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            let _ = models::Device::mark_offline_stale(&stale_pool, 90).await;
        }
    });

    let listener = tokio::net::TcpListener::bind(&listen_addr).await?;
    tracing::info!("Listening on {listen_addr}");
    axum::serve(listener, router).await?;

    Ok(())
}
```

- [ ] **Step 3: Add `init_pool_from_existing` to db/mod.rs for tests**

Add this function to `server/src/db/mod.rs`:

```rust
/// For tests: run migrations on an already-connected pool.
pub async fn init_pool_from_existing(pool: PgPool) -> Result<()> {
    run_migrations(&pool).await?;
    Ok(())
}
```

- [ ] **Step 4: Update server/Cargo.toml to export lib**

Add to `server/Cargo.toml`:

```toml
[lib]
name = "rustdesk_server_console"
path = "src/lib.rs"
```

- [ ] **Step 5: Verify tests compile**

Run: `cd /mnt/d/rustdesk && cargo test -p rustdesk-server-console --no-run`
Expected: Compiles (tests won't run without a database)

- [ ] **Step 6: Commit**

```bash
git add server/
git commit -m "feat(server): add integration smoke tests and lib.rs for testability"
```

---

### Task 18: Final verification

- [ ] **Step 1: Full compilation check**

Run: `cd /mnt/d/rustdesk && cargo check -p rustdesk-server-console`
Expected: Clean compilation with no errors

- [ ] **Step 2: Verify the workspace still builds**

Run: `cd /mnt/d/rustdesk && cargo check`
Expected: Entire workspace compiles (server crate doesn't break anything)

- [ ] **Step 3: Build the binary**

Run: `cd /mnt/d/rustdesk && cargo build -p rustdesk-server-console`
Expected: Binary produced at `target/debug/rustdesk-server-console`

- [ ] **Step 4: Final commit if any loose changes**

```bash
git status
# If anything unstaged, add and commit
```
