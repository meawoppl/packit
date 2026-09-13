//! Reset is deliberately irreversible for accounts, but never for records.
use crate::test_support::{scratch_schema, test_db, UP_MIGRATIONS};
use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel::sql_types::Jsonb;
use serde_json::Value;

const UP: &str = UP_MIGRATIONS[5];
const DOWN: &str =
    include_str!("../../migrations/2026-09-14-000300_discoverable_accounts/down.sql");
const OLD_REGISTER: &str = "INSERT INTO users(id, username, kind) VALUES ('00000000-0000-0000-0000-000000000001', 'former', 'player'); INSERT INTO passkeys(credential_id,user_id,passkey) VALUES ('\\x01', '00000000-0000-0000-0000-000000000001', '{}'); INSERT INTO sessions(token_hash,user_id,expires_at) VALUES (decode(repeat('ab',32),'hex'),'00000000-0000-0000-0000-000000000001',now()+interval '1 day');";

#[derive(QueryableByName)]
struct Row {
    #[diesel(sql_type = Jsonb)]
    value: Value,
}
fn snapshot(conn: &mut PgConnection, table: &str, omit: &str) -> Vec<Value> {
    diesel::sql_query(format!(
        "SELECT to_jsonb(t) - '{omit}' AS value FROM {table} t ORDER BY to_jsonb(t)::text"
    ))
    .load::<Row>(conn)
    .unwrap()
    .into_iter()
    .map(|r| r.value)
    .collect()
}

#[test]
fn reset_preserves_records_and_credited_profiles_and_rejects_old_writers() {
    let Some(pool) = test_db() else { return };
    pool.get().unwrap().test_transaction::<_, diesel::result::Error, _>(|conn| {
        scratch_schema(conn, 5)?;
        conn.batch_execute(OLD_REGISTER)?;
        conn.batch_execute("INSERT INTO board_states(token,payload_hash,n,code,created_by) VALUES (repeat('a',24),decode(repeat('ab',32),'hex'),1,repeat('0',70),'00000000-0000-0000-0000-000000000001'); INSERT INTO scores(player,n,side,arrangement,user_id,board_token,glue_recorded) VALUES ('former',1,1,'{}','00000000-0000-0000-0000-000000000001',repeat('a',24),true);")?;
        let credited = snapshot(conn, "(SELECT * FROM users WHERE kind='credited')", "");
        let scores = snapshot(conn, "scores", "user_id");
        let boards = snapshot(conn, "board_states", "created_by");
        conn.batch_execute(UP)?;
        assert_eq!(snapshot(conn, "users", ""), credited);
        assert_eq!(snapshot(conn, "scores", "user_id"), scores);
        assert_eq!(snapshot(conn, "board_states", "created_by"), boards);
        assert!(snapshot(conn, "passkeys", "").is_empty());
        assert!(snapshot(conn, "sessions", "").is_empty());
        assert!(snapshot(conn, "scores", "").iter().all(|r| r["user_id"].is_null()));
        assert!(snapshot(conn, "board_states", "").iter().all(|r| r["created_by"].is_null()));
        let before = snapshot(conn, "users", "");
        let old = conn.transaction::<_, diesel::result::Error, _>(|conn| conn.batch_execute(OLD_REGISTER));
        assert!(old.is_err(), "old inserts lack the required discoverable column");
        assert_eq!(snapshot(conn, "users", ""), before, "no orphan player");
        conn.batch_execute(DOWN)?;
        assert_eq!(snapshot(conn, "users", ""), credited, "down cannot recover accounts");
        assert_eq!(snapshot(conn, "scores", "user_id"), scores);
        assert_eq!(snapshot(conn, "board_states", "created_by"), boards);
        Ok(())
    });
}

#[test]
fn reset_waits_for_an_old_registration_then_removes_it() {
    let Some(pool) = test_db() else { return };
    let schema = format!("reset_race_{}", uuid::Uuid::new_v4().simple());
    let mut setup = PgConnection::establish(&std::env::var("TEST_DATABASE_URL").unwrap()).unwrap();
    setup
        .batch_execute(&format!(
            "CREATE SCHEMA {schema}; SET search_path TO {schema},public;"
        ))
        .unwrap();
    struct Cleanup<'a>(&'a str, &'a crate::db::DbPool);
    impl Drop for Cleanup<'_> {
        fn drop(&mut self) {
            let _ = self
                .1
                .get()
                .unwrap()
                .batch_execute(&format!("DROP SCHEMA {} CASCADE", self.0));
        }
    }
    let _cleanup = Cleanup(&schema, &pool);
    for up in &UP_MIGRATIONS[..5] {
        setup.batch_execute(up).unwrap();
    }
    let mut old = PgConnection::establish(&std::env::var("TEST_DATABASE_URL").unwrap()).unwrap();
    old.batch_execute(&format!(
        "SET search_path TO {schema},public; SET statement_timeout='20s'; BEGIN;"
    ))
    .unwrap();
    old.batch_execute(OLD_REGISTER).unwrap();
    let mut migration =
        PgConnection::establish(&std::env::var("TEST_DATABASE_URL").unwrap()).unwrap();
    migration.batch_execute(&format!("SET search_path TO {schema},public; SET statement_timeout='20s'; SET lock_timeout='20s';")).unwrap();
    let (sent, received) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        sent.send(()).unwrap();
        migration.transaction::<_, diesel::result::Error, _>(|conn| conn.batch_execute(UP))
    });
    received.recv().unwrap();
    #[derive(QueryableByName)]
    struct Waiting {
        #[diesel(sql_type=diesel::sql_types::Bool)]
        waiting: bool,
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let waiting = diesel::sql_query(format!("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE relation='{schema}.users'::regclass AND NOT granted) AS waiting")).get_result::<Waiting>(&mut setup).unwrap().waiting;
        if waiting {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "migration never waited for registration"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    old.batch_execute("COMMIT").unwrap();
    worker.join().unwrap().unwrap();
    assert!(snapshot(&mut setup, "(SELECT * FROM users WHERE kind='player')", "").is_empty());
    assert!(snapshot(&mut setup, "passkeys", "").is_empty());
    assert!(snapshot(&mut setup, "sessions", "").is_empty());
    setup.batch_execute("SET search_path TO public").unwrap();
    old.batch_execute("SET search_path TO public").unwrap();
}
