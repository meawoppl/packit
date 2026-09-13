//! `/api/auth`: passkey registration, sign-in, sessions and extra passkeys.
//!
//! Every POST here must carry the site's exact `Origin`, start and finish
//! endpoints are rate limited per client address, there is no CORS, and no
//! response is cached. None of this gates the rest of the API.

use super::scores::{with_conn, HandlerError};
use crate::auth::ceremony::{Ceremony, CeremonyId, Operation};
use crate::auth::ratelimit::{client_ip, rate_key};
use crate::auth::session::{self, Session, REAUTH_WINDOW};
use crate::auth::{hex, random_bytes, username, Pending};
use crate::schema::{passkeys, sessions, users};
use crate::AppState;
use axum::extract::{ConnectInfo, DefaultBodyLimit, Request, State};
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use diesel::prelude::*;
use diesel::result::{DatabaseErrorKind, Error as DieselError};
use serde::{Deserialize, Serialize};
use shared::{AuthMe, AuthUsername};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tower_cookies::{CookieManagerLayer, Cookies};
use uuid::Uuid;
use webauthn_rs::prelude::{
    CreationChallengeResponse, Credential, CredentialID, Passkey, PublicKeyCredential,
    RegisterPublicKeyCredential, RequestChallengeResponse, WebauthnError,
};
use webauthn_rs_proto::ResidentKeyRequirement;

pub const SIGN_IN_FAILED: &str = "Sign-in failed";
pub const CEREMONY_INVALID: &str =
    "This passkey request expired or was already used; please start again";
pub const PASSKEY_TAKEN: &str = "That passkey is already registered";
pub const VERIFY_FAILED: &str = "Passkey verification failed";
pub const REAUTH: &str = "Sign in again to add a passkey";
pub const NOT_SIGNED_IN: &str = "Not signed in";

/// Options for the browser's WebAuthn call, and the handle to finish with.
#[derive(Serialize)]
pub struct Started<O> {
    pub ceremony: String,
    pub options: O,
}

/// The browser's WebAuthn result for a ceremony. The account and operation
/// come from the server-side ceremony, never from this body.
#[derive(Deserialize)]
pub struct Finish<C> {
    pub ceremony: String,
    pub credential: C,
}

pub fn router(state: Arc<AppState>) -> Router {
    let ceremonies = Router::new()
        .route("/api/auth/register/start", post(register_start))
        .route("/api/auth/register/finish", post(register_finish))
        .route("/api/auth/login/start", post(login_start))
        .route("/api/auth/login/finish", post(login_finish))
        .route("/api/auth/passkeys/start", post(add_passkey_start))
        .route("/api/auth/passkeys/finish", post(add_passkey_finish))
        .route_layer(middleware::from_fn_with_state(state.clone(), rate_limit));
    Router::new()
        .merge(ceremonies)
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/me", get(me))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_origin,
        ))
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(CookieManagerLayer::new())
        .layer(middleware::map_response(no_store))
        .with_state(state)
}

async fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// Refuse anything but GET and HEAD unless it carries exactly one `Origin`
/// equal to the configured origin. Browsers send `Origin` on every POST, so
/// a missing one means the request didn't come from our page.
async fn require_origin(State(state): State<Arc<AppState>>, req: Request, next: Next) -> Response {
    if req.method() != Method::GET && req.method() != Method::HEAD {
        let mut origins = req.headers().get_all(header::ORIGIN).iter();
        let expected = state.auth.origin.origin.as_bytes();
        let same_origin = matches!(
            (origins.next(), origins.next()),
            (Some(origin), None) if origin.as_bytes() == expected
        );
        if !same_origin {
            return HandlerError::new(StatusCode::FORBIDDEN, "Cross-origin request refused")
                .into_response();
        }
    }
    next.run(req).await
}

async fn rate_limit(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    req: Request,
    next: Next,
) -> Response {
    let ip = rate_key(client_ip(
        peer.ip(),
        req.headers(),
        state.auth.trusted_proxy,
    ));
    match state.auth.ip_limiter.check(&ip, Instant::now()) {
        Ok(()) => next.run(req).await,
        Err(wait) => too_many(wait),
    }
}

fn too_many(wait: Duration) -> Response {
    let secs = (wait.as_secs() + u64::from(wait.subsec_nanos() > 0)).max(1);
    let mut response = HandlerError::new(
        StatusCode::TOO_MANY_REQUESTS,
        "Too many attempts; try again later",
    )
    .into_response();
    response
        .headers_mut()
        .insert(header::RETRY_AFTER, HeaderValue::from(secs));
    response
}

fn sign_in_failed() -> HandlerError {
    HandlerError::new(StatusCode::UNAUTHORIZED, SIGN_IN_FAILED)
}

fn invalid_ceremony() -> HandlerError {
    HandlerError::new(StatusCode::BAD_REQUEST, CEREMONY_INVALID)
}

fn unavailable() -> HandlerError {
    HandlerError::new(StatusCode::CONFLICT, username::UNAVAILABLE)
}

fn verify_failed(e: WebauthnError) -> HandlerError {
    tracing::info!("passkey registration rejected: {e}");
    HandlerError::new(StatusCode::BAD_REQUEST, VERIFY_FAILED)
}

/// Bind a new ceremony to this browser with a fresh nonce cookie, and return
/// its id for the client.
fn begin(
    state: &AppState,
    cookies: &Cookies,
    op: Operation,
    user_id: Uuid,
    pending: Pending,
) -> Result<String, HandlerError> {
    let nonce = random_bytes();
    let id = state
        .auth
        .ceremonies
        .insert(op, nonce, user_id, pending, Instant::now())
        .ok_or_else(|| {
            HandlerError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "Too many sign-ins in progress; try again shortly",
            )
        })?;
    cookies.add(session::ceremony_cookie(
        state.auth.origin.https,
        hex::encode(&nonce),
    ));
    Ok(id.to_hex())
}

/// Consume the named ceremony. It must be live, for `op`, and bound to this
/// browser's nonce cookie; it is used up even if a check fails.
fn finish(
    state: &AppState,
    cookies: &Cookies,
    ceremony: &str,
    op: Operation,
) -> Result<Ceremony<Pending>, HandlerError> {
    let https = state.auth.origin.https;
    let nonce = session::read_token(cookies, session::ceremony_cookie_name(https));
    if cookies.get(session::ceremony_cookie_name(https)).is_some() {
        cookies.add(session::removal(session::ceremony_cookie(
            https,
            String::new(),
        )));
    }
    let id = CeremonyId::parse(ceremony).ok_or_else(invalid_ceremony)?;
    state
        .auth
        .ceremonies
        .take(id, op, nonce, Instant::now())
        .map_err(|e| {
            tracing::info!("auth ceremony refused: {e:?}");
            invalid_ceremony()
        })
}

fn presented_session(state: &AppState, cookies: &Cookies) -> Option<[u8; 32]> {
    session::read_token(
        cookies,
        session::session_cookie_name(state.auth.origin.https),
    )
}

fn set_session_cookie(state: &AppState, cookies: &Cookies, token: &[u8; 32]) {
    cookies.add(session::session_cookie(
        state.auth.origin.https,
        hex::encode(token),
    ));
}

async fn signed_in(state: &AppState, cookies: &Cookies) -> Result<Session, HandlerError> {
    session::current_session(state, cookies)
        .await?
        .ok_or_else(|| HandlerError::new(StatusCode::UNAUTHORIZED, NOT_SIGNED_IN))
}

/// Ask for a discoverable credential where the authenticator can make one.
/// The library only offers required or discouraged; this is client-side
/// guidance and doesn't change what finish verifies.
fn prefer_resident_key(options: &mut CreationChallengeResponse) {
    if let Some(selection) = options.public_key.authenticator_selection.as_mut() {
        selection.resident_key = Some(ResidentKeyRequirement::Preferred);
    }
}

/// Separate a unique-constraint violation, by constraint name, from other
/// query results.
fn unique_violation<T>(result: QueryResult<T>) -> QueryResult<Result<T, String>> {
    match result {
        Err(DieselError::DatabaseError(DatabaseErrorKind::UniqueViolation, info)) => {
            Ok(Err(info.constraint_name().unwrap_or_default().to_string()))
        }
        other => other.map(Ok),
    }
}

fn conflict(constraint: &str) -> HandlerError {
    match constraint {
        "users_username_key" => unavailable(),
        "passkeys_pkey" => HandlerError::new(StatusCode::CONFLICT, PASSKEY_TAKEN),
        other => {
            tracing::error!("unexpected unique violation on {other:?}");
            HandlerError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
        }
    }
}

async fn register_start(
    State(state): State<Arc<AppState>>,
    cookies: Cookies,
    Json(body): Json<AuthUsername>,
) -> Result<Json<Started<CreationChallengeResponse>>, HandlerError> {
    let name = username::normalize(&body.username).map_err(HandlerError::bad_request)?;
    if username::is_reserved(&name) {
        return Err(unavailable());
    }
    // A best-effort pre-check; the unique index decides at finish.
    let lookup = name.clone();
    let taken = with_conn(&state, move |conn| {
        diesel::select(diesel::dsl::exists(
            users::table.filter(users::username.eq(lookup)),
        ))
        .get_result::<bool>(conn)
    })
    .await?;
    if taken {
        return Err(unavailable());
    }
    let user_id = Uuid::new_v4();
    let (mut options, registration) = state
        .auth
        .webauthn
        .start_passkey_registration(user_id, &name, &name, None)?;
    prefer_resident_key(&mut options);
    let pending = Pending::Register {
        username: name,
        state: registration,
    };
    let ceremony = begin(&state, &cookies, Operation::Register, user_id, pending)?;
    Ok(Json(Started { ceremony, options }))
}

async fn register_finish(
    State(state): State<Arc<AppState>>,
    cookies: Cookies,
    Json(body): Json<Finish<RegisterPublicKeyCredential>>,
) -> Result<Json<AuthMe>, HandlerError> {
    let ceremony = finish(&state, &cookies, &body.ceremony, Operation::Register)?;
    let Pending::Register {
        username: name,
        state: registration,
    } = ceremony.state
    else {
        return Err(invalid_ceremony());
    };
    let passkey = state
        .auth
        .webauthn
        .finish_passkey_registration(&body.credential, &registration)
        .map_err(verify_failed)?;
    let user_id = ceremony.user_id;
    let credential_id = passkey.cred_id().to_vec();
    let stored = serde_json::to_value(&passkey)?;
    let previous = presented_session(&state, &cookies);
    let row_name = name.clone();
    let token = with_conn(&state, move |conn| {
        unique_violation(conn.transaction(|conn| {
            diesel::insert_into(users::table)
                .values((
                    users::id.eq(user_id),
                    users::username.eq(&row_name),
                    users::kind.eq("player"),
                ))
                .execute(conn)?;
            diesel::insert_into(passkeys::table)
                .values((
                    passkeys::credential_id.eq(&credential_id),
                    passkeys::user_id.eq(user_id),
                    passkeys::passkey.eq(&stored),
                ))
                .execute(conn)?;
            session::create(conn, user_id, previous)
        }))
    })
    .await?
    .map_err(|constraint| conflict(&constraint))?;
    set_session_cookie(&state, &cookies, &token);
    tracing::info!("registered {name}");
    Ok(Json(AuthMe { username: name }))
}

async fn login_start(
    State(state): State<Arc<AppState>>,
    cookies: Cookies,
    Json(body): Json<AuthUsername>,
) -> Result<Response, HandlerError> {
    let Ok(name) = username::normalize(&body.username) else {
        return Err(sign_in_failed());
    };
    if let Err(wait) = state.auth.username_limiter.check(&name, Instant::now()) {
        return Ok(too_many(wait));
    }
    let rows = with_conn(&state, move |conn| {
        passkeys::table
            .inner_join(users::table)
            .filter(users::username.eq(name))
            .filter(users::kind.eq("player"))
            .select((users::id, passkeys::passkey))
            .load::<(Uuid, serde_json::Value)>(conn)
    })
    .await?;
    let Some(user_id) = rows.first().map(|row| row.0) else {
        return Err(sign_in_failed());
    };
    let credentials = rows
        .into_iter()
        .map(|(_, passkey)| serde_json::from_value::<Passkey>(passkey))
        .collect::<Result<Vec<_>, _>>()?;
    let (options, authentication) = state
        .auth
        .webauthn
        .start_passkey_authentication(&credentials)?;
    let ceremony = begin(
        &state,
        &cookies,
        Operation::Login,
        user_id,
        Pending::Login(authentication),
    )?;
    Ok(Json(Started::<RequestChallengeResponse> { ceremony, options }).into_response())
}

async fn login_finish(
    State(state): State<Arc<AppState>>,
    cookies: Cookies,
    Json(body): Json<Finish<PublicKeyCredential>>,
) -> Result<Json<AuthMe>, HandlerError> {
    let ceremony = finish(&state, &cookies, &body.ceremony, Operation::Login)?;
    let Pending::Login(authentication) = ceremony.state else {
        return Err(invalid_ceremony());
    };
    let result = state
        .auth
        .webauthn
        .finish_passkey_authentication(&body.credential, &authentication)
        .map_err(|e| {
            tracing::info!("passkey sign-in rejected: {e}");
            sign_in_failed()
        })?;
    if !result.user_verified() {
        return Err(sign_in_failed());
    }
    let user_id = ceremony.user_id;
    let previous = presented_session(&state, &cookies);
    let signed_in_as = with_conn(&state, move |conn| {
        conn.transaction(|conn| {
            let credential_id = result.cred_id().to_vec();
            let row = passkeys::table
                .inner_join(users::table)
                .filter(passkeys::credential_id.eq(&credential_id))
                .filter(passkeys::user_id.eq(user_id))
                .filter(users::kind.eq("player"))
                .select((passkeys::passkey, users::username))
                .for_update()
                .first::<(serde_json::Value, String)>(conn)
                .optional()?;
            let Some((stored, name)) = row else {
                return Ok(None);
            };
            let mut passkey: Passkey = serde_json::from_value(stored)
                .map_err(|e| DieselError::DeserializationError(Box::new(e)))?;
            // The library compared the counter with the credential as it was
            // at login start. Apply its rule again to the locked row, so two
            // assertions checked against the same snapshot can't both pass.
            let counter = Credential::from(passkey.clone()).counter;
            if (result.counter() > 0 || counter > 0) && result.counter() <= counter {
                tracing::warn!("passkey counter did not advance; possible cloned authenticator");
                return Ok(None);
            }
            if passkey.update_credential(&result).is_none() {
                return Ok(None);
            }
            let updated = serde_json::to_value(&passkey)
                .map_err(|e| DieselError::SerializationError(Box::new(e)))?;
            diesel::update(passkeys::table.filter(passkeys::credential_id.eq(&credential_id)))
                .set((
                    passkeys::passkey.eq(updated),
                    passkeys::last_used_at.eq(Utc::now()),
                ))
                .execute(conn)?;
            let token = session::create(conn, user_id, previous)?;
            Ok(Some((name, token)))
        })
    })
    .await?;
    let (name, token) = signed_in_as.ok_or_else(sign_in_failed)?;
    set_session_cookie(&state, &cookies, &token);
    Ok(Json(AuthMe { username: name }))
}

async fn add_passkey_start(
    State(state): State<Arc<AppState>>,
    cookies: Cookies,
) -> Result<Json<Started<CreationChallengeResponse>>, HandlerError> {
    let session = signed_in(&state, &cookies).await?;
    if Utc::now() - session.created_at > REAUTH_WINDOW {
        return Err(HandlerError::new(StatusCode::FORBIDDEN, REAUTH));
    }
    let user = session.user;
    let owner = user.id;
    let existing = with_conn(&state, move |conn| {
        passkeys::table
            .filter(passkeys::user_id.eq(owner))
            .select(passkeys::credential_id)
            .load::<Vec<u8>>(conn)
    })
    .await?;
    let exclude = existing.into_iter().map(CredentialID::from).collect();
    let (mut options, registration) = state.auth.webauthn.start_passkey_registration(
        user.id,
        &user.username,
        &user.username,
        Some(exclude),
    )?;
    prefer_resident_key(&mut options);
    let ceremony = begin(
        &state,
        &cookies,
        Operation::AddPasskey,
        user.id,
        Pending::AddPasskey(registration),
    )?;
    Ok(Json(Started { ceremony, options }))
}

async fn add_passkey_finish(
    State(state): State<Arc<AppState>>,
    cookies: Cookies,
    Json(body): Json<Finish<RegisterPublicKeyCredential>>,
) -> Result<Json<AuthMe>, HandlerError> {
    let ceremony = finish(&state, &cookies, &body.ceremony, Operation::AddPasskey)?;
    let Pending::AddPasskey(registration) = ceremony.state else {
        return Err(invalid_ceremony());
    };
    // The passkey goes to the account the ceremony was started for, and only
    // while that account is still the one signed in here.
    let session = signed_in(&state, &cookies).await?;
    if session.user.id != ceremony.user_id {
        return Err(HandlerError::new(StatusCode::FORBIDDEN, REAUTH));
    }
    let passkey = state
        .auth
        .webauthn
        .finish_passkey_registration(&body.credential, &registration)
        .map_err(verify_failed)?;
    let owner = ceremony.user_id;
    let credential_id = passkey.cred_id().to_vec();
    let stored = serde_json::to_value(&passkey)?;
    with_conn(&state, move |conn| {
        unique_violation(
            diesel::insert_into(passkeys::table)
                .values((
                    passkeys::credential_id.eq(credential_id),
                    passkeys::user_id.eq(owner),
                    passkeys::passkey.eq(stored),
                ))
                .execute(conn),
        )
    })
    .await?
    .map_err(|constraint| conflict(&constraint))?;
    tracing::info!("added a passkey for {}", session.user.username);
    Ok(Json(AuthMe {
        username: session.user.username,
    }))
}

async fn logout(
    State(state): State<Arc<AppState>>,
    cookies: Cookies,
) -> Result<StatusCode, HandlerError> {
    if let Some(token) = presented_session(&state, &cookies) {
        let hash = session::hash_token(&token);
        with_conn(&state, move |conn| {
            diesel::delete(sessions::table.filter(sessions::token_hash.eq(hash))).execute(conn)
        })
        .await?;
    }
    cookies.add(session::removal(session::session_cookie(
        state.auth.origin.https,
        String::new(),
    )));
    Ok(StatusCode::NO_CONTENT)
}

async fn me(
    State(state): State<Arc<AppState>>,
    cookies: Cookies,
) -> Result<Json<AuthMe>, HandlerError> {
    session::current_user(&state, &cookies)
        .await?
        .map(|user| {
            Json(AuthMe {
                username: user.username,
            })
        })
        .ok_or_else(|| HandlerError::new(StatusCode::UNAUTHORIZED, NOT_SIGNED_IN))
}
