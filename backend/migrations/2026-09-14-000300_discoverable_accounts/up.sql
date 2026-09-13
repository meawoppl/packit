-- Same order as board/score writes, followed by account writers. Acquire all
-- before deleting: an old registration either commits before this lock (and
-- is removed) or resumes afterwards and fails its missing-column insert.
LOCK TABLE board_states, scores, users, passkeys, sessions IN ACCESS EXCLUSIVE MODE;
DELETE FROM users WHERE kind = 'player';
-- No default: old application binaries cannot insert old-style credentials.
ALTER TABLE passkeys ADD COLUMN discoverable BOOLEAN NOT NULL;
