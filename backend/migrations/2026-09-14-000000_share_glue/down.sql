-- Glued snapshots can't satisfy the original constraint.
DELETE FROM solution_shares WHERE length(code) <> 22 + 48 * n;
ALTER TABLE solution_shares DROP CONSTRAINT solution_shares_code_check;
ALTER TABLE solution_shares ADD CONSTRAINT solution_shares_check
    CHECK (length(code) = 22 + 48 * n AND code ~ '^[0-9a-f]+$');
