//! Helpers for tests that drive the whole app in-process.

use crate::auth::proxy::ProxyTrust;
use crate::auth::{hex, session};
use crate::config::PublicOrigin;
use crate::db::{self, DbPool};
use crate::schema::users;
use crate::AppState;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool};
use std::sync::Arc;
use uuid::Uuid;

pub const TEST_URL: &str = "https://packit.test";

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
