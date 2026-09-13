-- Sign-in data is never dropped to make room for a downgrade. Only the
-- credited profiles this migration seeded may go with the tables.
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM users WHERE kind = 'player')
        OR EXISTS (SELECT 1 FROM passkeys)
        OR EXISTS (SELECT 1 FROM sessions)
        OR EXISTS (SELECT 1 FROM scores WHERE user_id IS NOT NULL)
        OR EXISTS (SELECT 1 FROM solution_shares WHERE created_by IS NOT NULL)
    THEN
        RAISE EXCEPTION 'players, passkeys, sessions or score/share attribution exist; refusing to drop sign-in data';
    END IF;
END
$$;
ALTER TABLE solution_shares DROP COLUMN created_by;
ALTER TABLE scores DROP COLUMN user_id;
DROP TABLE sessions;
DROP TABLE passkeys;
DROP TABLE users;
