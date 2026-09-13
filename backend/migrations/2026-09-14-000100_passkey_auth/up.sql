-- Passkey accounts. Usernames are stored normalized (trimmed, ASCII
-- lowercase). Credited profiles share the namespace, so their names are
-- reserved by the same unique index.
CREATE TABLE users (
    id UUID PRIMARY KEY,
    username TEXT NOT NULL UNIQUE CHECK (username = lower(username)),
    kind TEXT NOT NULL CHECK (kind IN ('player', 'credited')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (kind <> 'player' OR username ~ '^[a-z0-9][a-z0-9_-]{2,23}$')
);

-- Credential IDs are globally unique: one credential belongs to one account.
CREATE TABLE passkeys (
    credential_id BYTEA PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    passkey JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at TIMESTAMPTZ
);
CREATE INDEX passkeys_user_id_idx ON passkeys (user_id);

-- Only SHA-256 of the session token is stored; the token lives in the cookie.
CREATE TABLE sessions (
    token_hash BYTEA PRIMARY KEY CHECK (octet_length(token_hash) = 32),
    user_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX sessions_user_id_idx ON sessions (user_id);
CREATE INDEX sessions_expires_at_idx ON sessions (expires_at);

-- Ownership for later; existing rows stay anonymous.
ALTER TABLE scores ADD COLUMN user_id UUID REFERENCES users (id) ON DELETE SET NULL;
ALTER TABLE solution_shares ADD COLUMN created_by UUID REFERENCES users (id) ON DELETE SET NULL;
-- Also keeps the ON DELETE SET NULL above from scanning either table.
CREATE INDEX scores_user_id_idx ON scores (user_id);
CREATE INDEX solution_shares_created_by_idx ON solution_shares (created_by);
