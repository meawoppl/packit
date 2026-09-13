//! Session and ceremony cookies, and session lookup.
//!
//! The session cookie holds a random 256-bit token; the database stores only
//! its SHA-256. Over https the cookies use the `__Host-` prefix, which pins
//! them to this exact host with `Secure`, `Path=/` and no `Domain`. Plain http
//! is possible only in dev mode on localhost, where the prefix (which
//! requires `Secure`) is dropped.

use super::{hex, random_bytes, CEREMONY_TTL};
use crate::handlers::scores::{with_conn, HandlerError};
use crate::schema::{sessions, users};
use crate::AppState;
use axum::http::StatusCode;
use chrono::{DateTime, TimeDelta, Utc};
use diesel::prelude::*;
use sha2::{Digest, Sha256};
use tower_cookies::cookie::{time, SameSite};
use tower_cookies::{Cookie, Cookies};
use uuid::Uuid;

/// Sessions last 30 days from sign-in and are never extended.
const SESSION_DAYS: i64 = 30;

/// Adding a passkey requires a sign-in at most this old.
pub const REAUTH_WINDOW: TimeDelta = TimeDelta::minutes(5);

pub fn session_cookie_name(https: bool) -> &'static str {
    if https {
        "__Host-packit_session"
    } else {
        "packit_session"
    }
}

pub fn ceremony_cookie_name(https: bool) -> &'static str {
    if https {
        "__Host-packit_ceremony"
    } else {
        "packit_ceremony"
    }
}

pub fn session_cookie(https: bool, value: String) -> Cookie<'static> {
    Cookie::build((session_cookie_name(https), value))
        .http_only(true)
        .secure(https)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(time::Duration::days(SESSION_DAYS))
        .build()
}

/// The per-browser nonce a ceremony is bound to. Only our own pages ever
/// send it, so it can be `Strict`.
pub fn ceremony_cookie(https: bool, value: String) -> Cookie<'static> {
    Cookie::build((ceremony_cookie_name(https), value))
        .http_only(true)
        .secure(https)
        .same_site(SameSite::Strict)
        .path("/")
        .max_age(time::Duration::seconds(CEREMONY_TTL.as_secs() as i64))
        .build()
}

/// An expired copy of `cookie`. It keeps the attributes, since browsers only
/// let a `Secure` cookie overwrite a `__Host-` one.
pub fn removal(mut cookie: Cookie<'static>) -> Cookie<'static> {
    cookie.make_removal();
    cookie
}

/// A 32-byte value from the named cookie, if present and well formed.
pub fn read_token(cookies: &Cookies, name: &str) -> Option<[u8; 32]> {
    cookies.get(name).and_then(|c| hex::decode(c.value()))
}

pub fn hash_token(token: &[u8; 32]) -> Vec<u8> {
    Sha256::digest(token).to_vec()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub id: Uuid,
    pub username: String,
}

pub struct Session {
    pub user: User,
    pub created_at: DateTime<Utc>,
}

/// Start a session for `user_id` and return its token. The session the
/// browser presented, if any, is deleted, and so are expired sessions.
pub fn create(
    conn: &mut PgConnection,
    user_id: Uuid,
    previous: Option<[u8; 32]>,
) -> QueryResult<[u8; 32]> {
    let now = Utc::now();
    diesel::delete(sessions::table.filter(sessions::expires_at.le(now))).execute(conn)?;
    if let Some(previous) = previous {
        diesel::delete(sessions::table.filter(sessions::token_hash.eq(hash_token(&previous))))
            .execute(conn)?;
    }
    let token = random_bytes();
    diesel::insert_into(sessions::table)
        .values((
            sessions::token_hash.eq(hash_token(&token)),
            sessions::user_id.eq(user_id),
            sessions::created_at.eq(now),
            sessions::expires_at.eq(now + TimeDelta::days(SESSION_DAYS)),
        ))
        .execute(conn)?;
    Ok(token)
}

/// The live session this browser presents, if any. An expired session is
/// deleted when it is seen.
pub async fn current_session(
    state: &AppState,
    cookies: &Cookies,
) -> Result<Option<Session>, HandlerError> {
    let name = session_cookie_name(state.auth.origin.https);
    let Some(token) = read_token(cookies, name) else {
        return Ok(None);
    };
    let hash = hash_token(&token);
    with_conn(state, move |conn| {
        let row = sessions::table
            .inner_join(users::table)
            .filter(sessions::token_hash.eq(&hash))
            .select((
                users::id,
                users::username,
                users::kind,
                sessions::created_at,
                sessions::expires_at,
            ))
            .first::<(Uuid, String, String, DateTime<Utc>, DateTime<Utc>)>(conn)
            .optional()?;
        match row {
            Some((id, username, kind, created_at, expires_at))
                if kind == "player" && expires_at > Utc::now() =>
            {
                Ok(Some(Session {
                    user: User { id, username },
                    created_at,
                }))
            }
            Some(_) => {
                diesel::delete(sessions::table.filter(sessions::token_hash.eq(&hash)))
                    .execute(conn)?;
                Ok(None)
            }
            None => Ok(None),
        }
    })
    .await
}

/// The signed-in user, if any.
pub async fn current_user(
    state: &AppState,
    cookies: &Cookies,
) -> Result<Option<User>, HandlerError> {
    Ok(current_session(state, cookies).await?.map(|s| s.user))
}

/// The account a write is credited to, or a 401 whose message says what
/// signing in is needed for. Routes that call this must also require the
/// exact `Origin` (`handlers::auth::require_origin`).
pub async fn require_user(
    state: &AppState,
    cookies: &Cookies,
    why: &'static str,
) -> Result<User, HandlerError> {
    current_user(state, cookies)
        .await?
        .ok_or_else(|| HandlerError::new(StatusCode::UNAUTHORIZED, why))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_cookie_is_host_only_and_locked_down() {
        let c = session_cookie(true, hex::encode(&[0xab; 32]));
        assert_eq!(c.name(), "__Host-packit_session");
        assert_eq!(c.value(), "ab".repeat(32));
        assert_eq!(c.http_only(), Some(true));
        assert_eq!(c.secure(), Some(true));
        assert_eq!(c.same_site(), Some(SameSite::Lax));
        assert_eq!(c.path(), Some("/"));
        assert_eq!(c.domain(), None);
        assert_eq!(c.max_age(), Some(time::Duration::days(30)));
        let header = c.to_string();
        for part in [
            "HttpOnly",
            "SameSite=Lax",
            "Secure",
            "Path=/",
            "Max-Age=2592000",
        ] {
            assert!(header.contains(part), "{header}");
        }
        assert!(!header.contains("Domain"), "{header}");
    }

    #[test]
    fn dev_http_cookies_drop_the_prefix_and_secure() {
        let c = session_cookie(false, "v".into());
        assert_eq!(c.name(), "packit_session");
        assert_eq!(c.secure(), Some(false));
        assert_eq!(c.http_only(), Some(true));
        assert!(!c.to_string().contains("Secure"));
        let c = ceremony_cookie(false, "v".into());
        assert_eq!(c.name(), "packit_ceremony");
        assert!(!c.to_string().contains("Secure"));
    }

    #[test]
    fn ceremony_cookie_is_strict_and_short_lived() {
        let c = ceremony_cookie(true, "v".into());
        assert_eq!(c.name(), "__Host-packit_ceremony");
        assert_eq!(c.same_site(), Some(SameSite::Strict));
        assert_eq!(c.secure(), Some(true));
        assert_eq!(c.http_only(), Some(true));
        assert_eq!(c.path(), Some("/"));
        assert_eq!(c.domain(), None);
        assert_eq!(c.max_age(), Some(time::Duration::minutes(5)));
    }

    #[test]
    fn removal_expires_the_same_cookie() {
        for https in [true, false] {
            let r = removal(session_cookie(https, "v".into()));
            assert_eq!(r.name(), session_cookie_name(https));
            assert_eq!(r.value(), "");
            assert_eq!(r.max_age(), Some(time::Duration::ZERO));
            assert_eq!(r.secure(), Some(https));
            assert_eq!(r.http_only(), Some(true));
            assert_eq!(r.path(), Some("/"));
            assert!(r.to_string().contains("Max-Age=0"));
        }
    }

    #[test]
    fn stores_only_the_token_digest() {
        let token = [5u8; 32];
        let hash = hash_token(&token);
        assert_eq!(hash.len(), 32);
        assert_ne!(hash, token);
        assert_eq!(hash, Sha256::digest(token).to_vec());
    }
}
