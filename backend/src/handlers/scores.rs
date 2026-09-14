use super::boards;
use crate::auth::session::{self, User};
use crate::models::{NewScore, Score};
use crate::schema::scores;
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use diesel::prelude::*;
use sha2::{Digest, Sha256};
use shared::board::{self, BoardState};
use shared::geometry;
use shared::{
    ApiError, Arrangement, ScoreDetail, ScoreEntry, ScoresQuery, SubmitScore, MAX_N, VALIDATION_TOL,
};
use std::sync::Arc;
use std::time::Duration;
use tower_cookies::Cookies;
use uuid::Uuid;

pub const SIGN_IN_TO_SUBMIT: &str = "Sign in to submit a score";
pub const DEFAULT_LIMIT: u32 = 50;
pub const MAX_LIMIT: u32 = 200;

/// An error response carrying an HTTP status, a JSON [`ApiError`] body and,
/// for 429s, a `Retry-After` in seconds.
#[derive(Debug)]
pub struct HandlerError(StatusCode, String, Option<u64>);

impl HandlerError {
    pub(crate) fn new(status: StatusCode, msg: impl Into<String>) -> Self {
        Self(status, msg.into(), None)
    }

    pub(super) fn not_found(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, msg)
    }

    pub(super) fn bad_request(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, msg)
    }

    /// Ask the client to wait `wait`, rounded up to whole seconds.
    pub(crate) fn retry_after(mut self, wait: Duration) -> Self {
        self.2 = Some((wait.as_secs() + u64::from(wait.subsec_nanos() > 0)).max(1));
        self
    }
}

impl<E: std::fmt::Display> From<E> for HandlerError {
    fn from(e: E) -> Self {
        tracing::error!("internal error: {e}");
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
    }
}

impl IntoResponse for HandlerError {
    fn into_response(self) -> Response {
        let mut response = (self.0, Json(ApiError { error: self.1 })).into_response();
        if let Some(secs) = self.2 {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from(secs));
        }
        response
    }
}

type HandlerResult<T> = Result<Json<T>, HandlerError>;

/// Run a blocking Diesel closure on the blocking thread pool.
pub(crate) async fn with_conn<T, F>(state: &AppState, f: F) -> Result<T, HandlerError>
where
    T: Send + 'static,
    F: FnOnce(&mut PgConnection) -> QueryResult<T> + Send + 'static,
{
    let pool = state.db_pool.clone();
    let result = tokio::task::spawn_blocking(move || {
        let mut conn = pool.get()?;
        f(&mut conn).map_err(anyhow::Error::from)
    })
    .await??;
    Ok(result)
}

/// Rank of a score among all scores with the same `n`. Ordering is
/// `(side, submitted_at, id)`, matching [`list`], so ranks are unique.
fn rank_of(conn: &mut PgConnection, score: &Score) -> QueryResult<u32> {
    let better: i64 = scores::table
        .filter(scores::shape.eq(score.shape))
        .filter(scores::container.eq(score.container))
        .filter(scores::n.eq(score.n))
        .filter(
            scores::side
                .lt(score.side)
                .or(scores::side.eq(score.side).and(
                    scores::submitted_at
                        .lt(score.submitted_at)
                        .or(scores::submitted_at
                            .eq(score.submitted_at)
                            .and(scores::id.lt(score.id))),
                )),
        )
        .count()
        .get_result(conn)?;
    Ok(better as u32 + 1)
}

fn entry(score: &Score, rank: u32) -> ScoreEntry {
    ScoreEntry {
        container: shared::Shape::from_sides(score.container as u8)
            .expect("validated stored container"),
        shape: shared::Shape::from_sides(score.shape as u8).expect("validated stored shape"),
        id: score.id,
        player: score.player.clone(),
        n: score.n as u32,
        side: score.side,
        submitted_at: score.submitted_at,
        rank,
        account: score.user_id.is_some(),
        board: score.board_token.clone(),
        glue_recorded: score.glue_recorded,
    }
}

/// Check a submission before it touches the database.
pub fn check_submission(arr: &Arrangement) -> Result<(), HandlerError> {
    if !(1..=MAX_N).contains(&arr.n) {
        return Err(HandlerError::bad_request(format!(
            "n must be between 1 and {MAX_N}"
        )));
    }
    geometry::validate(arr, VALIDATION_TOL)
        .map_err(|v| HandlerError::bad_request(format!("invalid packing: {v}")))
}

/// Save a score for the signed-in account, named by its username, with the
/// board state it was submitted as. The board is decoded and its packing
/// checked first, since that needs no database.
pub async fn submit(
    State(state): State<Arc<AppState>>,
    cookies: Cookies,
    Json(body): Json<SubmitScore>,
) -> HandlerResult<ScoreEntry> {
    let board = board::decode(&body.board.code, body.board.n).map_err(HandlerError::bad_request)?;
    check_submission(&board.arrangement)?;
    let user = session::require_user(&state, &cookies, SIGN_IN_TO_SUBMIT).await?;
    let result = with_conn(&state, move |conn| record(conn, &board, user)).await?;
    tracing::info!(
        "score {} by {}: n={} side={} rank={}",
        result.id,
        result.player,
        result.n,
        result.side,
        result.rank
    );
    Ok(Json(result))
}

/// Store `board`, or find it already stored, and a score on it for `user`,
/// in one transaction: both or neither. Keep one personal best per account
/// and (container, shape, n); ties keep the earlier entry. Board links remain
/// immutable even when the score that first referenced them is replaced.
pub(crate) fn record(
    conn: &mut PgConnection,
    board: &BoardState,
    user: User,
) -> QueryResult<ScoreEntry> {
    let arr = &board.arrangement;
    let arrangement = serde_json::to_value(arr)
        .map_err(|e| diesel::result::Error::SerializationError(e.into()))?;
    conn.transaction(|conn| {
        // Serialize even the first submission, where there is no score row
        // to lock yet. Hash collisions only serialize unrelated categories.
        // Acquire before touching boards; retain the boards -> scores order
        // used by migrations and other submit transactions.
        let mut key = Sha256::new();
        key.update(b"potatos-personal-best");
        key.update(user.id.as_bytes());
        key.update([arr.container.sides() as u8, arr.shape.sides() as u8]);
        key.update(arr.n.to_le_bytes());
        let key = i64::from_le_bytes(key.finalize()[..8].try_into().unwrap());
        diesel::sql_query("SELECT pg_advisory_xact_lock($1)")
            .bind::<diesel::sql_types::BigInt, _>(key)
            .execute(conn)?;
        let board_token = boards::store(conn, board, user.id)?;
        let own = scores::table
            .filter(scores::user_id.eq(user.id))
            .filter(scores::container.eq(arr.container.sides() as i32))
            .filter(scores::shape.eq(arr.shape.sides() as i32))
            .filter(scores::n.eq(arr.n as i32));
        let best = own
            .order((scores::side, scores::submitted_at, scores::id))
            .select(Score::as_select())
            .first::<Score>(conn)
            .optional()?;
        if let Some(best) = best.filter(|s| s.side <= arr.side) {
            diesel::delete(own.filter(scores::id.ne(best.id))).execute(conn)?;
            return Ok(entry(&best, rank_of(conn, &best)?));
        }
        diesel::delete(own).execute(conn)?;
        let new = NewScore {
            container: arr.container.sides() as i32,
            shape: arr.shape.sides() as i32,
            player: user.username,
            user_id: user.id,
            n: arr.n as i32,
            side: arr.side,
            arrangement,
            board_token,
        };
        let score: Score = diesel::insert_into(scores::table)
            .values(&new)
            .returning(Score::as_returning())
            .get_result(conn)?;
        let rank = rank_of(conn, &score)?;
        Ok(entry(&score, rank))
    })
}

/// With `n`: the leaderboard for that `n`. Without: the current record holder
/// for every `n` that has submissions.
pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ScoresQuery>,
) -> HandlerResult<Vec<ScoreEntry>> {
    if let Some(player) = &query.player {
        if player.len() < 3
            || player.len() > 24
            || !player
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'-')
        {
            return Err(HandlerError::bad_request("invalid username"));
        }
    }
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT) as i64;
    if let Some(player) = query.player {
        let rows = with_conn(&state, move |conn| {
            personal_records(conn, &player, query.shape, query.container, query.n, limit)
        })
        .await?;
        return Ok(Json(rows));
    }
    let rows = with_conn(&state, move |conn| match query.n {
        Some(n) => {
            let rows: Vec<Score> = scores::table
                .filter(scores::shape.eq(query.shape.sides() as i32))
                .filter(scores::container.eq(query.container.sides() as i32))
                .filter(scores::n.eq(n as i32))
                .order((
                    scores::side.asc(),
                    scores::submitted_at.asc(),
                    scores::id.asc(),
                ))
                .limit(limit)
                .select(Score::as_select())
                .load(conn)?;
            Ok(rows
                .iter()
                .enumerate()
                .map(|(i, s)| entry(s, i as u32 + 1))
                .collect::<Vec<_>>())
        }
        None => {
            let rows: Vec<Score> = scores::table
                .filter(scores::shape.eq(query.shape.sides() as i32))
                .filter(scores::container.eq(query.container.sides() as i32))
                .distinct_on(scores::n)
                .order((
                    scores::n.asc(),
                    scores::side.asc(),
                    scores::submitted_at.asc(),
                    scores::id.asc(),
                ))
                .limit(limit)
                .select(Score::as_select())
                .load(conn)?;
            Ok(rows.iter().map(|s| entry(s, 1)).collect())
        }
    })
    .await?;
    Ok(Json(rows))
}

/// Public account records, with identity resolved through users, not display names.
pub(crate) fn personal_records(
    conn: &mut PgConnection,
    player: &str,
    shape: shared::Shape,
    container: shared::Shape,
    n: Option<u32>,
    limit: i64,
) -> QueryResult<Vec<ScoreEntry>> {
    use crate::schema::users;
    let owner: Option<Uuid> = users::table
        .filter(users::username.eq(player))
        .filter(users::kind.eq("player"))
        .select(users::id)
        .first(conn)
        .optional()?;
    let Some(owner) = owner else {
        return Ok(Vec::new());
    };
    let mut selection = scores::table
        .distinct_on(scores::n)
        .order((
            scores::n.asc(),
            scores::side.asc(),
            scores::submitted_at.asc(),
            scores::id.asc(),
        ))
        .filter(scores::user_id.eq(owner))
        .filter(scores::shape.eq(shape.sides() as i32))
        .filter(scores::container.eq(container.sides() as i32))
        .into_boxed();
    if let Some(n) = n {
        selection = selection.filter(scores::n.eq(n as i32));
    }
    let rows: Vec<Score> = selection
        .limit(limit)
        .select(Score::as_select())
        .load(conn)?;
    rows.iter()
        .map(|s| rank_of(conn, s).map(|rank| entry(s, rank)))
        .collect()
}

pub async fn detail(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> HandlerResult<ScoreDetail> {
    let found = with_conn(&state, move |conn| {
        let score: Option<Score> = scores::table
            .find(id)
            .select(Score::as_select())
            .first(conn)
            .optional()?;
        match score {
            Some(score) => {
                let rank = rank_of(conn, &score)?;
                Ok(Some((entry(&score, rank), score.arrangement)))
            }
            None => Ok(None),
        }
    })
    .await?;
    let (entry, arrangement) = found.ok_or_else(|| HandlerError::not_found("score not found"))?;
    let arrangement: Arrangement = serde_json::from_value(arrangement)?;
    Ok(Json(ScoreDetail { entry, arrangement }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::Placement;

    fn packing(squares: Vec<(f64, f64)>, side: f64) -> Arrangement {
        Arrangement {
            container: shared::Shape::Square,
            shape: shared::Shape::Square,
            n: squares.len() as u32,
            side,
            squares: squares
                .into_iter()
                .map(|(cx, cy)| Placement { cx, cy, theta: 0.0 })
                .collect(),
        }
    }

    /// serde_json's default float parser can land a ULP off the double the
    /// shortest-repr text came from; `float_roundtrip` makes it exact.
    #[test]
    fn json_floats_round_trip_bit_exactly() {
        use crate::test_support::{arrangement_of, awkward_floats, float_bits};
        let exact: f64 = serde_json::from_str("2.5115089416503906").unwrap();
        assert_eq!(exact.to_bits(), 2.5115089416503906_f64.to_bits());
        for floats in awkward_floats(4000).chunks(7) {
            let arrangement = arrangement_of(floats);
            let bits = float_bits(&arrangement);
            let text = serde_json::to_string(&arrangement).unwrap();
            let back: Arrangement = serde_json::from_str(&text).unwrap();
            assert_eq!(float_bits(&back), bits, "{text}");
            // As the handler reads jsonb: through a `Value`.
            let value: serde_json::Value = serde_json::from_str(&text).unwrap();
            let back: Arrangement = serde_json::from_value(value).unwrap();
            assert_eq!(float_bits(&back), bits, "{text}");
            // A submission carries its doubles as board-code bits, which
            // JSON passes through as a string.
            let body = SubmitScore {
                board: shared::BoardCode {
                    n: arrangement.n,
                    code: board::encode(&arrangement, &[]),
                },
            };
            let text = serde_json::to_string(&body).unwrap();
            let back: SubmitScore = serde_json::from_str(&text).unwrap();
            assert_eq!(back, body, "{text}");
        }
    }

    #[test]
    fn entries_say_whether_an_account_submitted_them() {
        let score = |user_id| Score {
            container: 4,
            shape: 4,
            id: Uuid::new_v4(),
            player: "ada".into(),
            n: 1,
            side: 1.0,
            arrangement: serde_json::Value::Null,
            submitted_at: chrono::DateTime::from_timestamp(0, 0).unwrap().naive_utc(),
            user_id,
            board_token: "0123456789abcdef01234567".into(),
            glue_recorded: true,
        };
        assert!(entry(&score(Some(Uuid::new_v4())), 1).account);
        assert!(!entry(&score(None), 1).account);
    }

    #[test]
    fn accepts_a_valid_packing() {
        assert!(check_submission(&packing(vec![(0.5, 0.5), (1.5, 0.5)], 2.0)).is_ok());
    }

    #[test]
    fn rejects_overlap_and_empty() {
        let overlap = packing(vec![(0.5, 0.5), (1.2, 0.5)], 2.0);
        assert_eq!(
            check_submission(&overlap).unwrap_err().0,
            StatusCode::BAD_REQUEST
        );
        assert!(check_submission(&packing(vec![], 1.0)).is_err());
    }
}
