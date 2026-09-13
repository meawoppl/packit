-- Prints `empty` for a database with no tables at all, and `ready` for one
-- at exactly the migrations before boards that holds nothing but the
-- credited profiles those migrations seed. Raises for anything else.
DO $$
DECLARE
    history text;
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_class AS c JOIN pg_namespace AS n ON n.oid = c.relnamespace
        WHERE c.relkind IN ('r', 'p', 'v', 'm', 'S')
            AND n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
            AND n.nspname NOT LIKE 'pg_temp%'
    ) THEN
        RETURN;
    END IF;
    IF to_regclass('public.__diesel_schema_migrations') IS NULL THEN
        RAISE EXCEPTION 'FAIL: the rehearsal database has tables but no migration history; use an empty database';
    END IF;
    SELECT string_agg(version, ',' ORDER BY version) INTO history
    FROM public.__diesel_schema_migrations;
    IF history IS DISTINCT FROM '00000000000000,20260913000000,20260914000000,20260914000100' THEN
        RAISE EXCEPTION 'FAIL: the rehearsal database is at migrations %, not the four before boards', history;
    END IF;
    IF to_regnamespace('rehearsal') IS NOT NULL THEN
        RAISE EXCEPTION 'FAIL: the rehearsal database was rehearsed on before; use a fresh one';
    END IF;
    IF EXISTS (SELECT 1 FROM public.scores)
        OR EXISTS (SELECT 1 FROM public.solution_shares)
        OR EXISTS (SELECT 1 FROM public.passkeys)
        OR EXISTS (SELECT 1 FROM public.sessions)
        OR EXISTS (SELECT 1 FROM public.users WHERE kind <> 'credited')
    THEN
        RAISE EXCEPTION 'FAIL: the rehearsal database holds data; use a fresh throwaway database';
    END IF;
END
$$;
SELECT CASE WHEN to_regclass('public.__diesel_schema_migrations') IS NULL THEN 'empty' ELSE 'ready' END;
