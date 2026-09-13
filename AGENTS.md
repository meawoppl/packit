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

- Squares are **unit** squares: `shared::Placement { cx, cy, theta }` = center + rotation in radians. Container is `[0, side]^2`, origin bottom-left. Never introduce a second square type; `physics` and `solver` depend on `shared`.
- Server-side validation is `shared::geometry::validate(&arr, shared::VALIDATION_TOL)`. The play engine may allow small stiff overlaps for bounce, but submissions must pass validation.
- `refs/best_known.json` is compiled into the backend (`include_str!`) and served at `/api/records`. A unit test checks it parses, is sorted by `n`, and respects the `sqrt(n)` area bound.
- `rank` in `ScoreEntry` is computed at read time (smaller side first, earlier submission breaks ties), so a rank returned at submit time goes stale.
- Axum is 0.7: path params use `/:id`, not `/{id}`.
- Passkey auth lives in `backend/src/auth/` (ceremony store, rate limits, sessions, usernames) and `backend/src/handlers/auth.rs`. `PUBLIC_URL` is parsed once into `config::PublicOrigin` (RP ID, the only accepted `Origin`, cookie `Secure`). `/api/auth` has no CORS layer, needs an exact `Origin` on every POST, and never trusts a username or user id sent at finish. Use `auth::session::current_user` to attribute work to an account; nothing is gated on it yet.
- The passkey migration seeds a `kind = 'credited'` user for every person in `refs/credits.json` (`packing_by`, `proof_by`, `other_proofs`, `prior_credits`), and `credited_profiles_match_the_migration_seed` fails if the two drift. Once that migration is merged, a newly credited person needs a new migration inserting their row, and the test must learn to read it. Its `down.sql` refuses while players, passkeys, sessions or attribution exist.
- Proxy trust lives in `backend/src/auth/proxy.rs`: `TRUSTED_PROXY_TOKEN` (exact `X-Packit-Proxy-Token`, constant-time) and/or `TRUSTED_PROXY` (socket peer) decide whether X-Forwarded-For is believed, and then only its rightmost entry. `proxy::edge` must stay the outermost layer in `build_app`, since it strips the token header; put any request logging inside it.
- Auth tests (`backend/src/auth/tests.rs`) drive the router with `oneshot`, so each request needs a `ConnectInfo<SocketAddr>` extension for the rate limiter. SoftPasskey covers normal flows; `TestKey` there sets the counter, backup flags and a fixed credential ID, which SoftPasskey can't. webauthn-rs rejects a credential in `excludeCredentials` itself at finish, before the database sees it.
- Glue types (`Feature`, `Glue`, `MAX_GLUES`) and their validation (`shared::glue::check`) live in `shared`; `physics` re-exports them. Share codes carry glue in an optional tagged trailer (`'G'`, version 2, one `u16` per feature) written only when there is glue, so glue-free codes stay byte-identical and existing links and dedup hashes remain valid.
- The share-link codec and load limits live in `shared::share`, used by the frontend (share links, JSON import) and the backend (link previews). `/play/:n` is a backend route that injects meta tags into trunk's built `dist/index.html` (via `include_str!`) before `</head>` and changes nothing else; tests assert the rest of the page is byte-identical, so hashed scripts and SRI survive. Absolute URLs come from `PUBLIC_URL`, never request headers.

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
- CI runs clippy twice: host `--workspace --all-targets`, and `--target wasm32-unknown-unknown` for `frontend physics solver shared`, both with `RUSTFLAGS=-Dwarnings`.
- CI's `dtolnay/rust-toolchain@stable` can be newer than a local default, and newer clippy lints (e.g. `chunks_exact_to_as_chunks` in 1.98) then fail CI only. Install CI's version alongside (`rustup toolchain install 1.98.1 --profile minimal --component clippy,rustfmt --target wasm32-unknown-unknown`) and run both clippy passes with `cargo +1.98.1 clippy ...` before pushing.
- When scripting checks, gate on cargo's exit status: redirect to a log and `if ! cargo test ... > log 2>&1; then exit 1; fi`. Don't rely on `set -e` through pipelines, since `cargo test | grep | head` can mask a failure. Expected summary lines (e.g. `^test result: ok. 12 passed`) are only an extra check: one test binary or doctest run can report ok while another fails.
