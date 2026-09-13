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
| POST | `/api/scores` | `SubmitScore` (`{ board: { n, code } }`) | `ScoreEntry`; needs a session |
| GET | `/api/scores` | `?n=&limit=` | `Vec<ScoreEntry>`, each with its `board` token and `glue_recorded` |
| GET | `/api/scores/:id` | | `ScoreDetail` |
| GET | `/api/records` | | `Vec<KnownRecord>` |
| POST | `/api/boards` | `BoardCode` (`{ n, code }`) | `BoardLink` (`{ url }`), the durable short URL of a stored board; needs a session |
| GET | `/s/:token` | — | Redirect to the stored board and its preview |
| GET | `/api/preview.png` | `?n=&s=` | 1200×630 PNG of a board code |
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
using `shared::VALIDATION_TOL`. A score's leaderboard name is its account's
username.

### Board states

A board state is a stored scene: a packing and the glue between its squares
and walls, as a board code (`shared::board`, the bit-exact hex that also
appears in `/play/:n?s=<hex>`). The `board_states` table holds every one that
was shared or submitted, deduplicated by the SHA-256 of its canonical code, and
`/s/<token>` redirects to its `/play/:n?s=<hex>` page. It is durable app state
and belongs in database backups.

- The **Share** button saves the current board and copies its short
  `/s/<token>` URL, suitable for posting on social media. Unfinished packings
  can be shared without submitting a score. If a browser blocks automatic
  clipboard access, use **Copy link** or select the shown URL.
- **Submit** sends the board as it was when pressed: the certified packing
  and the scene's glue. The server decodes and validates it, then stores the
  board (or finds it already stored) and the score in one transaction. Each
  score references its board by token (`scores.board_token`), and the
  leaderboard and score pages link to it.
- Identical boards share one row and one URL, whoever shares or submits them,
  and the row keeps the account that first created it.
- `scores.glue_recorded` is false for scores saved before boards existed.
  Their boards were backfilled from the stored arrangement, so they hold the
  packing without its glue, and the leaderboard says "glue not recorded".
  It belongs to the score, not the board: a legacy score and a new glue-free
  submission of the same packing share one board.
- Reverting the board migration refuses while any score has a recorded board
  (`glue_recorded`), since its board link and glue would be lost. With only
  legacy scores it reverts, keeping every board row as a share link, and
  upgrading again links each score to the same board. The upgrade itself
  refuses scores whose stored arrangement doesn't match their own `n` and
  side, or doesn't fit a board code, rather than guessing their boards.

Board links unfurl as the board's packing: the page carries Open Graph and
Twitter tags whose absolute URLs use `PUBLIC_URL` (default
`https://potatos.txcl.io`), never the request host. The preview image is only
served for a code that fully decodes, and is cached as immutable.

## Accounts

Players create an account with a username and a passkey (WebAuthn, via
`webauthn-rs`) from the header's account control, and sign in the same way.
Submitting a score and creating a short link need an account. Playing,
opening existing links (`/s/:token` and `/play/:n?s=`), link previews,
records and the leaderboards stay public and anonymous.

- On the play screen, Share or Submit while signed out opens sign-in instead
  of sending anything. The request is frozen as it was when pressed (the
  certified packing and its glue for Submit; the board for Share), kept
  in memory, and sent once if the sign-in succeeds. Dismissing sign-in,
  leaving the page or unmounting the screen drops it. Submit with no
  certified packing asks for a sign-in and then for Submit again. A 401 from
  either, such as an expired session, goes the same way, and Share resends
  the same code.
- A score is credited to the signed-in account (`scores.user_id`) and named
  by its username; a name in the request body is ignored. Scores from before
  accounts keep their stored names and stay unowned, even if an account later
  takes the same name. Every `ScoreEntry` carries `account` (true when the
  score has an owner), and the leaderboards show legacy names muted, with a
  "legacy" tag.
- There is no account recovery. An account whose passkeys are all lost can't
  be recovered, so the sign-up help asks players to keep a synced passkey or
  add a second one.
- The header's account state numbers each auth operation (the startup
  `/api/auth/me`, sign-in, registration, adding a passkey, sign-out) and
  applies only the latest one's response. Closing the dialog or a new sign-in
  request abandons a ceremony in flight, so a late response can't undo a
  newer sign-in or sign-out, or complete another request.
- A board records its creator (`board_states.created_by`). Sharing or
  submitting a board that is already stored reuses it unchanged, so it never
  reveals or changes who created it, and boards from before accounts stay
  unowned.
- The frontend calls WebAuthn from Rust (`frontend/src/webauthn.rs`): base64url
  fields in the server's options become `ArrayBuffer`s for
  `navigator.credentials`, and the credential's become base64url again for
  the finish endpoints. There is no hand-written JavaScript.

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
- Every auth POST, and `POST /api/scores` and `POST /api/boards`, must send
  exactly one `Origin` equal to `PUBLIC_URL` (403 otherwise), checked before
  the session cookie is read; none of them get CORS. Signed out, those two
  answer 401 once their body has passed validation. Public reads keep
  permissive CORS and no Origin check.
- A client is an IPv4 address or an IPv6 /64, and IPv6 clients are also
  grouped by /48. Start and finish endpoints are rate limited per client and
  per /48; login start is also limited per username and client. Each client
  may have 5 sign-ins in progress at once, and each /48 50.
- Sign-in is username-first, so anyone can find out whether a username
  exists; registration reports taken names too. That enumeration is accepted.
- Everyone named in `refs/credits.json` has a credited profile, seeded by the
  migration that creates the tables, so nobody can register their names
  first. The username is the ASCII-folded surname (`goebel` for Frits Göbel),
  with the full name in `display_name`. Credited profiles have no passkeys and
  can't sign in.
- Reverting that migration refuses while any player, passkey, session or
  score/share attribution exists; it never drops sign-in data.

### Behind a reverse proxy

**Production behind Traefik must set `TRUSTED_PROXY_TOKEN` or
`TRUSTED_PROXY`.** Rate limits key on the TCP peer address, so without them
every client shares Traefik's buckets and one client can lock everyone out of
signing in. The server logs a warning, once, when it sees `X-Forwarded-For`
while neither is set.

This assumes a single proxy hop. A trusted request takes the client IP from
the rightmost entry of the last `X-Forwarded-For` header, which Traefik
appends for the client it saw. Earlier entries are client-supplied and
ignored, and a malformed last entry falls back to the socket peer.

Two settings decide which requests are trusted:

- `TRUSTED_PROXY_TOKEN` (preferred): a shared secret, in production 32
  random bytes as 64 hex characters (`openssl rand -hex 32`). Traefik must
  overwrite the `X-Packit-Proxy-Token` request header with it on every
  request (a `headers` middleware with `customRequestHeaders`). When it is
  set, `X-Forwarded-For` is trusted only if the request carries exactly one
  `X-Packit-Proxy-Token` header equal to the token. It must be 32 to 256
  visible ASCII characters with no commas; any other value stops startup.
- `TRUSTED_PROXY`: Traefik's IP as the backend sees it, e.g. a fixed address
  on the shared Docker network. On its own, requests from that socket peer are
  trusted. With the token also set, both must match.

With neither set, both headers are ignored and every request is keyed on its
socket peer; leave them unset only when clients connect directly. The token
header is removed at the outermost layer, before any handler or log sees it,
and its value is never logged. A token header that doesn't match, or that
arrives with no token configured, is logged once per process.

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

The passkey flows also run end to end in Chrome, against the built frontend
served by the backend, with a WebDriver virtual authenticator (see
`backend/src/e2e.rs`; CI's **Passkey E2E** job). Plain `cargo test` skips
them without `CHROMEDRIVER`; `PACKIT_E2E=1` makes a missing driver or database
fail instead:

```sh
(cd frontend && trunk build)
PACKIT_E2E=1 CHROMEDRIVER=/path/to/chromedriver \
TEST_DATABASE_URL=postgresql://packit:dev_password@localhost:5433/packit_test \
  cargo test -p backend --locked e2e -- --test-threads=1
```
