//! Helpers for tests that drive the whole app in-process.

use crate::auth::proxy::ProxyTrust;
use crate::auth::{hex, session};
use crate::config::PublicOrigin;
use crate::db::{self, DbPool};
use crate::schema::{board_states, users};
use crate::AppState;
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use diesel::connection::SimpleConnection;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool};
use sha2::{Digest, Sha256};
use shared::{board, Arrangement};
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

pub const TEST_URL: &str = "https://packit.test";

/// Every up migration, oldest first, for tests that rebuild the schema in a
/// scratch namespace.
pub const UP_MIGRATIONS: [&str; 5] = [
    include_str!("../migrations/00000000000000_initial/up.sql"),
    include_str!("../migrations/2026-09-13-000000_solution_shares/up.sql"),
    include_str!("../migrations/2026-09-14-000000_share_glue/up.sql"),
    include_str!("../migrations/2026-09-14-000100_passkey_auth/up.sql"),
    include_str!("../migrations/2026-09-14-000200_board_states/up.sql"),
];

/// Inside a test transaction, which rolls it all back: create a scratch
/// schema, put it first on the search path, and run the first `count` up
/// migrations there, so real rows and concurrent tests are never touched.
pub fn scratch_schema(conn: &mut PgConnection, count: usize) -> QueryResult<()> {
    let schema = format!("scratch_{}", Uuid::new_v4().simple());
    conn.batch_execute(&format!(
        "CREATE SCHEMA {schema}; SET LOCAL search_path TO {schema}, public;"
    ))?;
    for up in &UP_MIGRATIONS[..count] {
        conn.batch_execute(up)?;
    }
    Ok(())
}

/// Tables as they were before a later migration renamed them.
pub mod legacy {
    diesel::table! {
        solution_shares (token) {
            token -> Text,
            payload_hash -> Bytea,
            n -> Int4,
            code -> Text,
            created_at -> Timestamptz,
            created_by -> Nullable<Uuid>,
        }
    }
}

/// Store `arrangement`, glue-free, as a board with no creator, like the
/// share links made before accounts; returns its token.
pub fn creatorless_board(conn: &mut PgConnection, arrangement: &Arrangement) -> String {
    let code = board::encode(arrangement, &[]);
    let token = Uuid::new_v4().simple().to_string()[..24].to_owned();
    diesel::insert_into(board_states::table)
        .values((
            board_states::token.eq(&token),
            board_states::payload_hash.eq(Sha256::digest(code.as_bytes()).to_vec()),
            board_states::n.eq(arrangement.n as i32),
            board_states::code.eq(&code),
        ))
        .execute(conn)
        .unwrap();
    token
}

/// Send `req` through `app`: the status and the JSON body.
pub async fn call<T: serde::de::DeserializeOwned>(
    app: &Router,
    req: Request<Body>,
) -> (StatusCode, T) {
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

/// A same-origin POST with no session.
pub fn post_json(uri: &str, body: &impl serde::Serialize) -> Request<Body> {
    Request::post(uri)
        .header(header::ORIGIN, TEST_URL)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(body).unwrap()))
        .unwrap()
}

/// A same-origin POST from `player`'s browser.
pub fn post_as(uri: &str, body: &impl serde::Serialize, player: &Player) -> Request<Body> {
    let mut req = post_json(uri, body);
    req.headers_mut()
        .insert(header::COOKIE, player.cookie.parse().unwrap());
    req
}

pub fn state_for(db_pool: DbPool) -> Arc<AppState> {
    let origin = PublicOrigin::parse(TEST_URL, false).unwrap();
    Arc::new(AppState::new(true, db_pool, origin, ProxyTrust::default()).unwrap())
}

/// A fresh player account signed in with a live session, and the `Cookie`
/// header that presents it.
pub struct Player {
    pub id: Uuid,
    pub username: String,
    pub cookie: String,
}

/// Create a player and a session for it directly in the database. Tests of
/// the passkey ceremonies themselves live in `auth::tests`.
pub fn sign_up(pool: &DbPool) -> Player {
    let id = Uuid::new_v4();
    let username = format!("player-{}", &id.simple().to_string()[..12]);
    let mut conn = pool.get().unwrap();
    diesel::insert_into(users::table)
        .values((
            users::id.eq(id),
            users::username.eq(&username),
            users::kind.eq("player"),
        ))
        .execute(&mut conn)
        .unwrap();
    let token = session::create(&mut conn, id, None).unwrap();
    let https = PublicOrigin::parse(TEST_URL, false).unwrap().https;
    Player {
        id,
        username,
        cookie: format!(
            "{}={}",
            session::session_cookie_name(https),
            hex::encode(&token)
        ),
    }
}

/// Finite doubles that stress JSON round trips: the edges (zeros,
/// subnormals, the smallest normal, ±MAX), a value that once came back a
/// ULP off, and `count` from fresh random bit patterns, a quarter of them
/// subnormal.
pub fn awkward_floats(count: usize) -> Vec<f64> {
    let mut floats = vec![
        0.0,
        -0.0,
        5e-324,
        -5e-324,
        f64::MIN_POSITIVE,
        -f64::MIN_POSITIVE,
        f64::MAX,
        f64::MIN,
        2.5115089416503906,
        0.1,
        1.0 / 3.0,
    ];
    floats.extend(
        (0..count)
            .map(|i| {
                let bits = u64::from_le_bytes(crate::auth::random_bytes());
                // Clearing the exponent leaves a subnormal.
                f64::from_bits(if i % 4 == 0 {
                    bits & 0x800f_ffff_ffff_ffff
                } else {
                    bits
                })
            })
            .filter(|f| f.is_finite()),
    );
    floats
}

/// Every double in an arrangement, as bits, so `-0.0` and `0.0` differ.
pub fn float_bits(a: &shared::Arrangement) -> Vec<u64> {
    let squares = a
        .squares
        .iter()
        .flat_map(|p| [p.cx.to_bits(), p.cy.to_bits(), p.theta.to_bits()]);
    std::iter::once(a.side.to_bits()).chain(squares).collect()
}

/// An arrangement made of `floats`: the side, then each square's three.
pub fn arrangement_of(floats: &[f64]) -> shared::Arrangement {
    let squares: Vec<shared::Placement> = floats[1..]
        .as_chunks::<3>()
        .0
        .iter()
        .map(|&[cx, cy, theta]| shared::Placement { cx, cy, theta })
        .collect();
    shared::Arrangement {
        n: squares.len() as u32,
        side: floats[0],
        squares,
    }
}

/// A one-connection pool on `TEST_DATABASE_URL` whose connection sits in a
/// transaction that never commits. Fixtures the app has to read through its
/// own handlers go with it, even if the test panics or is killed. Drop any
/// connection taken from it before calling the app.
pub fn rolled_back_pool() -> Option<DbPool> {
    test_db()?;
    let url = std::env::var("TEST_DATABASE_URL").ok()?;
    Some(
        Pool::builder()
            .max_size(1)
            .connection_customizer(Box::new(diesel::r2d2::TestCustomizer))
            .build(ConnectionManager::<PgConnection>::new(url))
            .unwrap(),
    )
}

/// A pool that never connects, for routes that don't touch the database.
pub fn unconnected_pool() -> DbPool {
    Pool::builder().build_unchecked(ConnectionManager::<PgConnection>::new(
        "postgres://invalid/db",
    ))
}

/// Pool for `TEST_DATABASE_URL`, migrated exactly once per test process so
/// parallel DB tests don't race to create the schema.
pub fn test_db() -> Option<DbPool> {
    static POOL: std::sync::OnceLock<Option<DbPool>> = std::sync::OnceLock::new();
    POOL.get_or_init(|| {
        let url = std::env::var("TEST_DATABASE_URL").ok()?;
        let pool = Pool::builder()
            .max_size(8)
            .build(ConnectionManager::<PgConnection>::new(url))
            .unwrap();
        db::run_migrations(&pool).unwrap();
        Some(pool)
    })
    .clone()
}
