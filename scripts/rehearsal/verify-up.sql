-- Checks after the board migration, against the baseline. Read-only: each
-- check raises on failure (so psql -v ON_ERROR_STOP=1 exits nonzero) and
-- prints PASS otherwise.

DO $$
DECLARE
    before bigint := (SELECT count(*) FROM rehearsal.scores);
    after bigint := (SELECT count(*) FROM scores);
BEGIN
    IF before <> after THEN
        RAISE EXCEPTION 'FAIL score count: % before, % after', before, after;
    END IF;
    IF EXISTS (
        SELECT 1 FROM rehearsal.scores AS b LEFT JOIN scores AS s USING (id)
        WHERE s.id IS NULL
            OR (s.player, s.n, s.side, s.arrangement, s.submitted_at, s.user_id)
                IS DISTINCT FROM (b.player, b.n, b.side, b.arrangement, b.submitted_at, b.user_id)
    ) THEN
        RAISE EXCEPTION 'FAIL a score changed or went missing';
    END IF;
    RAISE NOTICE 'PASS score count unchanged (%), every score as it was', after;
END
$$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM scores AS s LEFT JOIN board_states AS b ON b.token = s.board_token
        WHERE b.token IS NULL
    ) THEN
        RAISE EXCEPTION 'FAIL a score has no board_token, or one that resolves to no board';
    END IF;
    RAISE NOTICE 'PASS every score has a board_token that resolves to a board';
END
$$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM rehearsal.shares AS o LEFT JOIN board_states AS b USING (token)
        WHERE b.token IS NULL
            OR (b.payload_hash, b.n, b.code, b.created_at, b.created_by)
                IS DISTINCT FROM (o.payload_hash, o.n, o.code, o.created_at, o.created_by)
    ) THEN
        RAISE EXCEPTION 'FAIL a share link''s token, code or creator changed';
    END IF;
    RAISE NOTICE 'PASS all % share links kept their token, code and creator (% without one)',
        (SELECT count(*) FROM rehearsal.shares),
        (SELECT count(*) FROM rehearsal.shares WHERE created_by IS NULL);
END
$$;

DO $$
DECLARE
    new_boards bigint := (
        SELECT count(*) FROM board_states AS b
        WHERE NOT EXISTS (SELECT 1 FROM rehearsal.shares AS o WHERE o.token = b.token)
    );
BEGIN
    IF EXISTS (
        SELECT 1 FROM board_states AS b
        WHERE NOT EXISTS (SELECT 1 FROM rehearsal.shares AS o WHERE o.token = b.token)
            AND NOT EXISTS (SELECT 1 FROM scores AS s WHERE s.board_token = b.token)
    ) THEN
        RAISE EXCEPTION 'FAIL a new board belongs to no score';
    END IF;
    IF EXISTS (
        SELECT 1 FROM board_states AS b
        WHERE NOT EXISTS (SELECT 1 FROM rehearsal.shares AS o WHERE o.token = b.token)
            AND b.created_by IS DISTINCT FROM (
                SELECT s.user_id FROM scores AS s WHERE s.board_token = b.token
                ORDER BY s.submitted_at, s.id LIMIT 1
            )
    ) THEN
        RAISE EXCEPTION 'FAIL a new board''s creator is not its earliest score''s user';
    END IF;
    RAISE NOTICE 'PASS each of % new boards belongs to scores and has its earliest score''s user as creator',
        new_boards;
END
$$;

-- Each score's board code, read back field by field: the header, then side
-- and every cx, cy and theta as little-endian IEEE 754 bits, against the
-- bits of the stored arrangement's numbers. Failures give counts only, so
-- no snapshot data reaches a log.
DO $$
DECLARE
    bad bigint;
BEGIN
    SELECT count(*) INTO bad
    FROM scores AS s JOIN board_states AS b ON b.token = s.board_token
    WHERE length(b.code) <> 22 + 48 * s.n
        OR substr(b.code, 1, 2) <> '01'
        OR substr(b.code, 3, 4) <> substr(encode(int2send(s.n::int2), 'hex'), 3, 2)
            || substr(encode(int2send(s.n::int2), 'hex'), 1, 2)
        OR b.payload_hash <> sha256(convert_to(b.code, 'UTF8'));
    IF bad > 0 THEN
        RAISE EXCEPTION 'FAIL % scores'' board codes have the wrong header, length or hash', bad;
    END IF;
    WITH coded AS (
        SELECT s.id, s.arrangement, b.code
        FROM scores AS s JOIN board_states AS b ON b.token = s.board_token
    ), fields AS (
        SELECT id, 'side' AS field, substr(code, 7, 16) AS le, (arrangement ->> 'side')::float8 AS value
        FROM coded
        UNION ALL
        SELECT c.id, format('square %s %s', e.i - 1, f.name),
            substr(c.code, (23 + 48 * (e.i - 1) + f.offset_)::int, 16),
            (e.sq ->> f.name)::float8
        FROM coded AS c
        CROSS JOIN LATERAL jsonb_array_elements(c.arrangement -> 'squares') WITH ORDINALITY AS e (sq, i)
        CROSS JOIN (VALUES ('cx', 0), ('cy', 16), ('theta', 32)) AS f (name, offset_)
    )
    SELECT count(*) INTO bad FROM fields
    WHERE (SELECT string_agg(substr(le, 15 - 2 * k, 2), '' ORDER BY k) FROM generate_series(0, 7) AS k)
        IS DISTINCT FROM encode(float8send(value), 'hex');
    IF bad > 0 THEN
        RAISE EXCEPTION 'FAIL % fields of board codes differ from the stored arrangements', bad;
    END IF;
    RAISE NOTICE 'PASS every score''s board code decodes to exactly its stored arrangement';
END
$$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM (
            SELECT id, row_number() OVER (PARTITION BY n ORDER BY side, submitted_at, id) AS rank
            FROM scores
        ) AS now
        JOIN rehearsal.scores AS b USING (id)
        WHERE now.rank <> b.rank
    ) THEN
        RAISE EXCEPTION 'FAIL the rank order of some n changed';
    END IF;
    RAISE NOTICE 'PASS rank order identical for every n';
END
$$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM scores AS s JOIN rehearsal.scores USING (id) WHERE s.glue_recorded) THEN
        RAISE EXCEPTION 'FAIL a pre-existing score claims its glue was recorded';
    END IF;
    RAISE NOTICE 'PASS glue_recorded is false for every pre-existing score';
END
$$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM board_states GROUP BY payload_hash HAVING count(*) > 1) THEN
        RAISE EXCEPTION 'FAIL duplicate payload_hash rows';
    END IF;
    IF EXISTS (SELECT 1 FROM board_states WHERE token !~ '^[0-9a-f]{24}$') THEN
        RAISE EXCEPTION 'FAIL a board token is not 24 lowercase hex digits';
    END IF;
    RAISE NOTICE 'PASS no duplicate payload_hash rows among % boards', (SELECT count(*) FROM board_states);
END
$$;
