-- A score submitted since boards existed keeps its glue and exact board only
-- through board_token, and once glue_recorded is gone it would pass for a
-- legacy score; neither is given up for a downgrade. A legacy score's board
-- is its arrangement's glue-free code, which the up migration finds again.
-- Every board row stays, so every short link keeps working.
--
-- Locked before the check, in the app's order, so no score can be recorded
-- between the check and the column drops; the locks hold until the
-- migration commits.
LOCK TABLE board_states, scores IN ACCESS EXCLUSIVE MODE;
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM scores WHERE glue_recorded) THEN
        RAISE EXCEPTION 'scores with recorded boards exist; refusing to drop their board links and glue';
    END IF;
END
$$;
ALTER TABLE scores DROP COLUMN board_token;
ALTER TABLE scores DROP COLUMN glue_recorded;
ALTER INDEX board_states_created_by_idx RENAME TO solution_shares_created_by_idx;
ALTER TABLE board_states RENAME CONSTRAINT board_states_created_by_fkey TO solution_shares_created_by_fkey;
ALTER TABLE board_states RENAME CONSTRAINT board_states_code_check TO solution_shares_code_check;
ALTER TABLE board_states RENAME CONSTRAINT board_states_n_check TO solution_shares_n_check;
ALTER TABLE board_states RENAME CONSTRAINT board_states_payload_hash_check TO solution_shares_payload_hash_check;
ALTER TABLE board_states RENAME CONSTRAINT board_states_token_check TO solution_shares_token_check;
ALTER TABLE board_states RENAME CONSTRAINT board_states_payload_hash_key TO solution_shares_payload_hash_key;
ALTER TABLE board_states RENAME CONSTRAINT board_states_pkey TO solution_shares_pkey;
ALTER TABLE board_states RENAME TO solution_shares;
