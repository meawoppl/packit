-- What the checks compare against, captured before migrating: every score
-- with its rank in the app's order (smaller side, then earlier submission,
-- then id), and every share link.
CREATE TABLE rehearsal.scores AS
    SELECT id, player, n, side, arrangement, submitted_at, user_id,
        row_number() OVER (PARTITION BY n ORDER BY side, submitted_at, id) AS rank
    FROM scores;
CREATE TABLE rehearsal.shares AS
    SELECT token, payload_hash, n, code, created_at, created_by FROM solution_shares;

SELECT format(
    'baseline: %s scores over %s values of n; %s share links, %s without a creator',
    (SELECT count(*) FROM rehearsal.scores),
    (SELECT count(DISTINCT n) FROM rehearsal.scores),
    (SELECT count(*) FROM rehearsal.shares),
    (SELECT count(*) FROM rehearsal.shares WHERE created_by IS NULL)
);
