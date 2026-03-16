# RustDesk Management Server - Design Specification

Self-hosted management server and web console that runs alongside the existing RustDesk rendezvous/relay server (hbbs/hbbr) as a standalone companion.

## Stack

- **Runtime:** Rust (Axum + Tower)
- **Database:** PostgreSQL (via SQLx with compile-time checked queries)
- **Templates:** Askama (server-rendered HTML)
- **Interactivity:** HTMX (vendored, no CDN)
- **CSS:** Pico CSS or Simple.css (classless, lightweight)
- **Auth:** argon2 (password hashing), jsonwebtoken (JWT/bearer tokens), reqwest (OIDC provider calls)
- **Shared types:** hbb_common workspace crate (protobuf definitions, config structs)

Ships as a single binary: `rustdesk-server-console`.

## Phasing

### Phase 1 - Foundation
- Web console with login (local accounts)
- User management (CRUD, roles)
- Device registration and listing

### Phase 2 - Core Management
- Address book (shared, per-user, per-group)
- Device groups
- Centralized settings/policy push
- Change ID management

### Phase 3 - Enterprise
- OIDC/OAuth integration
- Audit logging
- Access control (granular permissions)
- Concurrent connection tracking

## Project Structure

New workspace member at `server/` in the RustDesk repository.

```
server/
  Cargo.toml              # depends on hbb_common, axum, sqlx, askama, tower
  src/
    main.rs               # CLI args, config loading, server startup
    config.rs             # Server config (listen addr, DB URL, secret key, hbbs URL)
    db/
      mod.rs              # SQLx connection pool setup, migrations
      migrations/         # SQL migration files
    routes/
      mod.rs              # Router assembly
      auth.rs             # /api/login, /api/logout, /api/currentUser, /api/login-options
      oidc.rs             # /api/oidc/auth, /api/oidc/auth-query
      users.rs            # /api/users CRUD
      devices.rs          # /api/peers, /api/devices/cli, /api/sysinfo
      groups.rs           # /api/device-group/*
      ab.rs               # /api/ab/* (address book, tags, peers)
      audit.rs            # /api/audit, /api/record
      settings.rs         # Centralized settings/strategy push
      console.rs          # HTML page routes for web console
    models/               # SQLx row types and domain structs
    middleware/            # Auth extraction, session management
    templates/            # Askama HTML templates
      layout.html
      login.html
      dashboard.html
      users/
      devices/
      groups/
      address_books/
      audit/
      settings/
    static/               # CSS, JS (HTMX), vendored assets
```

### Cargo.toml Dependencies

- `axum` + `tower` - HTTP framework and middleware
- `sqlx` with `postgres` and `runtime-tokio` features - database
- `askama` with `axum` integration - templates
- `hbb_common` (workspace path dependency) - shared protobuf types and config
- `argon2` - password hashing
- `jsonwebtoken` - bearer token generation/validation
- `reqwest` - OIDC provider HTTP calls
- `tower-governor` - rate limiting
- `tracing` + `tracing-subscriber` - structured logging
- `serde` + `serde_json` - serialization
- `uuid` - ID generation
- `tokio` - async runtime

## Database Schema

### Users and Auth

```sql
CREATE TABLE users (
    id UUID PRIMARY KEY,
    username VARCHAR UNIQUE NOT NULL,
    email VARCHAR UNIQUE,
    password_hash VARCHAR,
    is_admin BOOLEAN DEFAULT false,
    status SMALLINT DEFAULT 1,        -- 0=disabled, 1=normal, -1=unverified
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
```

### Devices

```sql
CREATE TABLE devices (
    id VARCHAR PRIMARY KEY,           -- RustDesk peer ID string
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
    status SMALLINT DEFAULT 2,        -- 0=disabled, 1=online, 2=offline
    last_heartbeat TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_devices_user ON devices(user_id);
CREATE INDEX idx_devices_group ON devices(device_group_id);
```

### Groups

```sql
CREATE TABLE device_groups (
    id UUID PRIMARY KEY,
    name VARCHAR NOT NULL,
    note TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

### Address Books

```sql
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
    rule SMALLINT NOT NULL,           -- 1=read, 2=readWrite, 3=fullControl
    UNIQUE(address_book_id, user_id),
    UNIQUE(address_book_id, group_id),
    CHECK (
        (user_id IS NOT NULL AND group_id IS NULL) OR
        (user_id IS NULL AND group_id IS NOT NULL)
    )
);
```

### Roles and Access Control (Phase 3)

```sql
CREATE TABLE roles (
    id UUID PRIMARY KEY,
    name VARCHAR UNIQUE NOT NULL,
    permissions JSONB NOT NULL
);

CREATE TABLE user_roles (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role_id UUID NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, role_id)
);
```

### Audit Log

```sql
CREATE TABLE audit_log (
    id BIGSERIAL PRIMARY KEY,
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    action VARCHAR NOT NULL,
    target_type VARCHAR,
    target_id VARCHAR,
    detail JSONB,
    ip VARCHAR,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_audit_created ON audit_log(created_at);
CREATE INDEX idx_audit_user_created ON audit_log(user_id, created_at);
```

### Centralized Settings

```sql
CREATE TABLE settings_policies (
    id UUID PRIMARY KEY,
    name VARCHAR NOT NULL,
    target_type VARCHAR NOT NULL,     -- "global", "group", "device"
    target_id VARCHAR,                -- NULL for global
    config_options JSONB NOT NULL,
    extra JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_policies_target ON settings_policies(target_type, target_id);
```

### Session Recordings

```sql
CREATE TABLE recordings (
    id UUID PRIMARY KEY,
    device_id VARCHAR REFERENCES devices(id) ON DELETE SET NULL,
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    filename VARCHAR NOT NULL,
    size_bytes BIGINT,
    note TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

### OIDC Providers (Phase 3)

```sql
CREATE TABLE oidc_providers (
    id UUID PRIMARY KEY,
    name VARCHAR NOT NULL,
    client_id VARCHAR NOT NULL,
    client_secret VARCHAR NOT NULL,
    issuer_url VARCHAR NOT NULL,
    enabled BOOLEAN DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

## API Routes

### Client-Facing API (JSON)

These endpoints must match what the RustDesk client already calls. The request/response formats are dictated by existing client code.

**Pagination convention:** All paginated endpoints accept `current` (1-based page number) and `pageSize` query parameters. Responses use the envelope `{total: int, data: [...]}`.

**Note:** Several read operations use POST (e.g. address book listings). This matches the existing client convention, not REST convention. The server must use the exact methods the client sends.

#### Authentication

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/api/login` | Login with `{username, password, id, uuid}`. Returns `AuthBody`: `{type: "access_token", access_token, tfa_type, secret, user: UserPayload}`. For 2FA challenge, returns `{type: "2fa", tfa_type: "totp"}` instead |
| POST | `/api/currentUser` | Returns user payload from bearer token |
| POST | `/api/logout` | Invalidates session |
| GET | `/api/login-options` | Returns list of enabled OIDC providers |
| POST | `/api/oidc/auth` | Initiates OIDC flow with `{op, id, uuid, deviceInfo}`, returns `{code, url}` |
| GET | `/api/oidc/auth-query` | Polls OIDC result with `?code=...&id=...&uuid=...` |
| GET | `/api/oidc/callback` | OIDC provider redirect target; exchanges authorization code for tokens |

#### Device Sync

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/api/heartbeat` | Device heartbeat with `{id, uuid, ver, conns, modified_at}`. Response carries strategy push: `{sysinfo?: bool, disconnect?: [int], modified_at?: i64, strategy?: {config_options, extra}}` |
| POST | `/api/sysinfo` | Device system info upload. Returns plain text `SYSINFO_UPDATED` on success or `ID_NOT_FOUND` if device unknown |
| POST | `/api/sysinfo_ver` | Check if sysinfo needs re-upload. Returns plain text version string; client compares to cached version |
| POST | `/api/devices/cli` | Device CLI operations |

#### Groups and Users

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/api/device-group/accessible` | Paginated device groups. Query: `current`, `pageSize` |
| GET | `/api/users` | Paginated users list. Query: `current`, `pageSize`, `accessible`, `status` |
| GET | `/api/peers` | Paginated peers list. Query: `current`, `pageSize`, `accessible`, `status` |

#### Address Book

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/api/ab` | Fetch personal address book (legacy) |
| POST | `/api/ab` | Push personal address book (legacy) |
| POST | `/api/ab/settings` | Address book settings (max peers) |
| POST | `/api/ab/personal` | Get personal address book GUID |
| POST | `/api/ab/shared/profiles` | List shared address books |
| POST | `/api/ab/peers` | List peers in address book |
| POST | `/api/ab/peer/add/{guid}` | Add peer to address book |
| PUT | `/api/ab/peer/update/{guid}` | Update peer properties |
| DELETE | `/api/ab/peer/{guid}` | Delete peers from address book |
| POST | `/api/ab/tags/{guid}` | List tags in address book |
| POST | `/api/ab/tag/add/{guid}` | Add tag |
| PUT | `/api/ab/tag/rename/{guid}` | Rename tag |
| PUT | `/api/ab/tag/update/{guid}` | Update tag color |
| DELETE | `/api/ab/tag/{guid}` | Delete tag |

#### Audit and Recording

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/api/record` | Upload session recording chunks |
| PUT | `/api/audit` | Update audit note |

### Web Console Routes (HTML)

All return server-rendered HTML. HTMX handles partial page updates.

```
GET  /console/login              Login form (standalone, no sidebar)
POST /console/login              Form submit
GET  /console/logout             Logout and redirect
GET  /console/                   Dashboard (online count, recent activity, stats)
GET  /console/users              User list with search and pagination
GET  /console/users/{id}         User detail/edit form
GET  /console/devices            Device list with search, group filter, status
GET  /console/devices/{id}       Device detail (info, group, ID change, policy)
GET  /console/groups             Device group management
GET  /console/groups/{id}        Group detail (members, policies)
GET  /console/address-books      Address book listing
GET  /console/address-books/{guid}  AB detail (peers, tags, sharing)
GET  /console/audit              Audit log with filters
GET  /console/settings           Global and per-group policy editor
GET  /console/settings/oidc      OIDC provider configuration
GET  /console/recordings         Session recording browser
GET  /console/setup              First-run admin setup (only accessible when no users exist)
POST /console/setup              Create initial admin account
```

### Middleware Stack

Applied in order via Tower layers:

1. **Tracing** - Request/response logging
2. **CORS** - For client API cross-origin requests
3. **Rate limiting** - tower-governor, per-IP
4. **Auth extraction:**
   - `/api/*` routes: Bearer token from `Authorization` header
   - `/console/*` routes: Session cookie (HttpOnly, Secure)
   - Public routes (no auth): `/api/login`, `/api/login-options`, `/api/oidc/auth`, `/api/oidc/callback`, `/api/heartbeat`, `/api/sysinfo`, `/api/sysinfo_ver`, `/console/login`, `/console/setup`
5. **Permission check** (Phase 3): Role-based, checks user permissions against route

## HTMX Interaction Patterns

### Page Navigation

Sidebar links use `hx-boost="true"` to swap main content area without full reload.

### Tables (Users, Devices, Audit)

All list pages follow the same pattern:

- Search input with `hx-get` and `hx-trigger="keyup changed delay:300ms"` for debounced server-side filtering
- Server returns `<tbody>` fragments for HTMX partial swap
- Pagination controls at bottom with `hx-get="?page=N"` targeting the table container

### Inline Editing

Fields like device notes, user roles, and group assignments use click-to-edit:

- Display element has `hx-get="/console/.../edit-field"` to swap into a form input
- Form POSTs on submit, server returns the updated display element

### Delete Operations

All deletes use `hx-delete` with `hx-confirm` for browser-native confirmation dialog. Target is `closest tr` with a swap transition.

### Dashboard Live Updates

Online device count polls with `hx-trigger="every 10s"` for near-realtime status without websockets.

### Settings Policy Editor

Key-value form for JSONB config options. "Add option" button uses `hx-get` to append a new input row. Save POSTs the entire form.

### Template Hierarchy

```
layout.html                   Base shell (sidebar, topbar, HTMX/CSS includes)
  console/dashboard.html      Stats cards, recent activity, online count
  console/login.html          Standalone login (no sidebar)
  console/users/
    list.html                 Table with search, pagination, role badges
    detail.html               Edit form, role assignment, device list
    _row.html                 Partial for single table row swap
  console/devices/
    list.html                 Table with search, group filter, status indicators
    detail.html               Device info, group, ID change, policy override
    _row.html
  console/groups/
    list.html                 Group cards or table
    detail.html               Members, policy assignment
  console/address_books/
    list.html                 Personal + shared address books
    detail.html               Peer list, tag management, share rules
  console/audit/
    list.html                 Filterable log (date range, user, action type)
  console/settings/
    global.html               Global policy editor
    oidc.html                 OIDC provider config form
```

## Authentication Flows

### Local Login (Client)

1. Client POSTs `{username, password, id, uuid}` to `/api/login`
2. Server verifies password against argon2 hash
3. If 2FA enabled: returns `{type: "2fa", tfa_type: "totp"}`, client re-submits with TOTP code
4. On success: generates random 256-bit hex access token, inserts into `sessions` table
5. Returns `AuthBody`: `{type: "access_token", access_token, user: UserPayload}` where `UserPayload` includes `{name, display_name, email, avatar, note, status, is_admin, info: {settings, login_device_whitelist}, third_auth_type}`
6. Client checks `type == "access_token"` to confirm success, stores token, sends as `Authorization: Bearer {token}` on all requests

### Local Login (Web Console)

1. User submits HTML form to `POST /console/login`
2. Same password verification
3. On success: sets HttpOnly Secure cookie with session ID
4. Redirects to `/console/`

### OIDC Flow (Phase 3)

1. Admin configures provider via `/console/settings/oidc` (client_id, client_secret, issuer_url)
2. `GET /api/login-options` returns enabled provider names
3. Client POSTs to `/api/oidc/auth` with `{op, id, uuid, deviceInfo: {os, type, name}}`
4. Server generates state/nonce, stores keyed by a generated `code`, returns `{code, url}`
5. Client opens `url` in system browser; user authenticates with provider
6. Provider redirects to `/api/oidc/callback?code=...&state=...`
7. Server exchanges authorization code for tokens, extracts subject/email from ID token
8. Finds or creates user matched by `(oidc_provider, oidc_subject)`
9. Creates session, stores result keyed by the `code` from step 4
10. Client polls `/api/oidc/auth-query?code=...&id=...&uuid=...` until session is ready

### Token Management

- Access tokens: random 256-bit hex, stored hashed in `sessions`
- Default expiry: 7 days (configurable)
- Background task: runs hourly, deletes expired sessions
- Logout: deletes session row from both API token and console cookie paths

### Permission Model (Phase 3)

Default roles seeded on first run:

| Role | Permissions |
|------|-------------|
| admin | All permissions |
| operator | devices.*, groups.*, ab.*, settings.read |
| viewer | *.read only |

Middleware checks `user -> user_roles -> roles -> permissions JSONB` against the required permission for each route.

### First-Run Setup

1. Server starts, checks if `users` table is empty
2. If empty: all `/console/*` routes redirect to `/console/setup`
3. Setup page collects admin username, password, and base config (hbbs URL, listen address)
4. Creates admin user with `is_admin = true`
5. Redirects to `/console/login`
6. `/console/setup` returns 404 once any user exists
