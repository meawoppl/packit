//! Board states against Postgres: a submission stores its board with the
//! score, the leaderboard links to it, and the migration that introduced
//! boards backfills and reverts without losing anything. Migration tests
//! run in a scratch schema inside a rolled-back transaction. Every test
//! skips without TEST_DATABASE_URL.

use super::*;
use crate::auth::session::User;
use crate::build_app;
use crate::handlers::scores::record;
use crate::schema::{scores, users};
use crate::test_support::{
    call, legacy::solution_shares, post_as, scratch_schema, sign_up, state_for, test_db, TEST_URL,
    UP_MIGRATIONS,
};
use axum::body::Body;
use axum::http::Request;
use chrono::NaiveDateTime;
use diesel::connection::SimpleConnection;
use diesel::sql_types::BigInt;
use shared::glue::{Feature, Glue};
use shared::{ApiError, Arrangement, Placement, ScoreDetail, ScoreEntry, SubmitScore};
use tower::ServiceExt;

/// How many migrations come before boards; `UP_MIGRATIONS[BEFORE_BOARDS]`
/// is the board migration itself.
const BEFORE_BOARDS: usize = 4;
const DOWN: &str = include_str!("../../../migrations/2026-09-14-000200_board_states/down.sql");

/// Two touching squares in a box no other run uses, so their board never
/// dedupes onto another run's row.
fn unique_pair() -> Arrangement {
    pair(2.0 + (Uuid::new_v4().as_u128() % 1_000_000_000) as f64 * 1e-10)
}

fn pair(side: f64) -> Arrangement {
    let sq = |cx| Placement {
        cx,
        cy: 0.5,
        theta: 0.0,
    };
    Arrangement {
        n: 2,
        side,
        squares: vec![sq(0.5), sq(1.5)],
    }
}

/// Square 0 glued edge to edge to square 1, and to the left wall.
fn glues() -> Vec<Glue> {
    vec![
        Glue {
            a: Feature::Edge { square: 0, edge: 0 },
            b: Feature::Edge { square: 1, edge: 2 },
        },
        Glue {
            a: Feature::Wall(0),
            b: Feature::Midpoint { square: 0, edge: 2 },
        },
    ]
}

fn submission(arrangement: &Arrangement, glues: &[Glue]) -> SubmitScore {
    SubmitScore {
        board: BoardCode {
            n: arrangement.n,
            code: board::encode(arrangement, glues),
        },
    }
}

/// Every stored board with `code`: its token and creator.
fn boards_with(conn: &mut PgConnection, code: &str) -> QueryResult<Vec<(String, Option<Uuid>)>> {
    boards::table
        .filter(boards::payload_hash.eq(Sha256::digest(code.as_bytes()).to_vec()))
        .select((boards::token, boards::created_by))
        .load(conn)
}

fn get(uri: &str) -> Request<Body> {
    Request::get(uri).body(Body::empty()).unwrap()
}

#[tokio::test]
async fn a_submission_stores_its_board_with_its_glue() {
    let Some(pool) = test_db() else {
        eprintln!("TEST_DATABASE_URL not set; skipping");
        return;
    };
    let (first, second) = (sign_up(&pool), sign_up(&pool));
    let app = build_app(state_for(pool.clone()));
    let mut conn = pool.get().unwrap();
    let arr = unique_pair();
    let body = submission(&arr, &glues());
    let (status, entry): (_, ScoreEntry) = call(&app, post_as("/api/scores", &body, &first)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(entry.glue_recorded);
    assert_eq!(
        boards_with(&mut conn, &body.board.code).unwrap(),
        [(entry.board.clone(), Some(first.id))]
    );
    let stored: String = scores::table
        .find(entry.id)
        .select(scores::board_token)
        .first(&mut conn)
        .unwrap();
    assert_eq!(stored, entry.board);

    // Its link opens exactly the submitted board, glue included.
    let response = app.clone().oneshot(get(&entry.board_link())).await.unwrap();
    assert_eq!(response.status(), StatusCode::FOUND);
    let location = response.headers()[header::LOCATION].to_str().unwrap();
    assert_eq!(location, format!("{TEST_URL}/play/2?s={}", body.board.code));
    assert_eq!(
        board::decode(location.split("?s=").nth(1).unwrap(), 2).unwrap(),
        BoardState {
            arrangement: arr.clone(),
            glues: glues()
        }
    );

    // Submitted or shared again by another account, it is the same row
    // with the same creator. Without its glue it's another board.
    let (_, again): (_, ScoreEntry) = call(&app, post_as("/api/scores", &body, &second)).await;
    assert_ne!(again.id, entry.id);
    assert_eq!(again.board, entry.board);
    let (_, link): (_, BoardLink) = call(&app, post_as("/api/boards", &body.board, &second)).await;
    assert_eq!(link.url, format!("{TEST_URL}{}", entry.board_link()));
    assert_eq!(
        boards_with(&mut conn, &body.board.code).unwrap(),
        [(entry.board.clone(), Some(first.id))]
    );
    let (_, bare): (_, ScoreEntry) = call(
        &app,
        post_as("/api/scores", &submission(&arr, &[]), &second),
    )
    .await;
    assert_ne!(bare.board, entry.board);
}

#[tokio::test]
async fn an_invalid_submission_stores_nothing() {
    let Some(pool) = test_db() else {
        eprintln!("TEST_DATABASE_URL not set; skipping");
        return;
    };
    let player = sign_up(&pool);
    let app = build_app(state_for(pool.clone()));
    let mut conn = pool.get().unwrap();
    let mut overlap = unique_pair();
    overlap.squares[1].cx = 1.2;
    let arr = unique_pair();
    let self_glue = [Glue {
        a: Feature::Edge { square: 0, edge: 0 },
        b: Feature::Corner {
            square: 0,
            corner: 1,
        },
    }];
    let wrong_n = SubmitScore {
        board: BoardCode {
            n: 3,
            code: board::encode(&arr, &[]),
        },
    };
    for (body, why) in [
        (submission(&overlap, &glues()), "invalid packing"),
        (submission(&arr, &self_glue), "different objects"),
        (wrong_n, "not for 3 squares"),
    ] {
        let (status, err): (_, ApiError) = call(&app, post_as("/api/scores", &body, &player)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{why}");
        assert!(err.error.contains(why), "{}", err.error);
        assert!(
            boards_with(&mut conn, &body.board.code).unwrap().is_empty(),
            "{why}"
        );
    }
    let scored: i64 = scores::table
        .filter(scores::user_id.eq(player.id))
        .count()
        .get_result(&mut conn)
        .unwrap();
    assert_eq!(scored, 0);
}

#[tokio::test]
async fn concurrent_submissions_of_one_board_share_one_row() {
    let Some(pool) = test_db() else {
        eprintln!("TEST_DATABASE_URL not set; skipping");
        return;
    };
    let (a, b) = (sign_up(&pool), sign_up(&pool));
    let app = build_app(state_for(pool.clone()));
    let body = submission(&unique_pair(), &glues());
    let ((status_a, entry_a), (status_b, entry_b)): ((_, ScoreEntry), (_, ScoreEntry)) = tokio::join!(
        call(&app, post_as("/api/scores", &body, &a)),
        call(&app, post_as("/api/scores", &body, &b)),
    );
    assert_eq!((status_a, status_b), (StatusCode::OK, StatusCode::OK));
    assert_ne!(entry_a.id, entry_b.id);
    assert_eq!(entry_a.board, entry_b.board);
    let rows = boards_with(&mut pool.get().unwrap(), &body.board.code).unwrap();
    assert_eq!(rows.len(), 1);
    assert!([Some(a.id), Some(b.id)].contains(&rows[0].1));
}

#[tokio::test]
async fn the_leaderboard_links_each_score_to_its_board() {
    let Some(pool) = test_db() else {
        eprintln!("TEST_DATABASE_URL not set; skipping");
        return;
    };
    let player = sign_up(&pool);
    let app = build_app(state_for(pool.clone()));
    // Seven squares in a box no other run uses, on a leaderboard few tests
    // touch.
    let side = 3.0 + (Uuid::new_v4().as_u128() % 1_000_000) as f64 * 1e-9;
    let arr = Arrangement {
        n: 7,
        side,
        squares: (0..7)
            .map(|i| Placement {
                cx: (i % 3) as f64 + 0.5,
                cy: (i / 3) as f64 + 0.5,
                theta: 0.0,
            })
            .collect(),
    };
    let (status, submitted): (_, ScoreEntry) = call(
        &app,
        post_as("/api/scores", &submission(&arr, &[]), &player),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // A score as the migration leaves a legacy one, on the same board: the
    // board is shared, and only this score's glue went unrecorded.
    let legacy = Uuid::new_v4();
    diesel::insert_into(scores::table)
        .values((
            scores::id.eq(legacy),
            scores::player.eq("legacy"),
            scores::n.eq(7),
            scores::side.eq(side),
            scores::arrangement.eq(serde_json::to_value(&arr).unwrap()),
            scores::board_token.eq(&submitted.board),
            scores::glue_recorded.eq(false),
        ))
        .execute(&mut pool.get().unwrap())
        .unwrap();

    let (status, ranked): (_, Vec<ScoreEntry>) = call(&app, get("/api/scores?n=7&limit=200")).await;
    assert_eq!(status, StatusCode::OK);
    let row = |id| ranked.iter().find(|e| e.id == id).unwrap();
    assert_eq!(
        (
            row(submitted.id).board.as_str(),
            row(submitted.id).glue_recorded
        ),
        (submitted.board.as_str(), true)
    );
    assert_eq!(
        (row(legacy).board.as_str(), row(legacy).glue_recorded),
        (submitted.board.as_str(), false)
    );
    assert_eq!(row(legacy).board_link(), format!("/s/{}", submitted.board));
    let (_, detail): (_, ScoreDetail) = call(&app, get(&format!("/api/scores/{legacy}"))).await;
    assert_eq!(
        (detail.entry.board.as_str(), detail.entry.glue_recorded),
        (submitted.board.as_str(), false)
    );
    let (_, leaders): (_, Vec<ScoreEntry>) = call(&app, get("/api/scores?limit=200")).await;
    let top = leaders.iter().find(|e| e.n == 7).unwrap();
    let stored: i64 = boards::table
        .find(&top.board)
        .count()
        .get_result(&mut pool.get().unwrap())
        .unwrap();
    assert_eq!(stored, 1, "the record holder links to a stored board");
}

// ---------------------------------------------------------------------------
// The migration, in scratch schemas
// ---------------------------------------------------------------------------

fn at(seconds: i64) -> NaiveDateTime {
    chrono::DateTime::from_timestamp(1_700_000_000 + seconds, 0)
        .unwrap()
        .naive_utc()
}

/// A score row as stored before boards: `arrangement` is written as given.
fn score_row(
    conn: &mut PgConnection,
    n: i32,
    side: f64,
    arrangement: &serde_json::Value,
    user: Option<Uuid>,
    submitted_at: NaiveDateTime,
) -> QueryResult<Uuid> {
    let id = Uuid::new_v4();
    diesel::insert_into(scores::table)
        .values((
            scores::id.eq(id),
            scores::player.eq("legacy"),
            scores::n.eq(n),
            scores::side.eq(side),
            scores::arrangement.eq(arrangement),
            scores::submitted_at.eq(submitted_at),
            scores::user_id.eq(user),
        ))
        .execute(conn)?;
    Ok(id)
}

/// A score as the submit handler stored one before boards.
fn legacy_score(
    conn: &mut PgConnection,
    arrangement: &Arrangement,
    user: Option<Uuid>,
    submitted_at: NaiveDateTime,
) -> QueryResult<Uuid> {
    let json = serde_json::to_value(arrangement).unwrap();
    score_row(
        conn,
        arrangement.n as i32,
        arrangement.side,
        &json,
        user,
        submitted_at,
    )
}

/// A share link as stored before boards.
fn legacy_share(conn: &mut PgConnection, code: &str, creator: Option<Uuid>) -> QueryResult<String> {
    let token = new_token();
    diesel::insert_into(solution_shares::table)
        .values((
            solution_shares::token.eq(&token),
            solution_shares::payload_hash.eq(Sha256::digest(code.as_bytes()).to_vec()),
            solution_shares::n.eq(2),
            solution_shares::code.eq(code),
            solution_shares::created_by.eq(creator),
        ))
        .execute(conn)?;
    Ok(token)
}

fn scratch_user(conn: &mut PgConnection, username: &str) -> QueryResult<User> {
    let id = Uuid::new_v4();
    diesel::insert_into(users::table)
        .values((
            users::id.eq(id),
            users::username.eq(username),
            users::kind.eq("player"),
        ))
        .execute(conn)?;
    Ok(User {
        id,
        username: username.into(),
    })
}

type Row = (String, Vec<u8>, i32, String, Option<Uuid>);

/// Every board row, by token.
fn board_rows(conn: &mut PgConnection) -> QueryResult<Vec<Row>> {
    boards::table
        .order(boards::token)
        .select((
            boards::token,
            boards::payload_hash,
            boards::n,
            boards::code,
            boards::created_by,
        ))
        .load(conn)
}

/// Every share row, by token, while the table still has its old name.
fn share_rows(conn: &mut PgConnection) -> QueryResult<Vec<Row>> {
    solution_shares::table
        .order(solution_shares::token)
        .select((
            solution_shares::token,
            solution_shares::payload_hash,
            solution_shares::n,
            solution_shares::code,
            solution_shares::created_by,
        ))
        .load(conn)
}

/// Each score's board and whether its glue was recorded, by score id.
fn links(conn: &mut PgConnection) -> QueryResult<Vec<(Uuid, String, bool)>> {
    scores::table
        .order(scores::id)
        .select((scores::id, scores::board_token, scores::glue_recorded))
        .load(conn)
}

fn link(conn: &mut PgConnection, id: Uuid) -> QueryResult<(String, bool)> {
    scores::table
        .find(id)
        .select((scores::board_token, scores::glue_recorded))
        .first(conn)
}

fn creator(conn: &mut PgConnection, token: &str) -> QueryResult<Option<Uuid>> {
    boards::table
        .find(token)
        .select(boards::created_by)
        .first(conn)
}

/// Score ids in leaderboard order: by n, then side, submission time and id.
fn ranked(conn: &mut PgConnection) -> QueryResult<Vec<Uuid>> {
    scores::table
        .order((scores::n, scores::side, scores::submitted_at, scores::id))
        .select(scores::id)
        .load(conn)
}

fn count(conn: &mut PgConnection, query: &str) -> QueryResult<i64> {
    diesel::select(diesel::dsl::sql::<BigInt>(&format!("({query})"))).get_result(conn)
}

/// A migration in a savepoint, so an expected failure leaves the test's
/// transaction usable.
fn run(conn: &mut PgConnection, sql: &str) -> QueryResult<()> {
    conn.transaction(|conn| conn.batch_execute(sql))
}

/// `n`, then the IEEE 754 bits of every float.
fn bits(a: &Arrangement) -> Vec<u64> {
    [a.n as u64, a.side.to_bits()]
        .into_iter()
        .chain(
            a.squares
                .iter()
                .flat_map(|p| [p.cx.to_bits(), p.cy.to_bits(), p.theta.to_bits()]),
        )
        .collect()
}

/// Arrangements whose floats are hard to carry through JSON, jsonb and SQL:
/// zeros of both signs, subnormals, decimals with no exact binary form, the
/// extremes a board code allows, and a hundred squares of random bits.
fn awkward(seed: u64) -> Vec<Arrangement> {
    let mut state = seed;
    // splitmix64
    let mut next = move || {
        state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    };
    // A finite f64 of random bits in `range`.
    let mut random = move |range: std::ops::RangeInclusive<f64>| loop {
        let v = f64::from_bits(next());
        if v.is_finite() && range.contains(&v) {
            return v;
        }
    };
    let sq = |cx, cy, theta| Placement { cx, cy, theta };
    let min_subnormal = f64::from_bits(1);
    let max_subnormal = f64::from_bits(0x000f_ffff_ffff_ffff);
    let below_1000 = f64::from_bits(1000f64.to_bits() - 1);
    vec![
        Arrangement {
            n: 1,
            side: 1.0,
            squares: vec![sq(min_subnormal, -0.0, 0.1)],
        },
        Arrangement {
            n: 3,
            side: 2.0 + 1.0 / 3.0,
            squares: vec![
                sq(0.1, 1.0 / 3.0, -0.0),
                sq(max_subnormal, -1e-310, f64::MAX),
                sq(f64::MIN_POSITIVE, 0.1 + 0.2, -f64::MAX),
            ],
        },
        Arrangement {
            n: 2,
            side: 1000.0,
            squares: vec![
                sq(1000.0, -1000.0, f64::EPSILON),
                sq(-1e-300, below_1000, 1e300),
            ],
        },
        Arrangement {
            n: 100,
            side: random(1.0..=1000.0),
            squares: (0..100)
                .map(|_| {
                    sq(
                        random(-1000.0..=1000.0),
                        random(-1000.0..=1000.0),
                        random(f64::MIN..=f64::MAX),
                    )
                })
                .collect(),
        },
    ]
}

#[test]
fn the_backfill_writes_rust_board_codes_byte_for_byte() {
    let Some(pool) = test_db() else {
        eprintln!("TEST_DATABASE_URL not set; skipping");
        return;
    };
    let seed = Uuid::new_v4().as_u64_pair().0;
    let legacy = awkward(seed);
    pool.get()
        .unwrap()
        .test_transaction::<_, diesel::result::Error, _>(|conn| {
            scratch_schema(conn, BEFORE_BOARDS)?;
            let mut ids = Vec::new();
            for (i, arr) in legacy.iter().enumerate() {
                ids.push(legacy_score(conn, arr, None, at(i as i64))?);
            }
            conn.batch_execute(UP_MIGRATIONS[BEFORE_BOARDS])?;
            for (id, original) in ids.into_iter().zip(&legacy) {
                let (code, recorded): (String, bool) = scores::table
                    .inner_join(boards::table)
                    .filter(scores::id.eq(id))
                    .select((boards::code, scores::glue_recorded))
                    .first(conn)?;
                // The score holds what was saved, bit for bit, except that
                // jsonb has no negative zero. It isn't read back through
                // serde_json, whose default float parsing is best-effort
                // and rejects Postgres's long decimals near f64::MAX.
                let unsigned = |v: f64| if v == 0.0 { 0.0 } else { v };
                let stored = Arrangement {
                    squares: original
                        .squares
                        .iter()
                        .map(|p| sq_unsigned(p, unsigned))
                        .collect(),
                    ..original.clone()
                };
                assert_eq!(code, board::encode(&stored, &[]), "seed {seed}");
                let decoded = board::decode(&code, stored.n).unwrap();
                assert!(decoded.glues.is_empty());
                assert_eq!(bits(&decoded.arrangement), bits(&stored), "seed {seed}");
                assert!(!recorded, "legacy glue was never recorded");
            }
            let rows = board_rows(conn)?;
            assert_eq!(rows.len(), legacy.len());
            for (token, hash, _, code, created_by) in rows {
                assert_eq!(hash, Sha256::digest(code.as_bytes()).to_vec());
                assert_eq!(token.len(), 24);
                assert!(token
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
                assert_eq!(created_by, None);
            }
            Ok(())
        });
}

fn sq_unsigned(p: &Placement, unsigned: impl Fn(f64) -> f64) -> Placement {
    Placement {
        cx: unsigned(p.cx),
        cy: unsigned(p.cy),
        theta: unsigned(p.theta),
    }
}

/// An upgrade of a database holding share links and scores: every score gets
/// its board, a code already stored is reused as it is, share links and
/// ranks don't change, and each score keeps its own glue provenance
/// whichever of it and its board came first.
#[test]
fn upgrading_a_populated_database_links_every_score_to_its_board() {
    let Some(pool) = test_db() else {
        eprintln!("TEST_DATABASE_URL not set; skipping");
        return;
    };
    pool.get()
        .unwrap()
        .test_transaction::<_, diesel::result::Error, _>(|conn| {
            scratch_schema(conn, BEFORE_BOARDS)?;
            let (ada, bob) = (scratch_user(conn, "ada")?, scratch_user(conn, "bob")?);
            let (shared_first, scored_twice, scored_once) = (pair(2.0), pair(2.25), pair(2.5));
            // Share links: one of a packing that is scored later, one glued,
            // and one from before accounts.
            let shared_token =
                legacy_share(conn, &board::encode(&shared_first, &[]), Some(bob.id))?;
            legacy_share(conn, &board::encode(&pair(3.0), &glues()), Some(ada.id))?;
            legacy_share(conn, &board::encode(&pair(3.5), &[]), None)?;
            // Scores: the shared packing, a packing scored twice (first
            // anonymously), and one scored twice at the same moment.
            let on_share = legacy_score(conn, &shared_first, Some(ada.id), at(3))?;
            let twice_later = legacy_score(conn, &scored_twice, Some(ada.id), at(2))?;
            let twice_first = legacy_score(conn, &scored_twice, None, at(1))?;
            let tie_bob = legacy_score(conn, &scored_once, Some(bob.id), at(4))?;
            let tie_ada = legacy_score(conn, &scored_once, Some(ada.id), at(4))?;
            let shares = share_rows(conn)?;
            let ranks = ranked(conn)?;

            conn.batch_execute(UP_MIGRATIONS[BEFORE_BOARDS])?;

            assert_eq!(ranked(conn)?, ranks);
            let rows = board_rows(conn)?;
            for share in &shares {
                assert!(rows.contains(share), "{share:?} is kept as it was");
            }
            assert_eq!(rows.len(), shares.len() + 2, "one board per new code");
            assert_eq!(link(conn, on_share)?, (shared_token.clone(), false));
            assert_eq!(
                creator(conn, &shared_token)?,
                Some(bob.id),
                "a reused board keeps its creator"
            );
            let (twice, _) = link(conn, twice_first)?;
            assert_eq!(link(conn, twice_later)?, (twice.clone(), false));
            assert_eq!(creator(conn, &twice)?, None, "the earliest score made it");
            let (once, _) = link(conn, tie_bob)?;
            assert_eq!(link(conn, tie_ada)?, (once.clone(), false));
            let first = if tie_bob < tie_ada { bob.id } else { ada.id };
            assert_eq!(creator(conn, &once)?, Some(first));
            assert!(links(conn)?.iter().all(|(_, _, recorded)| !recorded));

            // A new glue-free submission of a legacy score's packing reuses
            // its board, creator and all, and records its own glue; the
            // legacy score's stays unrecorded. With glue it's a new board.
            let bob_again = User {
                id: bob.id,
                username: bob.username.clone(),
            };
            let fresh = record(
                conn,
                &BoardState {
                    arrangement: scored_twice.clone(),
                    glues: vec![],
                },
                bob_again,
            )?;
            assert_eq!(
                (fresh.board.as_str(), fresh.glue_recorded),
                (twice.as_str(), true)
            );
            assert_eq!(link(conn, twice_first)?, (twice.clone(), false));
            assert_eq!(creator(conn, &twice)?, None);
            let glued = record(
                conn,
                &BoardState {
                    arrangement: scored_twice.clone(),
                    glues: glues(),
                },
                bob,
            )?;
            assert_ne!(glued.board, twice);
            assert_eq!(board_rows(conn)?.len(), shares.len() + 3);
            Ok(())
        });
}

#[test]
fn the_board_migration_refuses_scores_it_cannot_encode() {
    let Some(pool) = test_db() else {
        eprintln!("TEST_DATABASE_URL not set; skipping");
        return;
    };
    let json = |a: &Arrangement| serde_json::to_value(a).unwrap();
    let mut short = pair(2.0);
    short.squares.pop();
    let mut far = pair(2.0);
    far.squares[1].cx = 1000.5;
    let mut wrong_n = json(&pair(2.0));
    wrong_n["n"] = 3.into();
    let cases = [
        (2, 2.0, json(&short), "fewer squares than n"),
        (2, 2.5, json(&pair(2.0)), "a side unlike its own"),
        (2, 1500.0, json(&pair(1500.0)), "a side too large"),
        (2, 2.0, json(&far), "a center too far out"),
        (2, 2.0, wrong_n, "another n"),
        (1, 1.0, serde_json::json!({}), "no arrangement"),
    ];
    pool.get()
        .unwrap()
        .test_transaction::<_, diesel::result::Error, _>(|conn| {
            scratch_schema(conn, BEFORE_BOARDS)?;
            legacy_score(conn, &pair(2.0), None, at(0))?;
            for (n, side, arrangement, why) in cases {
                let refused = conn
                    .transaction::<(), diesel::result::Error, _>(|conn| {
                        score_row(conn, n, side, &arrangement, None, at(1))?;
                        conn.batch_execute(UP_MIGRATIONS[BEFORE_BOARDS])
                    })
                    .unwrap_err()
                    .to_string();
                assert!(
                    refused.contains("refusing to guess their boards"),
                    "{why}: {refused}"
                );
                assert_eq!(count(conn, "SELECT count(*) FROM scores")?, 1, "{why}");
            }
            conn.batch_execute(UP_MIGRATIONS[BEFORE_BOARDS])?;
            assert_eq!(links(conn)?.len(), 1);
            Ok(())
        });
}

/// Reverting boards refuses while any score has a recorded board. Otherwise
/// every board row stays, as a share link, and upgrading again links every
/// legacy score to the same board.
#[test]
fn the_board_down_migration_never_drops_a_recorded_board() {
    let Some(pool) = test_db() else {
        eprintln!("TEST_DATABASE_URL not set; skipping");
        return;
    };
    let constraints = |conn: &mut PgConnection, prefix: &str| {
        count(
            conn,
            &format!(
                "SELECT count(*) FROM pg_constraint c JOIN pg_namespace n \
                 ON n.oid = c.connamespace WHERE n.nspname = current_schema() \
                 AND c.conname LIKE '{prefix}%'"
            ),
        )
    };
    pool.get()
        .unwrap()
        .test_transaction::<_, diesel::result::Error, _>(|conn| {
            scratch_schema(conn, BEFORE_BOARDS)?;
            let ada = scratch_user(conn, "ada")?;
            legacy_share(conn, &board::encode(&pair(3.0), &glues()), Some(ada.id))?;
            legacy_score(conn, &pair(2.0), Some(ada.id), at(0))?;
            legacy_score(conn, &pair(2.0), None, at(1))?;
            legacy_score(conn, &pair(2.5), None, at(2))?;
            conn.batch_execute(UP_MIGRATIONS[BEFORE_BOARDS])?;
            let (linked, rows) = (links(conn)?, board_rows(conn)?);

            // Only legacy scores: their boards come back from their
            // arrangements, so it reverts, and keeps every row.
            run(conn, DOWN)?;
            assert_eq!(share_rows(conn)?, rows);
            assert_eq!(constraints(conn, "solution_shares_")?, 7);
            assert_eq!(constraints(conn, "board_states_")?, 0);
            assert_eq!(
                count(
                    conn,
                    "SELECT count(*) FROM information_schema.columns WHERE table_schema = \
                     current_schema() AND column_name IN ('board_token', 'glue_recorded')"
                )?,
                0
            );
            run(conn, UP_MIGRATIONS[BEFORE_BOARDS])?;
            assert_eq!(links(conn)?, linked);
            assert_eq!(board_rows(conn)?, rows);

            // A recorded board can't go back.
            record(
                conn,
                &BoardState {
                    arrangement: pair(2.75),
                    glues: glues(),
                },
                ada,
            )?;
            let (linked, rows) = (links(conn)?, board_rows(conn)?);
            let refused = run(conn, DOWN).unwrap_err().to_string();
            assert!(
                refused.contains("refusing to drop their board links and glue"),
                "{refused}"
            );
            assert_eq!((links(conn)?, board_rows(conn)?), (linked, rows.clone()));

            // With no scores at all it reverts, still keeping every board.
            diesel::delete(scores::table).execute(conn)?;
            run(conn, DOWN)?;
            assert_eq!(share_rows(conn)?, rows);
            Ok(())
        });
}

#[test]
fn a_failed_score_insert_leaves_no_board() {
    let Some(pool) = test_db() else {
        eprintln!("TEST_DATABASE_URL not set; skipping");
        return;
    };
    pool.get()
        .unwrap()
        .test_transaction::<_, diesel::result::Error, _>(|conn| {
            scratch_schema(conn, UP_MIGRATIONS.len())?;
            conn.batch_execute(
                "ALTER TABLE scores ADD CONSTRAINT refuse_player CHECK (player <> 'refused')",
            )?;
            let board = BoardState {
                arrangement: unique_pair(),
                glues: glues(),
            };
            let code = board::encode(&board.arrangement, &board.glues);
            let refused = scratch_user(conn, "refused")?;
            let err = record(conn, &board, refused).unwrap_err().to_string();
            assert!(err.contains("refuse_player"), "{err}");
            assert!(boards_with(conn, &code)?.is_empty(), "the board went too");
            let accepted = scratch_user(conn, "accepted")?;
            let id = accepted.id;
            let entry = record(conn, &board, accepted)?;
            assert_eq!(boards_with(conn, &code)?, [(entry.board, Some(id))]);
            Ok(())
        });
}
