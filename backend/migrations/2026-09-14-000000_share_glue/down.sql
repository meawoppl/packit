-- Glued snapshots can't satisfy the original constraint, and are never
-- deleted to make room for it.
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM solution_shares WHERE length(code) <> 22 + 48 * n) THEN
        RAISE EXCEPTION 'glued share codes exist; cannot restore the glue-free length constraint';
    END IF;
END
$$;
ALTER TABLE solution_shares DROP CONSTRAINT solution_shares_code_check;
ALTER TABLE solution_shares ADD CONSTRAINT solution_shares_check
    CHECK (length(code) = 22 + 48 * n AND code ~ '^[0-9a-f]+$');
