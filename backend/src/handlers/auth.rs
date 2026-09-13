//! `/api/auth`: passkey registration, sign-in, sessions and extra passkeys.
//!
//! Every POST here must carry the site's exact `Origin`, start and finish
//! endpoints are rate limited per client, there is no CORS, and no response
//! is cached. None of this gates the rest of the API.

use super::scores::{with_conn, HandlerError};
use crate::auth::ceremony::{Ceremony, CeremonyId, InsertError};
use crate::auth::proxy::ViaTrustedProxy;
use crate::auth::ratelimit::RateKey;
use crate::auth::session::{self, Session, REAUTH_WINDOW};
use crate::auth::{hex, random_bytes, username, Pending};
use crate::schema::{passkeys, sessions, users};
use crate::AppState;
use axum::extract::{ConnectInfo, DefaultBodyLimit, Request, State};
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
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
    CreationChallengeResponse, Credential, CredentialID, Passkey, PasskeyAuthentication,
    PasskeyRegistration, PublicKeyCredential, RegisterPublicKeyCredential,
    RequestChallengeResponse, WebauthnError,
};
use webauthn_rs_proto::ResidentKeyRequirement;

pub const SIGN_IN_FAILED: &str = "Sign-in failed";
pub const CEREMONY_INVALID: &str =
    "This passkey request expired or was already used; please start again";
pub const PASSKEY_TAKEN: &str = "That passkey is already registered";
pub const VERIFY_FAILED: &str = "Passkey verification failed";
pub const REAUTH: &str = "Sign in again to add a passkey";
pub const NOT_SIGNED_IN: &str = "Not signed in";
pub const TOO_MANY: &str = "Too many attempts; try again later";

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

/// Spend the client's rate-limit tokens, and hand its [`RateKey`] to the
/// handler for the per-client ceremony and username limits.
async fn rate_limit(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Extension(ViaTrustedProxy(trusted)): Extension<ViaTrustedProxy>,
    mut req: Request,
    next: Next,
) -> Response {
    let client = state.auth.client(peer.ip(), req.headers(), trusted);
    if let Err(wait) = state.auth.check_client(client, Instant::now()) {
        return too_many(wait).into_response();
    }
    req.extensions_mut().insert(client);
    next.run(req).await
}

fn too_many(wait: Duration) -> HandlerError {
    HandlerError::new(StatusCode::TOO_MANY_REQUESTS, TOO_MANY).retry_after(wait)
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

fn reauth() -> HandlerError {
    HandlerError::new(StatusCode::FORBIDDEN, REAUTH)
}

fn verify_failed(e: WebauthnError) -> HandlerError {
    tracing::info!("passkey registration rejected: {e}");
    HandlerError::new(StatusCode::BAD_REQUEST, VERIFY_FAILED)
}

/// Store a ceremony bound to this browser's nonce cookie, and return its id
/// for the client. A valid nonce cookie is reused, so a second tab or a
/// double submit doesn't strand a ceremony already in progress.
fn begin(
    state: &AppState,
    cookies: &Cookies,
    client: RateKey,
    user_id: Uuid,
    pending: Pending,
) -> Result<String, HandlerError> {
    let https = state.auth.origin.https;
    let nonce = session::read_token(cookies, session::ceremony_cookie_name(https))
        .unwrap_or_else(random_bytes);
    let id = state
        .auth
        .ceremonies
        .insert(client, nonce, user_id, pending, Instant::now())
        .map_err(|e| match e {
            InsertError::Busy(wait) => too_many(wait),
            InsertError::Full => HandlerError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "Too many sign-ins in progress; try again shortly",
            ),
        })?;
    cookies.add(session::ceremony_cookie(https, hex::encode(&nonce)));
    Ok(id.to_hex())
}

/// Consume the named ceremony. It must be live, bound to this browser's
/// nonce cookie, and for the operation `pick` accepts; it is used up even if
/// a check fails. The nonce cookie stays for the browser's other ceremonies.
fn finish<T>(
    state: &AppState,
    cookies: &Cookies,
    ceremony: &str,
    pick: impl FnOnce(Pending) -> Option<T>,
) -> Result<Ceremony<T>, HandlerError> {
    let https = state.auth.origin.https;
    let nonce = session::read_token(cookies, session::ceremony_cookie_name(https));
    let id = CeremonyId::parse(ceremony).ok_or_else(invalid_ceremony)?;
    state
        .auth
        .ceremonies
        .take(id, nonce, Instant::now(), pick)
        .map_err(|e| {
            tracing::info!("auth ceremony refused: {e:?}");
            invalid_ceremony()
        })
}

fn registration(pending: Pending) -> Option<(String, PasskeyRegistration)> {
    match pending {
        Pending::Register { username, state } => Some((username, state)),
        Pending::Login(_) | Pending::AddPasskey(_) => None,
    }
}

fn authentication(pending: Pending) -> Option<PasskeyAuthentication> {
    match pending {
        Pending::Login(state) => Some(state),
        Pending::Register { .. } | Pending::AddPasskey(_) => None,
    }
}

fn added_passkey(pending: Pending) -> Option<PasskeyRegistration> {
    match pending {
        Pending::AddPasskey(state) => Some(state),
        Pending::Register { .. } | Pending::Login(_) => None,
    }
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

/// Adding a passkey needs a sign-in no older than [`REAUTH_WINDOW`].
fn require_recent(session: &Session) -> Result<(), HandlerError> {
    if Utc::now() - session.created_at > REAUTH_WINDOW {
        return Err(reauth());
    }
    Ok(())
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
    Extension(client): Extension<RateKey>,
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
    let ceremony = begin(&state, &cookies, client, user_id, pending)?;
    Ok(Json(Started { ceremony, options }))
}

async fn register_finish(
    State(state): State<Arc<AppState>>,
    cookies: Cookies,
    Json(body): Json<Finish<RegisterPublicKeyCredential>>,
) -> Result<Json<AuthMe>, HandlerError> {
    let ceremony = finish(&state, &cookies, &body.ceremony, registration)?;
    let (name, registration) = ceremony.state;
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
    Extension(client): Extension<RateKey>,
    cookies: Cookies,
    Json(body): Json<AuthUsername>,
) -> Result<Json<Started<RequestChallengeResponse>>, HandlerError> {
    let Ok(name) = username::normalize(&body.username) else {
        return Err(sign_in_failed());
    };
    state
        .auth
        .check_username(&name, client, Instant::now())
        .map_err(too_many)?;
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
        client,
        user_id,
        Pending::Login(authentication),
    )?;
    Ok(Json(Started { ceremony, options }))
}

async fn login_finish(
    State(state): State<Arc<AppState>>,
    cookies: Cookies,
    Json(body): Json<Finish<PublicKeyCredential>>,
) -> Result<Json<AuthMe>, HandlerError> {
    let ceremony = finish(&state, &cookies, &body.ceremony, authentication)?;
    // Passkey authentication requires user verification, so a result here
    // is always user-verified.
    let result = state
        .auth
        .webauthn
        .finish_passkey_authentication(&body.credential, &ceremony.state)
        .map_err(|e| {
            tracing::info!("passkey sign-in rejected: {e}");
            sign_in_failed()
        })?;
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
    Extension(client): Extension<RateKey>,
    cookies: Cookies,
) -> Result<Json<Started<CreationChallengeResponse>>, HandlerError> {
    let session = signed_in(&state, &cookies).await?;
    require_recent(&session)?;
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
        client,
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
    let ceremony = finish(&state, &cookies, &body.ceremony, added_passkey)?;
    // The passkey goes to the account the ceremony was started for, and only
    // while that account is still signed in here, recently.
    let session = signed_in(&state, &cookies).await?;
    if session.user.id != ceremony.user_id {
        return Err(reauth());
    }
    require_recent(&session)?;
    let passkey = state
        .auth
        .webauthn
        .finish_passkey_registration(&body.credential, &ceremony.state)
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

/// Clear the cookie first, so the browser is signed out even if deleting the
/// session row fails.
async fn logout(
    State(state): State<Arc<AppState>>,
    cookies: Cookies,
) -> Result<StatusCode, HandlerError> {
    let presented = presented_session(&state, &cookies);
    cookies.add(session::removal(session::session_cookie(
        state.auth.origin.https,
        String::new(),
    )));
    if let Some(token) = presented {
        let hash = session::hash_token(&token);
        with_conn(&state, move |conn| {
            diesel::delete(sessions::table.filter(sessions::token_hash.eq(hash))).execute(conn)
        })
        .await?;
    }
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
