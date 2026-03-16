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
