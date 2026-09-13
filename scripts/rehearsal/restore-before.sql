-- The snapshot carries no users, so the ownership foreign keys come off
-- while its rows load. restore-after.sql puts them back exactly as they
-- were, once every owner has a stub user.
CREATE SCHEMA rehearsal;
CREATE TABLE rehearsal.foreign_keys AS
    SELECT format('%I.%I', n.nspname, c.relname) AS tbl, k.conname,
        pg_get_constraintdef(k.oid) AS def
    FROM pg_constraint AS k
    JOIN pg_class AS c ON c.oid = k.conrelid
    JOIN pg_namespace AS n ON n.oid = c.relnamespace
    WHERE k.contype = 'f' AND n.nspname = 'public'
        AND c.relname IN ('scores', 'solution_shares');
DO $$
DECLARE
    fk record;
BEGIN
    IF (SELECT count(*) FROM rehearsal.foreign_keys) <> 2 THEN
        RAISE EXCEPTION 'FAIL: expected the two ownership foreign keys of the pre-board schema';
    END IF;
    FOR fk IN SELECT * FROM rehearsal.foreign_keys LOOP
        EXECUTE format('ALTER TABLE %s DROP CONSTRAINT %I', fk.tbl, fk.conname);
    END LOOP;
END
$$;
