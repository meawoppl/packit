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
| POST | `/api/auth/register/start` | `{ username }` | Passkey creation options and a ceremony id |
| POST | `/api/auth/register/finish` | `{ ceremony, credential }` | `{ username }`; sets the session cookie |
| POST | `/api/auth/login/start` | `{ username }` | Passkey request options and a ceremony id |
| POST | `/api/auth/login/finish` | `{ ceremony, credential }` | `{ username }`; sets the session cookie |
| POST | `/api/auth/passkeys/start` | — | Options to add a passkey (needs a sign-in within 5 minutes) |
| POST | `/api/auth/passkeys/finish` | `{ ceremony, credential }` | `{ username }` |
| POST | `/api/auth/logout` | — | 204; deletes the session and clears the cookie |
| GET | `/api/auth/me` | — | `{ username }`, or 401 |

Submissions are validated server-side with `shared::geometry::validate`
using `shared::VALIDATION_TOL`.

The **Share** button saves an immutable snapshot in Postgres and copies a short
`/s/<token>` URL, suitable for posting on social media. Identical snapshots reuse
the same URL. Unfinished packings can be shared without submitting a score. If
a browser blocks automatic clipboard access, use **Copy link** or select the
shown URL. Automatic settle still updates the address bar with the self-contained
`/play/:n?s=<hex>` form; short links redirect to that form. The `solution_shares`
table is durable app state and belongs in database backups.

Share links (codec in `shared::share`) unfurl as the
shared packing: the page carries Open Graph and Twitter tags whose absolute
URLs use `PUBLIC_URL` (default `https://potatos.txcl.io`), never the request
host. The preview image is only served for a code that fully decodes, and is
cached as immutable.

## Accounts

Players can create an account with a passkey (WebAuthn, via `webauthn-rs`).
Accounts don't gate anything yet: scores, shares and previews work
anonymously.

- `PUBLIC_URL` is the relying party. Its hostname is the RP ID and its exact
  origin is the only one accepted. It must be a bare `https://` origin;
  `http://localhost[:port]` is allowed only with `--dev-mode`, so try passkeys
  locally with `PUBLIC_URL=http://localhost:3000`.
- Usernames are trimmed and lowercased, then must be 3 to 24 of `a-z`, `0-9`,
  `_` and `-`, starting with a letter or digit.
- Ceremony state stays in server memory for 5 minutes, bound to a nonce cookie
  in the browser that started it. Run a single backend instance; a restart
  drops sign-ins in progress.
- Sessions are 30-day `__Host-packit_session` cookies (HttpOnly, Secure,
  SameSite=Lax). The database stores only a SHA-256 of the token, and every
  sign-in issues a new one.
- Every auth POST must send an `Origin` equal to `PUBLIC_URL`, and `/api/auth`
  has no CORS. The public API keeps permissive CORS and no Origin check, so
  before `/api/scores` or `/api/shares` start reading the session cookie
  (PR 2) they must get the same exact-Origin check.
- A client is an IPv4 address or an IPv6 /64, and IPv6 clients are also
  grouped by /48. Start and finish endpoints are rate limited per client and
  per /48; login start is also limited per username and client. Each client
  may have 5 sign-ins in progress at once, and each /48 50.
- Sign-in is username-first, so anyone can find out whether a username
  exists; registration reports taken names too. That enumeration is accepted.

### Behind a reverse proxy

**Production behind Traefik must set `TRUSTED_PROXY`.** Rate limits key on the
TCP peer address, so without it every client shares Traefik's buckets and one
client can lock everyone out of signing in. The server logs a warning, once,
when it sees `X-Forwarded-For` while `TRUSTED_PROXY` is unset.

Set it to Traefik's IP as the backend sees it, e.g. a fixed address for the
Traefik container on the shared Docker network. This assumes a single proxy
hop. Only requests from that exact address take the client IP from
`X-Forwarded-For`, and then only the rightmost entry of the last header,
which Traefik appends for the client it saw. Earlier entries are
client-supplied and ignored, and a malformed last entry falls back to the
proxy's own address. Leave it unset only when clients connect directly.

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
