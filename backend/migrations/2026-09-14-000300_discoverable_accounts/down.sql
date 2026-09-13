LOCK TABLE board_states, scores, users, passkeys, sessions IN ACCESS EXCLUSIVE MODE;
-- This schema reversal cannot restore the intentionally deleted accounts.
DO $$ BEGIN RAISE NOTICE 'Deleted player accounts, credentials and sessions are not restored'; END $$;
ALTER TABLE passkeys DROP COLUMN discoverable;
