-- Share snapshots are intentionally separate from validated leaderboard scores.
CREATE TABLE solution_shares (
    token TEXT PRIMARY KEY CHECK (token ~ '^[0-9a-f]{24}$'),
    payload_hash BYTEA NOT NULL UNIQUE CHECK (octet_length(payload_hash) = 32),
    n INTEGER NOT NULL CHECK (n BETWEEN 1 AND 100),
    code TEXT NOT NULL CHECK (length(code) = 22 + 48 * n AND code ~ '^[0-9a-f]+$'),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
