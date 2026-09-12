CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

CREATE TABLE scores (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    player TEXT NOT NULL,
    n INTEGER NOT NULL CHECK (n BETWEEN 1 AND 100),
    side DOUBLE PRECISION NOT NULL CHECK (side > 0),
    arrangement JSONB NOT NULL,
    submitted_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX scores_n_side_idx ON scores (n, side, submitted_at);
