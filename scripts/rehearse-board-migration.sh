#!/usr/bin/env bash
#
# Rehearses the board_states migration (2026-09-14-000200) on a minimal
# snapshot of a live database, before it deploys.
#
#   SOURCE_DATABASE_URL     database to snapshot; only read (one pg_dump and
#                           one identity query)
#   REHEARSAL_DATABASE_URL  throwaway database with a name distinct from the
#                           source's: empty, or at the migrations before
#                           boards with no scores, links or accounts
#   REHEARSAL_DIR           optional: keep the snapshot here, mode 0600 (it
#                           holds the source's scores and share links; delete
#                           it afterwards)
#
# Needs psql, pg_dump and pg_restore. Every check raises on failure, so the
# script exits nonzero on the first one; each prints PASS as it succeeds.
# Neither connection string nor any row of the snapshot is ever printed.
# The rehearsal database is left migrated, holding the snapshot.

set -euo pipefail
umask 077

: "${SOURCE_DATABASE_URL:?SOURCE_DATABASE_URL must be set}"
: "${REHEARSAL_DATABASE_URL:?REHEARSAL_DATABASE_URL must be set}"

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

say() { printf '\n== %s\n' "$*"; }
fail() {
    echo "FAIL: $*" >&2
    exit 1
}

# Everything the database tools print passes through here: the connection
# strings never reach a log, nor does a row the server quotes back.
redact() {
    local line
    while IFS= read -r line || [[ -n "$line" ]]; do
        line=${line//"$SOURCE_DATABASE_URL"/<SOURCE_DATABASE_URL>}
        line=${line//"$REHEARSAL_DATABASE_URL"/<REHEARSAL_DATABASE_URL>}
        case $line in
        *DETAIL:*) line="${line%%DETAIL:*}DETAIL:  (redacted)" ;;
        *"CONTEXT:  COPY"*) line="${line%%CONTEXT:*}CONTEXT:  COPY (row redacted)" ;;
        esac
        printf '%s\n' "$line"
    done
}
# Run a database tool with its messages redacted, keeping its exit status.
run() { { "$@" 2>&1 1>&3 3>&- | redact >&2; } 3>&1; }

rehearsal() { run psql "$REHEARSAL_DATABASE_URL" -X -q -v ON_ERROR_STOP=1 "$@"; }
# Checks run in read-only sessions, so they can't change what they check.
check() {
    run env PGOPTIONS='-c default_transaction_read_only=on' \
        psql "$REHEARSAL_DATABASE_URL" -X -q -v ON_ERROR_STOP=1 "$@"
}
version() {
    local v=${1%%_*}
    echo "${v//-/}"
}
# Apply a migration as diesel does: its SQL and its version, in one
# transaction.
up() {
    rehearsal -1 -f "$migrations/$1/up.sql" \
        -c "INSERT INTO __diesel_schema_migrations (version) VALUES ('$(version "$1")')"
}
down() {
    rehearsal -1 -f "$migrations/$1/down.sql" \
        -c "DELETE FROM __diesel_schema_migrations WHERE version = '$(version "$1")'"
}

say "Source and rehearsal databases"
identity="SELECT current_database(), coalesce(host(inet_server_addr()), 'socket'),
    coalesce(inet_server_port(), 0), pg_postmaster_start_time()"
source_id=$(run env PGOPTIONS='-c default_transaction_read_only=on' \
    psql "$SOURCE_DATABASE_URL" -X -q -At -v ON_ERROR_STOP=1 -c "$identity")
target_id=$(check -At -c "$identity")
# Aliases can name one database twice, so compare what the servers say. A
# fresh target with its own name is the contract; an equal name is refused
# even on another server.
if [[ "${source_id%%|*}" == "${target_id%%|*}" || "$source_id" == "$target_id" ]]; then
    fail "the rehearsal database has the source's name; use a fresh database with a distinct name"
fi
echo "PASS the rehearsal database is not the source"

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
[[ "$state" == ready ]] || fail "the rehearsal database is not ready"
echo "PASS at the migrations before boards, with no data"

if [[ -n "${REHEARSAL_DIR:-}" ]]; then
    dir=$REHEARSAL_DIR
    mkdir -p "$dir"
else
    dir=$(mktemp -d)
    trap 'rm -rf "$dir"' EXIT
fi
snapshot=$dir/snapshot.dump

say "Snapshot: scores, solution_shares and __diesel_schema_migrations, in one pg_dump"
run pg_dump "$SOURCE_DATABASE_URL" --format=custom --no-owner --no-privileges \
    -t public.scores -t public.solution_shares -t public.__diesel_schema_migrations \
    -f "$snapshot"
chmod 600 "$snapshot"
[[ -n $(find "$snapshot" -perm 600) ]] || fail "the snapshot is not private (mode 0600)"
history=$(run pg_restore --data-only -t __diesel_schema_migrations -f - "$snapshot" |
    awk '/^COPY public.__diesel_schema_migrations /{rows=1; next} /^\\\.$/{rows=0} rows{print $1}' |
    sort | paste -sd, -)
[[ "$history" == "$expected_history" ]] ||
    fail "the source is at migrations $history, not $expected_history"
echo "PASS the source is at the migrations before boards; the snapshot is mode 0600"

say "Restore, with a stub user for every owner"
rehearsal -1 -f "$sql/restore-before.sql"
run pg_restore --data-only --single-transaction --exit-on-error \
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
    [[ $(check -At -c "SELECT count(*) FROM scores") == 0 ]] ||
        fail "reverting went through with a recorded board"
    echo "(no scores, so there is no recorded board to refuse over)"
elif grep -q "refusing to drop their board links and glue" <<<"$refusal"; then
    echo "PASS reverting refuses while a score has a recorded board"
else
    fail "reverting failed for another reason: $refusal"
fi
check -f "$sql/verify-relink.sql"

say "Rehearsal passed"
