-- A stub user for every owner the snapshot names, so the foreign keys hold
-- on the real UUIDs. Nothing about the accounts is copied: the usernames
-- are made up from the ids.
INSERT INTO users (id, username, kind)
SELECT owner, 'stub-' || substr(md5(owner::text), 1, 16), 'player'
FROM (
    SELECT user_id FROM scores
    UNION
    SELECT created_by FROM solution_shares
) AS owners (owner)
WHERE owner IS NOT NULL
ON CONFLICT (id) DO NOTHING;

DO $$
DECLARE
    fk record;
BEGIN
    FOR fk IN SELECT * FROM rehearsal.foreign_keys LOOP
        EXECUTE format('ALTER TABLE %s ADD CONSTRAINT %I %s', fk.tbl, fk.conname, fk.def);
    END LOOP;
END
$$;
DROP TABLE rehearsal.foreign_keys;

SELECT format(
    'restored %s scores and %s share links, with %s stub users',
    (SELECT count(*) FROM scores),
    (SELECT count(*) FROM solution_shares),
    (SELECT count(*) FROM users WHERE kind = 'player')
);
