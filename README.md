# packit

An interactive square packing game. Players shove `n` unit squares around a
playful physics sandbox (self-attraction, mouse pokes) to fit them
into the smallest square container they can. When the scene settles, a solver
tightens the arrangement and compares it with the best known results from the
literature. Good packings go on the leaderboard.

Built on [single-binary-rust-website](https://github.com/meawoppl/single-binary-rust-website):
a Yew WASM frontend embedded into an Axum backend as one binary.

## Layout

```
Cargo.toml              # Workspace: backend, frontend, physics, shared, solver
├── shared/             # Types used everywhere + geometry validation
│   └── src/geometry.rs # Unit-square containment/overlap checks (SAT)
├── physics/            # GPU rough-body physics for the play screen
├── solver/             # Refine settled packings, recover exact side lengths
├── frontend/           # Yew app: home, /play/:n, /leaderboard
├── backend/            # Axum server, Postgres leaderboard, embeds frontend
├── refs/               # Literature + best_known.json (served at /api/records)
└── .github/workflows/  # CI and container build
```

## Conventions

A square is a **unit** square given by its center `(cx, cy)` and rotation
`theta` in radians. The container is `[0, side] x [0, side]` with the origin
at the bottom-left. Every crate uses `shared::Placement` and
`shared::Arrangement`.

## API

| Method | Path | Body / query | Returns |
| --- | --- | --- | --- |
| GET | `/api/health` | | `HealthResponse` |
| POST | `/api/scores` | `SubmitScore` | `ScoreEntry` |
| GET | `/api/scores` | `?n=&limit=` | `Vec<ScoreEntry>` |
| GET | `/api/scores/:id` | | `ScoreDetail` |
| GET | `/api/records` | | `Vec<KnownRecord>` |
| POST | `/api/shares` | `{ n, code }` | Durable short URL for a share-code snapshot |
| GET | `/s/:token` | — | Redirect to the saved solution and its preview |
| GET | `/api/preview.png` | `?n=&s=` | 1200×630 PNG of a share code |
| GET | `/play/:n` | `?s=` | The app page with link-preview metadata |

Submissions are validated server-side with `shared::geometry::validate`
using `shared::VALIDATION_TOL`.

The **Share** button saves an immutable snapshot in Postgres and copies a short
`/s/<token>` URL, suitable for posting on social media. Identical snapshots reuse
the same URL. Unfinished packings can be shared without submitting a score, and
the snapshot keeps any glue between squares and walls. If
a browser blocks automatic clipboard access, use **Copy link** or select the
shown URL. Automatic settle still updates the address bar with the self-contained
`/play/:n?s=<hex>` form; short links redirect to that form. The `solution_shares`
table is durable app state and belongs in database backups.

Share links (codec in `shared::share`) unfurl as the
shared packing: the page carries Open Graph and Twitter tags whose absolute
URLs use `PUBLIC_URL` (default `https://potatos.txcl.io`), never the request
host. The preview image is only served for a code that fully decodes, and is
cached as immutable.

## Quick start

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk --locked

docker compose up db -d
cp .env.example .env

# The frontend must be built first; the backend embeds frontend/dist
(cd frontend && trunk build)
cargo run -p backend -- --dev-mode
# -> http://localhost:3000
```

## Checks

CI runs the same commands you can run locally:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked
cargo test --workspace --locked
./scripts/check-migration-names.sh
```
