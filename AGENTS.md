# CLAUDE.md

## Crate Recommendations

### Static Asset Serving (Web Projects)

Use **`memory-serve`** for embedding and serving static frontend assets in axum web servers.

- Pre-compresses assets (brotli/gzip) at build time, zero CPU at startup
- Built-in content negotiation, ETag/304, cache-control headers, SPA fallback
- Replaces `rust-embed` + manual compression entirely

**Setup:**

```toml
[dependencies]
memory-serve = "2.1"

[build-dependencies]
memory-serve = "2.1"
```

```rust
// build.rs
fn main() {
    memory_serve::load_directory("./frontend/dist");
}
```

```rust
// main.rs
use memory_serve::CacheControl;

let frontend = memory_serve::load!()
    .index_file(Some("/index.html"))
    .fallback(Some("/index.html"))
    .fallback_status(axum::http::StatusCode::OK)
    .html_cache_control(CacheControl::NoCache)
    .cache_control(CacheControl::Long)
    .into_router();

let app = Router::new()
    // API routes first
    .route("/api/health", get(|| async { "ok" }))
    .with_state(app_state)
    .merge(frontend);
```

Note: memory-serve 2.x requires axum 0.8+. For axum 0.7, use memory-serve 0.6.0 (older `load_assets!` macro API).

## packit Conventions

- Every crate that parses f64s from JSON (backend, frontend, shared, solver) enables serde_json's `float_roundtrip`, so shortest-repr floats round-trip bit for bit; the default parser can be a ULP off. Add it with `cargo add serde_json@<locked version> -p <crate> --features float_roundtrip` (a bare `1` requirement gets "unrecognized feature") and check each crate on its own, e.g. `cargo tree -p frontend --target wasm32-unknown-unknown -e features -i serde_json`: workspace feature unification can hide a crate that lacks it. jsonb has no negative zero, so a stored `-0.0` reads back as `0.0`.
- Squares are **unit** squares: `shared::Placement { cx, cy, theta }` = center + rotation in radians. Container is `[0, side]^2`, origin bottom-left. Never introduce a second square type; `physics` and `solver` depend on `shared`.
- Server-side validation is `shared::geometry::validate(&arr, shared::VALIDATION_TOL)`. The play engine may allow small stiff overlaps for bounce, but submissions must pass validation.
- `refs/best_known.json` is compiled into the backend (`include_str!`) and served at `/api/records`. A unit test checks it parses, is sorted by `n`, and respects the `sqrt(n)` area bound.
- `rank` in `ScoreEntry` is computed at read time (smaller side first, earlier submission breaks ties), so a rank returned at submit time goes stale.
- Axum is 0.7: path params use `/:id`, not `/{id}`.
- Passkey auth lives in `backend/src/auth/` (ceremony store, rate limits, sessions, usernames) and `backend/src/handlers/auth.rs`. `PUBLIC_URL` is parsed once into `config::PublicOrigin` (RP ID, the only accepted `Origin`, cookie `Secure`). `/api/auth` has no CORS layer, needs an exact `Origin` on every POST, and never trusts a username or user id sent at finish.
- Signed-in writes (`POST /api/scores`, `POST /api/boards`) live in `build_app`'s `writes` router: `handlers::auth::require_origin` as a route layer, a `CookieManagerLayer`, a body limit sized for the longest board code, and no CORS. Merging keeps `GET /api/scores` in the CORS-layered public router. Handlers validate the body first, then call `auth::session::require_user`; the score name is the username, never the body. Board dedup (`handlers::boards::store`, used by Share and Submit) returns the existing row untouched, so `created_by` is whoever first shared or submitted it (or NULL). Tests sign in with `test_support::sign_up` (direct session) or the passkey harness in `auth::tests`.
- Board states: `board_states` (token, payload_hash, n, code, created_by) holds every stored scene, deduplicated by the SHA-256 of its canonical board code, and `/s/:token` redirects to `/play/:n?s=<code>`. "Share" stays the name of the button and action; the stored thing is a board. `POST /api/scores` takes `SubmitScore { board: BoardCode }`, decodes it, validates the arrangement, and `handlers::scores::record` stores the board and the score in one transaction. Table locks always go boards (or `solution_shares`) first, then `scores`: the submit transaction writes the board before the score, and both directions of the board migration take ACCESS EXCLUSIVE locks in that order before any check, so no score lands between a guard and the change it guards. `scores.board_token` is a NOT NULL FK to the board; `scores.glue_recorded` (false only for scores the board migration backfilled) is per-score provenance and never goes on the board, since a legacy score and a new glue-free submission can share one. `scores.arrangement` is still written and read by `/api/scores/:id`, and is redundant with the board; every crate that parses JSON enables serde_json's `float_roundtrip`, so its doubles read back bit for bit (except `-0.0`, which jsonb stores as `0`). Migration tests build scratch schemas with `test_support::scratch_schema` and `UP_MIGRATIONS`, and reach renamed tables through `test_support::legacy`.
- Discoverable auth: login/start has no username; the library verifies the assertion against the locked player credential selected by BOTH credential ID and user handle. Registration/add request residentKey required. Availability is advisory and rate-limited. Migration 20260914000300 deliberately deletes player accounts, passkeys and sessions, preserving credited profiles and all score/board data except owner FKs becoming NULL. Down cannot restore accounts. The no-default discoverable column prevents old binaries registering after reset. Fresh account counts and a record-preservation rehearsal precede release.
- Frontend accounts: `frontend/src/account.rs` holds the session in an `AccountProvider` context (`Account { username, ask }`) and renders a root modal; the header's `AccountMenu` is only an opener. Keep its page and modal-slot siblings stable so mounting the modal never remounts the game or loses its pending action. Session changes are serialized (`Settle`): a finish or sign-out, then a `/me` check, under one lock, and the page shows what `/me` says the cookie holds. UI guards can't stop a late `Set-Cookie`, so never let two cookie-changing requests overlap. Ceremonies are prepared (`api::prepare_*`) before `api::finish` sets the cookie, so a cancelled one is dropped before its finish; one cancelled mid-finish is signed out again. The startup `/me`, preparing and adding a passkey are numbered, and only the latest response counts. A busy provider ignores new actions (Enter submits past disabled buttons). `ScoreEntry.account` marks owned scores; legacy names render muted with a tag. A page that needs an account emits `Account::ask` with a `done` callback; the play screen freezes its Share or Submit in memory as a `BoardCode` (`Pending`), numbers each ask, and replays on a yes at most once. `frontend/src/webauthn.rs` bridges WebAuthn with `Reflect` (base64url to `ArrayBuffer` and back). Browser-test stubs for any API path (`Api`) and a fake `navigator.credentials` (`Passkeys`) live in `frontend/src/account/browser_tests.rs`; play-screen tests mount signed in with `mount_at`, or `mount_as(query, None)` signed out.
- The passkey migration seeds a `kind = 'credited'` user for every person in `refs/credits.json` (`packing_by`, `proof_by`, `other_proofs`, `prior_credits`), and `credited_profiles_match_the_migration_seed` fails if the two drift. Once that migration is merged, a newly credited person needs a new migration inserting their row, and the test must learn to read it. Its `down.sql` refuses while players, passkeys, sessions or attribution exist.
- Proxy trust lives in `backend/src/auth/proxy.rs`: `TRUSTED_PROXY_TOKEN` (exact `X-Packit-Proxy-Token`, constant-time) and/or `TRUSTED_PROXY` (socket peer) decide whether X-Forwarded-For is believed, and then only its rightmost entry. `proxy::edge` must stay the outermost layer in `build_app`, since it strips the token header; put any request logging inside it.
- Auth tests (`backend/src/auth/tests.rs`) drive the router with `oneshot`, so each request needs a `ConnectInfo<SocketAddr>` extension for the rate limiter. SoftPasskey covers normal flows; `TestKey` there sets the counter, backup flags and a fixed credential ID, which SoftPasskey can't. webauthn-rs rejects a credential in `excludeCredentials` itself at finish, before the database sees it.
- Glue types (`Feature`, `Glue`, `MAX_GLUES`) and their validation (`shared::glue::check`) live in `shared`; `physics` re-exports them. Board codes carry glue in an optional tagged trailer (`'G'`, version 2, one `u16` per feature) written only when there is glue, so glue-free codes stay byte-identical and existing links and dedup hashes remain valid.
- The board codec (`BoardState`, `encode`, `decode`) and load limits live in `shared::board`, used by the frontend (Share, Submit, board links, JSON import) and the backend (boards, submissions, link previews). `/play/:n` is a backend route that injects meta tags into trunk's built `dist/index.html` (via `include_str!`) before `</head>` and changes nothing else; tests assert the rest of the page is byte-identical, so hashed scripts and SRI survive. Absolute URLs come from `PUBLIC_URL`, never request headers.

## Workstream Ownership

Two agents share this repo; message before touching the other's files.

- Claude: `backend/`, `shared/`, `frontend/src/{main.rs,api.rs,leaderboard.rs}`, `frontend/style.css`, CI, README, `refs/markdown/` and the refs license table.
- Codex: `physics/`, `solver/`, `frontend/src/game/`, the rest of `refs/`.
- Every PR needs the other agent's review before merge. Both agents push as the same GitHub user, so reviews are PR comments ("LGTM ..."), not formal approvals.

## Merging

- `main` is protected: PRs required, and Lint Checks, Rustfmt, Clippy, Tests, Security Audit, Build Release Binary, and Build Container must pass. Admins are included, so there's no bypass.
- Merge with a merge commit (`gh pr merge N --merge`), not squash, so stacked branches rebase cleanly.
- To merge after CI, run `gh pr checks N --watch && gh pr merge N --merge`. Don't parse `gh pr checks` output by hand; a pending check once slipped through that way.
- Third-party papers in `refs/` keep their own licenses. Add every new paper to the "Licenses of included papers" table in `refs/README.md`.

## Local Checks

- `trunk` lives in `~/.cargo/bin`, which may not be on `PATH`.
- Build the frontend first (`cd frontend && trunk build`); the backend embeds `frontend/dist`. `backend/build.rs` reruns on `frontend/dist` changes; without it, new hashed asset names leave a stale embedded asset map and SRI failures.
- Physics host tests: `cargo test -p physics`. Browser tests for `physics` (WebGPU) and `frontend` (mounted play screen) are Rust `wasm-bindgen-test`s run in headless Chrome; see `physics/README.md` for the command. There is no hand-written JavaScript in the repo.
- DB tests run only when `TEST_DATABASE_URL` is set (CI sets it). Locally: `docker run -d --name packit-db -e POSTGRES_DB=packit -e POSTGRES_USER=packit -e POSTGRES_PASSWORD=dev_password -p 5433:5432 postgres:16-alpine`, create a separate test database (`docker exec packit-db psql -U packit -d packit -c "CREATE DATABASE packit_test"`), then `TEST_DATABASE_URL=postgresql://packit:dev_password@localhost:5433/packit_test cargo test --workspace`. Never point tests or browser checks at the `packit` database a preview serves; their rows show up on the leaderboard.
- The board migration renames `solution_shares`, so a test database it has run on breaks tests of older branches; worktrees on different branches should each use their own (e.g. `packit_test_boards`). It also refuses scores whose stored arrangement doesn't match their `n` and side, which older test runs left in long-lived test databases; recreate such a database rather than editing its rows.
- Passkey e2e (`backend/src/e2e.rs`, CI job "Passkey E2E"): build the frontend, then `PACKIT_E2E=1 CHROMEDRIVER=... TEST_DATABASE_URL=... cargo test -p backend --locked e2e -- --test-threads=1`. It uses only WebDriver commands, including the WebAuthn virtual-authenticator endpoints; never `execute_script`. Chrome allows one `internal` virtual authenticator per session, so remove one before adding another. To race a response, wrap the app in a test-only middleware (`Hold` delays a login finish's response); never add test hooks to the app. Scenes it shares must be unique per run: a deduped share keeps its first creator, so a reused snapshot never belongs to the new account.
- Rehearsing the board migration before it deploys: `SOURCE_DATABASE_URL=... REHEARSAL_DATABASE_URL=... scripts/rehearse-board-migration.sh` (SQL in `scripts/rehearsal/`). It needs only psql, pg_dump and pg_restore. The source is only read, by one `pg_dump` of `scores`, `solution_shares` and `__diesel_schema_migrations` (one transaction snapshot; no users, passkeys or sessions). The rehearsal database must be a fresh, empty throwaway one (or one already at the migrations before boards with no data) whose name differs from the source's (compared by `current_database()` and server identity, never by URL, before anything is written), and the script refuses anything else. It runs under `umask 077`, keeps the snapshot at mode 0600, and redacts both connection strings and any row the server quotes back (DETAIL, COPY context) from everything it prints. It restores the rows with a stub `users(id)` row per owner, captures a baseline, applies the board migration as diesel does, runs read-only checks that raise on any failure (score count and rows, board links, share tokens/codes/creators, new boards' creators, every code against its stored arrangement, rank order, `glue_recorded`, duplicate hashes), reverts and checks every link survives, migrates again, and checks the down refuses while a score has a recorded board (a fixture that is never committed). It exits nonzero on the first failure. Set `REHEARSAL_DIR` to keep the snapshot, which holds the source's scores and share links; delete it afterwards. Test changes to it only against throwaway local databases.
- CI runs clippy twice: host `--workspace --all-targets`, and `--target wasm32-unknown-unknown` for `frontend physics solver shared`, both with `RUSTFLAGS=-Dwarnings`.
- CI's `dtolnay/rust-toolchain@stable` can be newer than a local default, and newer clippy lints (e.g. `chunks_exact_to_as_chunks` in 1.98) then fail CI only. Install CI's version alongside (`rustup toolchain install 1.98.1 --profile minimal --component clippy,rustfmt --target wasm32-unknown-unknown`) and run both clippy passes with `cargo +1.98.1 clippy ...` before pushing.
- When scripting checks, gate on cargo's exit status: redirect to a log and `if ! cargo test ... > log 2>&1; then exit 1; fi`. Don't rely on `set -e` through pipelines, since `cargo test | grep | head` can mask a failure. Expected summary lines (e.g. `^test result: ok. 12 passed`) are only an extra check: one test binary or doctest run can report ok while another fails.

## Regular polygons

- `shared::Shape` is the discriminator on `Arrangement` (old JSON defaults to Square). Placements are centroids with unit edge length. Use `shape.vertices`, generic SAT and the actual polygon area for geometry and area bounds. Rotate canonical vertices with `theta.sin_cos()`; adding local angles to huge finite theta collapses vertices.
- Square board header 1 and glue trailer 2 are byte-identical to historical links. Polygon headers 3/5/6 identify triangles/pentagons/hexagons; glue trailer 3 uses three feature-index bits. Do not change old hashes. Shapes are in the board payload, not inferred from n or a caller-supplied score field.
- Migration 20260914000400 adds scores.shape default4; rankings filter shape and n. It preserves every score and board. Down refuses while any polygon score or board exists. Polygon play routes are `/play/:shape/:n`, and `/api/records?shape=triangle` selects the reference set. Missing query shape means square.
- `physics::new_for` selects the shape. Non-squares use the generic CPU path; square GPU remains unchanged. Evaluate pair contacts with a canonical index order so identical overlapping poses receive opposite forces. Optional edge attraction is disabled in polygon UI. Polygon refinement is numerical bounding-box tightening, never square algebraic certification.

## Container games

- `Arrangement.container` defaults to Square. `side` is the enclosing regular polygon's edge length; all containers are centered at `(side/2, side/2)`. Square coordinates and wall0..3 retain their historical convention. Other walls follow consecutive CCW vertices starting at the top. Use `Shape::walls`, `container_vertices`, `area_bound`, and `extent`; never reuse axis-aligned square containment or area for another enclosure.
- Square-piece/square-container games retain the GPU path. Other combinations use polygon CPU contacts. Non-square band changes shift bodies and the mouse by half the actual side delta; generalized wall reaction is measured about the fixed center. Settle grows only after stalled contact progress, as before.
- Canonical codes retain every old square-container byte. Polygon enclosures use header `0x80 | ((container_sides-3)<<2) | (piece_sides-3)`, with the existing 3-bit-feature glue trailer. Headers that redundantly encode a square enclosure are rejected. Scores filter by container, piece shape, and count. No square-container literature benchmark is shown for another enclosure.
- Picker uses a native modal dialog, draft choices, and Apply navigation. The game surface suppresses browser selection, but inputs, links and help remain selectable. Keep game action selectors distinct from modal buttons.

## Multitouch

- Board touch pointers are tracked by pointerId in game/touch.rs. Distinct pieces get independent springs; two pointers on the same piece rotate; two starting on empty space pan/pinch the view. Multi-pointer sequences never contribute to double-tap glue. Single-touch glue is recognized on release so a second finger cannot accidentally open it.
- Camera pan is a world offset relative to the current container center. Drawing, picking, wheel and corner controls share pan and extent. Manual zoom persists until Fit board, Reset or import. Physics frame growth shifts every grab target with the bodies.
- Physics::set_grabs uses capped CPU springs while held and resumes the existing GPU from the live pose when released. Independent drag targets survive another pointer releasing; only a remaining finger from a same-piece rotation is rebased. Settle/reset/load clear grabs.

## Personal best submissions

- Score submission keeps one best score per signed-in UUID and (container, piece shape, n). Better submissions delete older owned entries and insert their replacement atomically; worse/tied submissions keep the earliest best. Existing duplicate entries are pruned only when that owner submits in that category, never by name or by a bulk migration. Board rows and public tokens are preserved.
- `scores::record` acquires a transaction-scoped advisory lock for the account/category before touching board rows, then follows boards -> scores lock order. This serializes concurrent first submissions as well as replacements. Test both race orders and failure after deletion, using actual Diesel transactions rather than manual BEGIN around a Diesel transaction.

## Leaderboard filters and player records

- `/players/:username` uses the same icon/count filters as the global leaderboard. `GET /api/scores?player=<username>` resolves the account UUID through `users`, never through `scores.player`; same-named legacy scores remain unowned. Personal listings select one best per n within the selected piece/container category and keep each entry's global rank. SQL DISTINCT ON ordering and bounded limits matter for historical duplicates.
- Account names link to their records; legacy names remain plain tagged text. Filter selects mark the selected option explicitly so rerenders retain the displayed count. Keep the selected-game Play link, timestamps, and board links on both views.
