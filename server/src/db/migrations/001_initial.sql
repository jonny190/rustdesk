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
