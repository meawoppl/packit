use crate::models::{NewScore, Score};
use crate::schema::scores;
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use diesel::prelude::*;
use shared::geometry;
use shared::{
    ApiError, Arrangement, ScoreDetail, ScoreEntry, ScoresQuery, SubmitScore, MAX_N, VALIDATION_TOL,
};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

pub const MAX_PLAYER_LEN: usize = 32;
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
        id: score.id,
        player: score.player.clone(),
        n: score.n as u32,
        side: score.side,
        submitted_at: score.submitted_at,
        rank,
    }
}

/// Check a submission before it touches the database.
pub fn check_submission(body: &SubmitScore) -> Result<String, HandlerError> {
    let player = body.player.trim();
    if player.is_empty() || player.chars().count() > MAX_PLAYER_LEN {
        return Err(HandlerError::bad_request(format!(
            "player name must be 1 to {MAX_PLAYER_LEN} characters"
        )));
    }
    let arr = &body.arrangement;
    if !(1..=MAX_N).contains(&arr.n) {
        return Err(HandlerError::bad_request(format!(
            "n must be between 1 and {MAX_N}"
        )));
    }
    geometry::validate(arr, VALIDATION_TOL)
        .map_err(|v| HandlerError::bad_request(format!("invalid packing: {v}")))?;
    Ok(player.to_string())
}

pub async fn submit(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SubmitScore>,
) -> HandlerResult<ScoreEntry> {
    let player = check_submission(&body)?;
    let arr = body.arrangement;
    let new = NewScore {
        player,
        n: arr.n as i32,
        side: arr.side,
        arrangement: serde_json::to_value(&arr)?,
    };
    let result = with_conn(&state, move |conn| {
        let score: Score = diesel::insert_into(scores::table)
            .values(&new)
            .returning(Score::as_returning())
            .get_result(conn)?;
        let rank = rank_of(conn, &score)?;
        Ok(entry(&score, rank))
    })
    .await?;
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

/// With `n`: the leaderboard for that `n`. Without: the current record holder
/// for every `n` that has submissions.
pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ScoresQuery>,
) -> HandlerResult<Vec<ScoreEntry>> {
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT) as i64;
    let rows = with_conn(&state, move |conn| match query.n {
        Some(n) => {
            let rows: Vec<Score> = scores::table
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

    fn submission(player: &str, squares: Vec<(f64, f64)>, side: f64) -> SubmitScore {
        SubmitScore {
            player: player.to_string(),
            arrangement: Arrangement {
                n: squares.len() as u32,
                side,
                squares: squares
                    .into_iter()
                    .map(|(cx, cy)| Placement { cx, cy, theta: 0.0 })
                    .collect(),
            },
        }
    }

    #[test]
    fn accepts_valid_packing_and_trims_name() {
        let body = submission("  ada ", vec![(0.5, 0.5), (1.5, 0.5)], 2.0);
        assert_eq!(check_submission(&body).unwrap(), "ada");
    }

    #[test]
    fn rejects_bad_names() {
        let body = submission("   ", vec![(0.5, 0.5)], 1.0);
        assert!(check_submission(&body).is_err());
        let body = submission(&"x".repeat(MAX_PLAYER_LEN + 1), vec![(0.5, 0.5)], 1.0);
        assert!(check_submission(&body).is_err());
    }

    #[test]
    fn rejects_overlap_and_empty() {
        let body = submission("ada", vec![(0.5, 0.5), (1.2, 0.5)], 2.0);
        assert_eq!(
            check_submission(&body).unwrap_err().0,
            StatusCode::BAD_REQUEST
        );
        let body = submission("ada", vec![], 1.0);
        assert!(check_submission(&body).is_err());
    }
}
