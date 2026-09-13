//! Helpers for tests that drive the whole app in-process.

use crate::auth::proxy::ProxyTrust;
use crate::config::PublicOrigin;
use crate::db::{self, DbPool};
use crate::AppState;
use diesel::pg::PgConnection;
use diesel::r2d2::{ConnectionManager, Pool};
use std::sync::Arc;

pub const TEST_URL: &str = "https://packit.test";

pub fn state_for(db_pool: DbPool) -> Arc<AppState> {
    let origin = PublicOrigin::parse(TEST_URL, false).unwrap();
    Arc::new(AppState::new(true, db_pool, origin, ProxyTrust::default()).unwrap())
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
