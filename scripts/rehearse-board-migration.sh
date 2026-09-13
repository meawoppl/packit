#!/usr/bin/env bash
#
# Rehearses the board_states migration (2026-09-14-000200) on a minimal
# snapshot of a live database, before it deploys.
#
#   SOURCE_DATABASE_URL     database to snapshot; only read, by one pg_dump
#   REHEARSAL_DATABASE_URL  throwaway database: empty, or at the migrations
#                           before boards with no scores, links or accounts
#   REHEARSAL_DIR           optional: keep the snapshot here (it holds the
#                           source's scores and share links; delete it after)
#
# Needs psql, pg_dump and pg_restore. Every check raises on failure, so the
# script exits nonzero on the first one; each prints PASS as it succeeds.
# The rehearsal database is left migrated, holding the snapshot.

set -euo pipefail

: "${SOURCE_DATABASE_URL:?SOURCE_DATABASE_URL must be set}"
: "${REHEARSAL_DATABASE_URL:?REHEARSAL_DATABASE_URL must be set}"
if [[ "$SOURCE_DATABASE_URL" == "$REHEARSAL_DATABASE_URL" ]]; then
    echo "FAIL: the rehearsal database must not be the source" >&2
    exit 1
fi

here=$(cd "$(dirname "$0")" && pwd)
sql=$here/rehearsal
migrations=$here/../backend/migrations
board=2026-09-14-000200_board_states
before_boards=(
    00000000000000_initial
    2026-09-13-000000_solution_shares
    2026-09-14-000000_share_glue
    2026-09-14-000100_passkey_auth
)
expected_history=00000000000000,20260913000000,20260914000000,20260914000100

if [[ -n "${REHEARSAL_DIR:-}" ]]; then
    dir=$REHEARSAL_DIR
    mkdir -p "$dir"
else
    dir=$(mktemp -d)
    trap 'rm -rf "$dir"' EXIT
fi
snapshot=$dir/snapshot.dump

say() { printf '\n== %s\n' "$*"; }
rehearsal() { psql "$REHEARSAL_DATABASE_URL" -X -q -v ON_ERROR_STOP=1 "$@"; }
# Checks run in read-only sessions, so they can't change what they check.
check() { PGOPTIONS='-c default_transaction_read_only=on' rehearsal "$@"; }
version() { local v=${1%%_*}; echo "${v//-/}"; }
# Apply a migration as diesel does: its up.sql and its version, in one
# transaction.
up() {
    rehearsal -1 -f "$migrations/$1/up.sql" \
        -c "INSERT INTO __diesel_schema_migrations (version) VALUES ('$(version "$1")')"
}
down() {
    rehearsal -1 -f "$migrations/$1/down.sql" \
        -c "DELETE FROM __diesel_schema_migrations WHERE version = '$(version "$1")'"
}

say "Rehearsal database"
state=$(check -At -f "$sql/target-state.sql")
if [[ "$state" == empty ]]; then
    echo "empty: applying the migrations before boards"
    rehearsal -c "CREATE TABLE __diesel_schema_migrations (
        version VARCHAR(50) PRIMARY KEY NOT NULL,
        run_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP)"
    for m in "${before_boards[@]}"; do
        up "$m"
    done
    state=$(check -At -f "$sql/target-state.sql")
fi
[[ "$state" == ready ]]
echo "PASS at the migrations before boards, with no data"

say "Snapshot: scores, solution_shares and __diesel_schema_migrations, in one pg_dump"
pg_dump "$SOURCE_DATABASE_URL" --format=custom --no-owner --no-privileges \
    -t public.scores -t public.solution_shares -t public.__diesel_schema_migrations \
    -f "$snapshot"
history=$(pg_restore --data-only -t __diesel_schema_migrations -f - "$snapshot" |
    awk '/^COPY public.__diesel_schema_migrations /{rows=1; next} /^\\\.$/{rows=0} rows{print $1}' |
    sort | paste -sd, -)
if [[ "$history" != "$expected_history" ]]; then
    echo "FAIL: the source is at migrations $history, not $expected_history" >&2
    exit 1
fi
echo "PASS the source is at the migrations before boards"

say "Restore, with a stub user for every owner"
rehearsal -1 -f "$sql/restore-before.sql"
pg_restore --data-only --single-transaction --exit-on-error \
    -t scores -t solution_shares -d "$REHEARSAL_DATABASE_URL" "$snapshot"
rehearsal -1 -At -f "$sql/restore-after.sql"

say "Baseline"
rehearsal -1 -At -f "$sql/baseline.sql"

say "Migrate: $board"
up "$board"

say "Verify the upgrade"
check -f "$sql/verify-up.sql"
rehearsal -1 -c "CREATE TABLE rehearsal.boards AS
        SELECT token, payload_hash, n, code, created_at, created_by FROM board_states;
    CREATE TABLE rehearsal.links AS SELECT id, board_token FROM scores;"

say "Revert: $board"
down "$board"
check -f "$sql/verify-down.sql"

say "Migrate again"
up "$board"
# A score with a recorded board, in a transaction that is never committed:
# reverting must refuse rather than drop its link.
if refusal=$(rehearsal -1 -c "INSERT INTO scores (player, n, side, arrangement, board_token)
        SELECT 'rehearsal', n, side, arrangement, board_token FROM scores LIMIT 1" \
    -f "$migrations/$board/down.sql" 2>&1); then
    if [[ $(check -At -c "SELECT count(*) FROM scores") != 0 ]]; then
        echo "FAIL: reverting went through with a recorded board" >&2
        exit 1
    fi
    echo "(no scores, so there is no recorded board to refuse over)"
elif grep -q "refusing to drop their board links and glue" <<<"$refusal"; then
    echo "PASS reverting refuses while a score has a recorded board"
else
    echo "FAIL: reverting failed for another reason: $refusal" >&2
    exit 1
fi
check -f "$sql/verify-relink.sql"

say "Rehearsal passed"
