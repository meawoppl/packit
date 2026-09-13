-- Share codes may end in a glue trailer: after the 22 + 48 * n hex digits of
-- the glue-free layout, an 8-digit header, then 16 digits for each of 1 to
-- 4096 glues.
ALTER TABLE solution_shares DROP CONSTRAINT solution_shares_check;
ALTER TABLE solution_shares ADD CONSTRAINT solution_shares_code_check CHECK (
    code ~ '^[0-9a-f]+$'
    AND (
        length(code) = 22 + 48 * n
        OR (
            length(code) BETWEEN 22 + 48 * n + 8 + 16 AND 22 + 48 * n + 8 + 16 * 4096
            AND (length(code) - (22 + 48 * n + 8)) % 16 = 0
        )
    )
);
