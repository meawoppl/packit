//! Board states: every stored scene, shared or submitted, including
//! unfinished packings. Rows are immutable and deduplicated by code, and
//! `/s/:token` opens one.

use super::scores::{with_conn, HandlerError};
use crate::auth::session;
use crate::{schema::board_states as boards, AppState};
use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use diesel::prelude::*;
use sha2::{Digest, Sha256};
use shared::board::{self, BoardState};
use shared::{BoardCode, BoardLink};
use std::sync::Arc;
use tower_cookies::Cookies;
use uuid::Uuid;

#[cfg(test)]
mod tests;

pub const SIGN_IN_TO_SHARE: &str = "Sign in to create a short link";

/// A fresh short token: 96 random bits, the bytes of a v4 UUID that skip its
/// version and variant bits.
fn new_token() -> String {
    let random = Uuid::new_v4();
    random.as_bytes()[..6]
        .iter()
        .chain(&random.as_bytes()[9..15])
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The token of `board`, stored with `creator` unless the same code is
/// stored already. A duplicate is returned untouched: its creator, or lack
/// of one, stays, and every caller gets the same token. Runs inside the
/// caller's transaction, if there is one.
pub(crate) fn store(
    conn: &mut PgConnection,
    board: &BoardState,
    creator: Uuid,
) -> QueryResult<String> {
    let code = board::encode(&board.arrangement, &board.glues);
    let hash = Sha256::digest(code.as_bytes()).to_vec();
    // An insert handles both concurrent duplicate requests and token
    // collisions. A token collision retries; a digest match must also
    // match the full payload so it can never resolve to the wrong scene.
    for _ in 0..4 {
        let inserted = diesel::insert_into(boards::table)
            .values((
                boards::token.eq(new_token()),
                boards::payload_hash.eq(&hash),
                boards::n.eq(board.arrangement.n as i32),
                boards::code.eq(&code),
                boards::created_by.eq(creator),
            ))
            .on_conflict_do_nothing()
            .returning(boards::token)
            .get_result::<String>(conn)
            .optional()?;
        if let Some(token) = inserted {
            return Ok(token);
        }
        let existing = boards::table
            .filter(boards::payload_hash.eq(&hash))
            .select((boards::token, boards::code))
            .first::<(String, String)>(conn)
            .optional()?;
        if let Some((token, existing_code)) = existing {
            return if existing_code == code {
                Ok(token)
            } else {
                Err(diesel::result::Error::RollbackTransaction)
            };
        }
    }
    Err(diesel::result::Error::RollbackTransaction)
}

/// Save a board state for the signed-in account and return its short link.
pub async fn save(
    State(state): State<Arc<AppState>>,
    cookies: Cookies,
    Json(body): Json<BoardCode>,
) -> Result<Json<BoardLink>, HandlerError> {
    // Decode validates n, exact length, finite values, coordinate bounds and
    // glue before allocating, and before the session costs a query. Overlap
    // is allowed here; this never submits a score.
    let board = board::decode(&body.code, body.n).map_err(HandlerError::bad_request)?;
    let user = session::require_user(&state, &cookies, SIGN_IN_TO_SHARE).await?;
    let token = with_conn(&state, move |conn| store(conn, &board, user.id)).await?;
    Ok(Json(BoardLink {
        url: format!("{}/s/{token}", state.public_url),
    }))
}

pub async fn resolve(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
) -> Result<Response, HandlerError> {
    if token.len() != 24 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(HandlerError::not_found("board not found"));
    }
    let found = with_conn(&state, move |conn| {
        boards::table
            .find(token)
            .select((boards::n, boards::code))
            .first::<(i32, String)>(conn)
            .optional()
    })
    .await?
    .ok_or_else(|| HandlerError::not_found("board not found"))?;
    // Redirects are followed by social preview crawlers; the destination
    // serves the per-board OG tags and immutable preview PNG.
    let decoded = board::decode(&found.1, found.0 as u32).map_err(HandlerError::bad_request)?;
    let prefix = if decoded.arrangement.shape.is_square() {
        String::new()
    } else {
        format!("{}/", decoded.arrangement.shape)
    };
    let location = format!(
        "{}/play/{}{}?s={}",
        state.public_url, prefix, found.0, found.1
    );
    Ok((
        StatusCode::FOUND,
        [
            (header::LOCATION, location),
            (header::CACHE_CONTROL, "public, max-age=86400".into()),
        ],
    )
        .into_response())
}
