-- Passkey accounts. Usernames are stored normalized (trimmed, ASCII
-- lowercase). Credited profiles share the namespace, so their names are
-- reserved by the same unique index.
CREATE TABLE users (
    id UUID PRIMARY KEY,
    username TEXT NOT NULL UNIQUE CHECK (username = lower(username)),
    kind TEXT NOT NULL CHECK (kind IN ('player', 'credited')),
    -- The full name of a credited profile.
    display_name TEXT,
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

-- Credited profiles: everyone named in refs/credits.json, seeded with the
-- tables so nobody can register their names first. They have no passkeys
-- and can't sign in. The username is the ASCII-folded surname (ö -> oe,
-- ü -> ue, ä -> ae, other diacritics dropped), with "-" and the first
-- initial only when two people share a surname. A backend test checks this
-- list against credits.json.
INSERT INTO users (id, username, kind, display_name) VALUES
    (gen_random_uuid(), 'bentz', 'credited', 'Wolfram Bentz'),
    (gen_random_uuid(), 'bidwell', 'credited', 'John Bidwell'),
    (gen_random_uuid(), 'brendberg', 'credited', 'Sigvart Brendberg'),
    (gen_random_uuid(), 'cantrell', 'credited', 'David W. Cantrell'),
    (gen_random_uuid(), 'chang', 'credited', 'Allen Chang'),
    (gen_random_uuid(), 'cottingham', 'credited', 'Charles F. Cottingham'),
    (gen_random_uuid(), 'devincentis', 'credited', 'Joe DeVincentis'),
    (gen_random_uuid(), 'ellsworth', 'credited', 'David Ellsworth'),
    (gen_random_uuid(), 'friedman', 'credited', 'Erich Friedman'),
    (gen_random_uuid(), 'gensane', 'credited', 'Thierry Gensane'),
    (gen_random_uuid(), 'goebel', 'credited', 'Frits Göbel'),
    (gen_random_uuid(), 'gustafsson', 'credited', 'Mats Gustafsson'),
    (gen_random_uuid(), 'haemaelaeinen', 'credited', 'Pertti Hämäläinen'),
    (gen_random_uuid(), 'hajba', 'credited', 'Károly Hajba'),
    (gen_random_uuid(), 'hmbelvedere', 'credited', 'hmbelvedere'),
    (gen_random_uuid(), 'kearney', 'credited', 'Michael Kearney'),
    (gen_random_uuid(), 'morandi', 'credited', 'Maurizio Morandi'),
    (gen_random_uuid(), 'moumni', 'credited', 'Said El Moumni'),
    (gen_random_uuid(), 'nagamochi', 'credited', 'Hiroshi Nagamochi'),
    (gen_random_uuid(), 'ryckelynck', 'credited', 'Philippe Ryckelynck'),
    (gen_random_uuid(), 'schadt', 'credited', 'Thomas Schadt'),
    (gen_random_uuid(), 'shiu', 'credited', 'Peter Shiu'),
    (gen_random_uuid(), 'stenlund', 'credited', 'Evert Stenlund'),
    (gen_random_uuid(), 'stromquist', 'credited', 'Walter Stromquist'),
    (gen_random_uuid(), 'themagicanimals', 'credited', 'TheMagicAnimals'),
    (gen_random_uuid(), 'trump', 'credited', 'Walter Trump'),
    (gen_random_uuid(), 'wainwright', 'credited', 'Robert Wainwright'),
    (gen_random_uuid(), 'winter', 'credited', 'Joost de Winter');
