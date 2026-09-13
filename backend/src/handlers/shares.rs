//! Durable, immutable short links for snapshots, including unfinished packings.

use super::scores::{with_conn, HandlerError};
use crate::{schema::solution_shares as shares, AppState};
use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use diesel::prelude::*;
use sha2::{Digest, Sha256};
use shared::{share, CreateShare, ShortShare};
use std::sync::Arc;
use uuid::Uuid;

pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateShare>,
) -> Result<Json<ShortShare>, HandlerError> {
    // Decode validates n, exact length, finite values, coordinate bounds and
    // glue before allocating. Overlap is allowed here; this never submits a
    // score.
    let snapshot = share::decode(&body.code, body.n).map_err(HandlerError::bad_request)?;
    let code = share::encode(&snapshot.arrangement, &snapshot.glues);
    let hash = Sha256::digest(code.as_bytes()).to_vec();
    let n = snapshot.arrangement.n;
    let token = with_conn(&state, move |conn| {
        // An insert handles both concurrent duplicate requests and token
        // collisions. A token collision retries; a digest match must also
        // match the full payload so it can never resolve to the wrong scene.
        for _ in 0..4 {
            // Skip the UUID version/variant bytes: keep 96 random bits.
            let random = Uuid::new_v4();
            let token = random.as_bytes()[..6]
                .iter()
                .chain(&random.as_bytes()[9..15])
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            let inserted = diesel::insert_into(shares::table)
                .values((
                    shares::token.eq(&token),
                    shares::payload_hash.eq(&hash),
                    shares::n.eq(n as i32),
                    shares::code.eq(&code),
                ))
                .on_conflict_do_nothing()
                .returning(shares::token)
                .get_result::<String>(conn)
                .optional()?;
            if let Some(token) = inserted {
                return Ok(token);
            }
            let existing = shares::table
                .filter(shares::payload_hash.eq(&hash))
                .select((shares::token, shares::code))
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
    })
    .await?;
    Ok(Json(ShortShare {
        url: format!("{}/s/{token}", state.public_url),
    }))
}

pub async fn resolve(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
) -> Result<Response, HandlerError> {
    if token.len() != 24 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(HandlerError::not_found("share link not found"));
    }
    let found = with_conn(&state, move |conn| {
        shares::table
            .find(token)
            .select((shares::n, shares::code))
            .first::<(i32, String)>(conn)
            .optional()
    })
    .await?
    .ok_or_else(|| HandlerError::not_found("share link not found"))?;
    // Redirects are followed by social preview crawlers; the destination
    // serves the existing per-solution OG tags and immutable preview PNG.
    let location = format!("{}/play/{}?s={}", state.public_url, found.0, found.1);
    Ok((
        StatusCode::FOUND,
        [
            (header::LOCATION, location),
            (header::CACHE_CONTROL, "public, max-age=86400".into()),
        ],
    )
        .into_response())
}
