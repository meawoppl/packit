-- Checks after migrating again: every score is back on the same board, and
-- the refused revert left nothing behind. Read-only; each check raises on
-- failure and prints PASS otherwise.

DO $$
BEGIN
    IF (SELECT count(*) FROM board_states) <> (SELECT count(*) FROM rehearsal.boards)
        OR EXISTS (
            SELECT 1 FROM rehearsal.boards AS o LEFT JOIN board_states AS b USING (token)
            WHERE b.token IS NULL
                OR (b.payload_hash, b.n, b.code, b.created_at, b.created_by)
                    IS DISTINCT FROM (o.payload_hash, o.n, o.code, o.created_at, o.created_by)
        )
    THEN
        RAISE EXCEPTION 'FAIL migrating again changed the boards';
    END IF;
    IF (SELECT count(*) FROM scores) <> (SELECT count(*) FROM rehearsal.links)
        OR EXISTS (
            SELECT 1 FROM rehearsal.links AS l LEFT JOIN scores AS s USING (id)
            WHERE s.board_token IS DISTINCT FROM l.board_token OR s.glue_recorded
        )
    THEN
        RAISE EXCEPTION 'FAIL migrating again linked a score to another board, or left a score behind';
    END IF;
    RAISE NOTICE 'PASS migrating again links all % scores to the same boards, and nothing was left behind',
        (SELECT count(*) FROM scores);
END
$$;
