-- Checks after reverting the board migration. Read-only; each check raises
-- on failure and prints PASS otherwise.

DO $$
BEGIN
    IF (SELECT string_agg(version, ',' ORDER BY version) FROM __diesel_schema_migrations)
        IS DISTINCT FROM '00000000000000,20260913000000,20260914000000,20260914000100'
    THEN
        RAISE EXCEPTION 'FAIL the migration history is not back to passkey_auth';
    END IF;
    IF to_regclass('public.board_states') IS NOT NULL OR to_regclass('public.solution_shares') IS NULL THEN
        RAISE EXCEPTION 'FAIL board_states was not renamed back to solution_shares';
    END IF;
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'scores'
            AND column_name IN ('board_token', 'glue_recorded')
    ) THEN
        RAISE EXCEPTION 'FAIL scores still has board columns';
    END IF;
    IF (
        SELECT count(*) FROM pg_constraint
        WHERE conrelid = 'public.solution_shares'::regclass AND conname LIKE 'solution\_shares\_%'
    ) <> 7 THEN
        RAISE EXCEPTION 'FAIL solution_shares did not get its seven constraint names back';
    END IF;
    RAISE NOTICE 'PASS reverted to passkey_auth: solution_shares, its constraints and the score columns as before';
END
$$;

DO $$
BEGIN
    IF (SELECT count(*) FROM solution_shares) <> (SELECT count(*) FROM rehearsal.boards)
        OR EXISTS (
            SELECT 1 FROM rehearsal.boards AS b LEFT JOIN solution_shares AS s USING (token)
            WHERE s.token IS NULL
                OR (s.payload_hash, s.n, s.code, s.created_at, s.created_by)
                    IS DISTINCT FROM (b.payload_hash, b.n, b.code, b.created_at, b.created_by)
        )
    THEN
        RAISE EXCEPTION 'FAIL a board''s token, code or creator did not survive the revert';
    END IF;
    RAISE NOTICE 'PASS all % boards survive in solution_shares with their tokens, codes and creators',
        (SELECT count(*) FROM solution_shares);
END
$$;

DO $$
BEGIN
    IF (SELECT count(*) FROM scores) <> (SELECT count(*) FROM rehearsal.scores)
        OR EXISTS (
            SELECT 1 FROM rehearsal.scores AS b LEFT JOIN scores AS s USING (id)
            WHERE s.id IS NULL
                OR (s.player, s.n, s.side, s.arrangement, s.submitted_at, s.user_id)
                    IS DISTINCT FROM (b.player, b.n, b.side, b.arrangement, b.submitted_at, b.user_id)
        )
    THEN
        RAISE EXCEPTION 'FAIL a score changed or went missing in the revert';
    END IF;
    RAISE NOTICE 'PASS every score as it was';
END
$$;
