-- Board states: every stored scene, its arrangement and glue as a board code
-- (shared::board). Short links and scores both point at them, so the table
-- is no longer only for sharing. Tokens, codes and hashes are unchanged.
ALTER TABLE solution_shares RENAME TO board_states;
ALTER TABLE board_states RENAME CONSTRAINT solution_shares_pkey TO board_states_pkey;
ALTER TABLE board_states RENAME CONSTRAINT solution_shares_payload_hash_key TO board_states_payload_hash_key;
ALTER TABLE board_states RENAME CONSTRAINT solution_shares_token_check TO board_states_token_check;
ALTER TABLE board_states RENAME CONSTRAINT solution_shares_payload_hash_check TO board_states_payload_hash_check;
ALTER TABLE board_states RENAME CONSTRAINT solution_shares_n_check TO board_states_n_check;
ALTER TABLE board_states RENAME CONSTRAINT solution_shares_code_check TO board_states_code_check;
ALTER TABLE board_states RENAME CONSTRAINT solution_shares_created_by_fkey TO board_states_created_by_fkey;
ALTER INDEX solution_shares_created_by_idx RENAME TO board_states_created_by_idx;

-- Every existing score gets a board holding its stored arrangement. A score
-- whose arrangement disagrees with its own n or side, or that a board code
-- can't load (side in [1, 1000], centers within 1000), is never guessed at
-- or deleted.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM scores
        WHERE NOT CASE
            WHEN jsonb_typeof(arrangement -> 'n') = 'number'
                AND jsonb_typeof(arrangement -> 'side') = 'number'
                AND jsonb_typeof(arrangement -> 'squares') = 'array'
            THEN (arrangement ->> 'n')::numeric = n
                AND (arrangement ->> 'side')::float8 = side
                AND side BETWEEN 1 AND 1000
                AND jsonb_array_length(arrangement -> 'squares') = n
                AND NOT EXISTS (
                    SELECT 1 FROM jsonb_array_elements(arrangement -> 'squares') AS sq
                    WHERE CASE
                        WHEN jsonb_typeof(sq -> 'cx') = 'number'
                            AND jsonb_typeof(sq -> 'cy') = 'number'
                            AND jsonb_typeof(sq -> 'theta') = 'number'
                        THEN abs((sq ->> 'cx')::float8) > 1000
                            OR abs((sq ->> 'cy')::float8) > 1000
                        ELSE true
                    END
                )
            ELSE false
        END
    ) THEN
        RAISE EXCEPTION 'scores exist whose stored arrangement does not match their n and side, or does not fit a board code; refusing to guess their boards';
    END IF;
END
$$;

-- `bytes` as hex in reverse order: int2send and float8send are big-endian,
-- board codes little-endian.
CREATE FUNCTION pg_temp.le_hex(bytes bytea) RETURNS text
    LANGUAGE sql IMMUTABLE STRICT
    AS $$
        SELECT string_agg(encode(substring(bytes FROM i FOR 1), 'hex'), '' ORDER BY i DESC)
        FROM generate_series(1, length(bytes)) AS i
    $$;

-- The glue-free board code of a stored arrangement, byte for byte as
-- shared::board::encode writes it: version 1, n as u16, side, then cx, cy
-- and theta of each square, every float as its IEEE 754 bits. jsonb keeps
-- the decimal serde_json wrote, which float8 reads back correctly rounded,
-- so each value is the f64 the score was saved with. (jsonb has no negative
-- zero, so a -0.0 was already stored as 0.)
CREATE FUNCTION pg_temp.board_code(n integer, arrangement jsonb) RETURNS text
    LANGUAGE sql IMMUTABLE STRICT
    AS $$
        SELECT '01' || pg_temp.le_hex(int2send(n::int2))
            || pg_temp.le_hex(float8send((arrangement ->> 'side')::float8))
            || string_agg(
                pg_temp.le_hex(float8send((sq ->> 'cx')::float8))
                    || pg_temp.le_hex(float8send((sq ->> 'cy')::float8))
                    || pg_temp.le_hex(float8send((sq ->> 'theta')::float8)),
                '' ORDER BY i)
        FROM jsonb_array_elements(arrangement -> 'squares') WITH ORDINALITY AS e(sq, i)
    $$;

-- A token as boards::new_token makes one: the same 12 bytes of a random
-- UUID, which skip its version and variant bits.
CREATE FUNCTION pg_temp.new_token() RETURNS text
    LANGUAGE sql VOLATILE
    AS $$
        SELECT encode(substring(u FROM 1 FOR 6) || substring(u FROM 10 FOR 6), 'hex')
        FROM uuid_send(gen_random_uuid()) AS u
    $$;

-- Whether a score's glue was recorded is the score's own provenance, not
-- its board's: a legacy score and a glue-free submission can share a board.
-- Existing scores never recorded theirs; new ones always do.
ALTER TABLE scores ADD COLUMN glue_recorded BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE scores ALTER COLUMN glue_recorded SET DEFAULT true;

-- One board per distinct code. A code already stored, such as a share of the
-- same packing, is reused untouched, creator included. A new one is created
-- by the earliest score that has it.
INSERT INTO board_states (token, payload_hash, n, code, created_by)
SELECT pg_temp.new_token(), sha256(convert_to(code, 'UTF8')), n, code, user_id
FROM (
    SELECT DISTINCT ON (code) code, n, user_id
    FROM (
        SELECT pg_temp.board_code(n, arrangement) AS code, n, user_id, submitted_at, id
        FROM scores
    ) AS coded
    ORDER BY code, submitted_at, id
) AS first
WHERE NOT EXISTS (
    SELECT 1 FROM board_states AS b
    WHERE b.payload_hash = sha256(convert_to(first.code, 'UTF8'))
);

-- Each score references its board by token. Matching the code as well as
-- the hash leaves a (never seen) digest collision NULL, which SET NOT NULL
-- then refuses.
ALTER TABLE scores ADD COLUMN board_token TEXT REFERENCES board_states (token);
WITH coded AS (
    SELECT id, pg_temp.board_code(n, arrangement) AS code FROM scores
)
UPDATE scores AS s SET board_token = b.token
FROM coded AS c
JOIN board_states AS b
    ON b.payload_hash = sha256(convert_to(c.code, 'UTF8')) AND b.code = c.code
WHERE s.id = c.id;
ALTER TABLE scores ALTER COLUMN board_token SET NOT NULL;
CREATE INDEX scores_board_token_idx ON scores (board_token);

DROP FUNCTION pg_temp.new_token();
DROP FUNCTION pg_temp.board_code(integer, jsonb);
DROP FUNCTION pg_temp.le_hex(bytea);
