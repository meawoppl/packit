//! End-to-end passkey flows through the real router, with software
//! authenticators. Database tests skip when TEST_DATABASE_URL is unset.

use super::proxy::{ProxyToken, ProxyTrust, TOKEN_HEADER};
use super::{
    hex, random_bytes, session, username, CEREMONIES_PER_CLIENT, CEREMONIES_PER_SITE, IP_BURST,
    SITE_BURST, USERNAME_BURST,
};
use crate::build_app;
use crate::config::PublicOrigin;
use crate::handlers::auth::{
    CEREMONY_INVALID, NOT_SIGNED_IN, PASSKEY_TAKEN, REAUTH, SIGN_IN_FAILED, TOO_MANY, VERIFY_FAILED,
};
use crate::handlers::scores::SIGN_IN_TO_SUBMIT;
use crate::handlers::shares::SIGN_IN_TO_SHARE;
use crate::schema::{passkeys, scores, sessions, solution_shares, users};
use crate::test_support::{state_for, test_db, unconnected_pool, TEST_URL};
use crate::AppState;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, HeaderMap, Method, Request, StatusCode};
use axum::Router;
use chrono::{DateTime, TimeDelta, Utc};
use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use openssl::bn::{BigNum, BigNumContext};
use openssl::ec::{EcGroup, EcKey};
use openssl::hash::MessageDigest;
use openssl::nid::Nid;
use openssl::pkey::PKey;
use openssl::sign::Signer;
use serde::Serialize;
use serde_cbor_2::Value as Cbor;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;
use tower_cookies::cookie::time;
use tower_cookies::Cookie;
use uuid::Uuid;
use webauthn_authenticator_rs::error::WebauthnCError;
use webauthn_authenticator_rs::softpasskey::SoftPasskey;
use webauthn_authenticator_rs::{
    AuthenticatorBackend, AuthenticatorBackendHashedClientData, WebauthnAuthenticator,
};
use webauthn_rs::prelude::{
    Base64UrlSafeData, CreationChallengeResponse, Credential, Passkey, PublicKeyCredential,
    RegisterPublicKeyCredential, RequestChallengeResponse, Url,
};
use webauthn_rs_proto::{
    AuthenticatorAssertionResponseRaw, AuthenticatorAttestationResponseRaw,
    PublicKeyCredentialCreationOptions, PublicKeyCredentialRequestOptions, UserVerificationPolicy,
};

const SESSION: &str = "__Host-packit_session";
const CEREMONY: &str = "__Host-packit_ceremony";

const REGISTER_START: &str = "/api/auth/register/start";
const REGISTER_FINISH: &str = "/api/auth/register/finish";
const LOGIN_START: &str = "/api/auth/login/start";
const LOGIN_FINISH: &str = "/api/auth/login/finish";
const ADD_START: &str = "/api/auth/passkeys/start";
const ADD_FINISH: &str = "/api/auth/passkeys/finish";
const LOGOUT: &str = "/api/auth/logout";
const ME: &str = "/api/auth/me";

// ---------------------------------------------------------------------------
// Harness: a cookie-keeping browser, and authenticators
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct Resp {
    status: StatusCode,
    headers: HeaderMap,
    body: Value,
}

impl Resp {
    fn set_cookie(&self, name: &str) -> Option<String> {
        self.headers
            .get_all(header::SET_COOKIE)
            .iter()
            .map(|v| v.to_str().unwrap().to_owned())
            .find(|c| c.starts_with(&format!("{name}=")))
    }

    fn error(&self) -> &str {
        self.body["error"].as_str().unwrap_or_default()
    }
}

async fn call(app: &Router, req: Request<Body>) -> Resp {
    let res = app.clone().oneshot(req).await.unwrap();
    let (parts, body) = res.into_parts();
    let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into()))
    };
    Resp {
        status: parts.status,
        headers: parts.headers,
        body,
    }
}

/// One browser: its own address, the page origin it sends, and a cookie jar.
#[derive(Clone)]
struct Browser {
    ip: IpAddr,
    origin: Option<&'static str>,
    cookies: HashMap<String, String>,
}

impl Browser {
    fn new() -> Self {
        let r: [u8; 3] = random_bytes();
        Self {
            ip: IpAddr::from([10, r[0], r[1], r[2]]),
            origin: Some(TEST_URL),
            cookies: HashMap::new(),
        }
    }

    fn request(&self, method: Method, uri: &str, body: Option<&Value>) -> Request<Body> {
        let mut req = Request::builder().method(method).uri(uri);
        if let Some(origin) = self.origin {
            req = req.header(header::ORIGIN, origin);
        }
        if !self.cookies.is_empty() {
            let jar: Vec<_> = self
                .cookies
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect();
            req = req.header(header::COOKIE, jar.join("; "));
        }
        let mut req = match body {
            Some(body) => req
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(body).unwrap())),
            None => req.body(Body::empty()),
        }
        .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::new(self.ip, 40_000)));
        req
    }

    fn store(&mut self, headers: &HeaderMap) {
        for value in headers.get_all(header::SET_COOKIE) {
            let c = Cookie::parse(value.to_str().unwrap().to_owned()).unwrap();
            if c.max_age() == Some(time::Duration::ZERO) {
                self.cookies.remove(c.name());
            } else {
                self.cookies.insert(c.name().into(), c.value().into());
            }
        }
    }

    async fn send(&mut self, app: &Router, req: Request<Body>) -> Resp {
        let resp = call(app, req).await;
        self.store(&resp.headers);
        resp
    }

    async fn post(&mut self, app: &Router, uri: &str, body: Value) -> Resp {
        let req = self.request(Method::POST, uri, Some(&body));
        self.send(app, req).await
    }

    async fn get(&mut self, app: &Router, uri: &str) -> Resp {
        let req = self.request(Method::GET, uri, None);
        self.send(app, req).await
    }

    fn session_hash(&self) -> Vec<u8> {
        session::hash_token(&hex::decode(&self.cookies[SESSION]).unwrap())
    }
}

trait Client {
    fn create(
        &mut self,
        origin: &Url,
        options: CreationChallengeResponse,
    ) -> Result<RegisterPublicKeyCredential, WebauthnCError>;
    fn get(
        &mut self,
        origin: &Url,
        options: RequestChallengeResponse,
    ) -> Result<PublicKeyCredential, WebauthnCError>;
}

/// A software passkey behind the library's client, which checks the origin
/// against the RP ID like a browser. `SoftPasskey::new(true)` reports user
/// verification, as a real passkey does after its PIN or biometric.
fn soft() -> WebauthnAuthenticator<SoftPasskey> {
    WebauthnAuthenticator::new(SoftPasskey::new(true))
}

impl Client for WebauthnAuthenticator<SoftPasskey> {
    fn create(
        &mut self,
        origin: &Url,
        options: CreationChallengeResponse,
    ) -> Result<RegisterPublicKeyCredential, WebauthnCError> {
        self.do_registration(origin.clone(), options)
    }

    fn get(
        &mut self,
        origin: &Url,
        options: RequestChallengeResponse,
    ) -> Result<PublicKeyCredential, WebauthnCError> {
        self.do_authentication(origin.clone(), options)
    }
}

/// A minimal passkey with a fixed credential ID and a counter and backup
/// flags the test sets directly. SoftPasskey can't do any of that: its
/// credential IDs are random, and its counter starts at 1 and only goes up.
#[derive(Clone)]
struct TestKey {
    cred_id: Vec<u8>,
    private_der: Vec<u8>,
    counter: u32,
    backup_eligible: bool,
    backed_up: bool,
}

impl TestKey {
    fn new() -> Self {
        Self {
            cred_id: random_bytes::<16>().to_vec(),
            private_der: Vec::new(),
            counter: 0,
            backup_eligible: false,
            backed_up: false,
        }
    }

    /// RP ID hash, flags (UP and UV, plus backup bits) and counter.
    fn auth_data(&self, rp_id: &str, extra_flags: u8) -> Vec<u8> {
        let flags = 0x01
            | 0x04
            | if self.backup_eligible { 0x08 } else { 0 }
            | if self.backed_up { 0x10 } else { 0 }
            | extra_flags;
        let mut data = Sha256::digest(rp_id.as_bytes()).to_vec();
        data.push(flags);
        data.extend(self.counter.to_be_bytes());
        data
    }
}

impl AuthenticatorBackendHashedClientData for TestKey {
    fn perform_register(
        &mut self,
        _client_data_hash: Vec<u8>,
        options: PublicKeyCredentialCreationOptions,
        _timeout_ms: u32,
    ) -> Result<RegisterPublicKeyCredential, WebauthnCError> {
        let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1).unwrap();
        let key = EcKey::generate(&group).unwrap();
        let (mut x, mut y) = (BigNum::new().unwrap(), BigNum::new().unwrap());
        key.public_key()
            .affine_coordinates_gfp(&group, &mut x, &mut y, &mut BigNumContext::new().unwrap())
            .unwrap();
        self.private_der = key.private_key_to_der().unwrap();
        let cose = serde_cbor_2::to_vec(&Cbor::Map(BTreeMap::from([
            (Cbor::Integer(1), Cbor::Integer(2)),
            (Cbor::Integer(3), Cbor::Integer(-7)),
            (Cbor::Integer(-1), Cbor::Integer(1)),
            (Cbor::Integer(-2), Cbor::Bytes(x.to_vec_padded(32).unwrap())),
            (Cbor::Integer(-3), Cbor::Bytes(y.to_vec_padded(32).unwrap())),
        ])))
        .unwrap();
        // Attested credential data follows the header: AAGUID, ID, key.
        let mut auth_data = self.auth_data(&options.rp.id, 0x40);
        auth_data.extend([0; 16]);
        auth_data.extend((self.cred_id.len() as u16).to_be_bytes());
        auth_data.extend(&self.cred_id);
        auth_data.extend(cose);
        let attestation = serde_cbor_2::to_vec(&Cbor::Map(BTreeMap::from([
            (Cbor::Text("fmt".into()), Cbor::Text("none".into())),
            (Cbor::Text("attStmt".into()), Cbor::Map(BTreeMap::new())),
            (Cbor::Text("authData".into()), Cbor::Bytes(auth_data)),
        ])))
        .unwrap();
        Ok(RegisterPublicKeyCredential {
            id: b64(&self.cred_id),
            raw_id: self.cred_id.clone().into(),
            response: AuthenticatorAttestationResponseRaw {
                attestation_object: attestation.into(),
                client_data_json: Base64UrlSafeData::new(),
                transports: None,
            },
            type_: "public-key".into(),
            extensions: Default::default(),
        })
    }

    fn perform_auth(
        &mut self,
        client_data_hash: Vec<u8>,
        options: PublicKeyCredentialRequestOptions,
        _timeout_ms: u32,
    ) -> Result<PublicKeyCredential, WebauthnCError> {
        assert!(options
            .allow_credentials
            .iter()
            .any(|c| c.id == self.cred_id));
        let auth_data = self.auth_data(&options.rp_id, 0);
        let key =
            PKey::from_ec_key(EcKey::private_key_from_der(&self.private_der).unwrap()).unwrap();
        let mut signer = Signer::new(MessageDigest::sha256(), &key).unwrap();
        signer.update(&auth_data).unwrap();
        signer.update(&client_data_hash).unwrap();
        Ok(PublicKeyCredential {
            id: b64(&self.cred_id),
            raw_id: self.cred_id.clone().into(),
            response: AuthenticatorAssertionResponseRaw {
                authenticator_data: auth_data.into(),
                client_data_json: Base64UrlSafeData::new(),
                signature: signer.sign_to_vec().unwrap().into(),
                user_handle: None,
            },
            extensions: Default::default(),
            type_: "public-key".into(),
        })
    }
}

impl Client for TestKey {
    fn create(
        &mut self,
        origin: &Url,
        options: CreationChallengeResponse,
    ) -> Result<RegisterPublicKeyCredential, WebauthnCError> {
        AuthenticatorBackend::perform_register(self, origin.clone(), options.public_key, 60_000)
    }

    fn get(
        &mut self,
        origin: &Url,
        options: RequestChallengeResponse,
    ) -> Result<PublicKeyCredential, WebauthnCError> {
        AuthenticatorBackend::perform_auth(self, origin.clone(), options.public_key, 60_000)
    }
}

fn b64(bytes: &[u8]) -> String {
    serde_json::to_value(Base64UrlSafeData::from(bytes.to_vec()))
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

fn origin() -> Url {
    Url::parse(TEST_URL).unwrap()
}

fn evil() -> Url {
    Url::parse("https://evil.test").unwrap()
}

/// A username no other test run uses.
fn fresh(prefix: &str) -> String {
    format!("{prefix}-{}", &Uuid::new_v4().simple().to_string()[..12])
}

fn db_app() -> Option<(Arc<AppState>, Router)> {
    let Some(pool) = test_db() else {
        eprintln!("TEST_DATABASE_URL not set; skipping");
        return None;
    };
    let state = state_for(pool);
    Some((state.clone(), build_app(state)))
}

// ---------------------------------------------------------------------------
// Flows
// ---------------------------------------------------------------------------

async fn start(app: &Router, b: &mut Browser, path: &str, name: &str) -> Resp {
    b.post(app, path, json!({ "username": name })).await
}

fn creation(started: &Resp) -> CreationChallengeResponse {
    serde_json::from_value(started.body["options"].clone()).unwrap()
}

fn request_options(started: &Resp) -> RequestChallengeResponse {
    serde_json::from_value(started.body["options"].clone()).unwrap()
}

fn finish_body(started: &Resp, credential: &impl Serialize) -> Value {
    json!({ "ceremony": started.body["ceremony"], "credential": credential })
}

async fn register(app: &Router, b: &mut Browser, key: &mut impl Client, name: &str) -> Resp {
    let s = start(app, b, REGISTER_START, name).await;
    assert_eq!(s.status, StatusCode::OK, "{:?}", s.body);
    let cred = key.create(&origin(), creation(&s)).unwrap();
    b.post(app, REGISTER_FINISH, finish_body(&s, &cred)).await
}

async fn login(app: &Router, b: &mut Browser, key: &mut impl Client, name: &str) -> Resp {
    let s = start(app, b, LOGIN_START, name).await;
    assert_eq!(s.status, StatusCode::OK, "{:?}", s.body);
    let cred = key.get(&origin(), request_options(&s)).unwrap();
    b.post(app, LOGIN_FINISH, finish_body(&s, &cred)).await
}

async fn add_passkey(app: &Router, b: &mut Browser, key: &mut impl Client) -> Resp {
    let s = b.post(app, ADD_START, json!({})).await;
    assert_eq!(s.status, StatusCode::OK, "{:?}", s.body);
    let cred = key.create(&origin(), creation(&s)).unwrap();
    b.post(app, ADD_FINISH, finish_body(&s, &cred)).await
}

/// Send requests concurrently, each on its own task.
async fn race(app: &Router, reqs: Vec<Request<Body>>) -> Vec<Resp> {
    let tasks: Vec<_> = reqs
        .into_iter()
        .map(|req| {
            let app = app.clone();
            tokio::spawn(async move { call(&app, req).await })
        })
        .collect();
    let mut out = Vec::new();
    for task in tasks {
        out.push(task.await.unwrap());
    }
    out.sort_by_key(|r| r.status.as_u16());
    out
}

// ---------------------------------------------------------------------------
// Database helpers
// ---------------------------------------------------------------------------

fn conn(state: &AppState) -> PooledConnection<ConnectionManager<PgConnection>> {
    state.db_pool.get().unwrap()
}

fn user_id(state: &AppState, name: &str) -> Uuid {
    users::table
        .filter(users::username.eq(name))
        .select(users::id)
        .first(&mut conn(state))
        .unwrap()
}

fn user_count(state: &AppState, name: &str) -> i64 {
    users::table
        .filter(users::username.eq(name))
        .count()
        .get_result(&mut conn(state))
        .unwrap()
}

fn passkey_ids(state: &AppState, owner: Uuid) -> Vec<Vec<u8>> {
    passkeys::table
        .filter(passkeys::user_id.eq(owner))
        .select(passkeys::credential_id)
        .load(&mut conn(state))
        .unwrap()
}

fn session_hashes(state: &AppState, owner: Uuid) -> Vec<Vec<u8>> {
    sessions::table
        .filter(sessions::user_id.eq(owner))
        .select(sessions::token_hash)
        .load(&mut conn(state))
        .unwrap()
}

fn stored_credential(state: &AppState, cred_id: &[u8]) -> Credential {
    let stored: Value = passkeys::table
        .filter(passkeys::credential_id.eq(cred_id))
        .select(passkeys::passkey)
        .first(&mut conn(state))
        .unwrap();
    Credential::from(serde_json::from_value::<Passkey>(stored).unwrap())
}

fn last_used(state: &AppState, cred_id: &[u8]) -> Option<DateTime<Utc>> {
    passkeys::table
        .filter(passkeys::credential_id.eq(cred_id))
        .select(passkeys::last_used_at)
        .first(&mut conn(state))
        .unwrap()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn register_me_logout_roundtrip() {
    let Some((state, app)) = db_app() else { return };
    let name = fresh("ada");
    let mut b = Browser::new();
    let mut key = soft();

    let s = start(
        &app,
        &mut b,
        REGISTER_START,
        &format!("  {}  ", name.to_uppercase()),
    )
    .await;
    assert_eq!(s.status, StatusCode::OK, "{:?}", s.body);
    assert_eq!(s.headers[header::CACHE_CONTROL], "no-store");
    let opts = &s.body["options"]["publicKey"];
    assert_eq!(opts["rp"]["id"], "packit.test");
    assert_eq!(opts["user"]["name"], name.as_str());
    assert_eq!(opts["authenticatorSelection"]["residentKey"], "preferred");
    assert_eq!(
        opts["authenticatorSelection"]["userVerification"],
        "required"
    );
    assert_eq!(s.body["ceremony"].as_str().unwrap().len(), 32);
    let nonce = s.set_cookie(CEREMONY).unwrap();
    for part in [
        "HttpOnly",
        "Secure",
        "SameSite=Strict",
        "Path=/",
        "Max-Age=300",
    ] {
        assert!(nonce.contains(part), "{nonce}");
    }
    // Nothing is stored until the ceremony finishes.
    assert_eq!(user_count(&state, &name), 0);

    let cred = key.create(&origin(), creation(&s)).unwrap();
    let r = b.post(&app, REGISTER_FINISH, finish_body(&s, &cred)).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
    assert_eq!(r.body, json!({ "username": name }));
    assert_eq!(r.headers[header::CACHE_CONTROL], "no-store");
    let cookie = r.set_cookie(SESSION).unwrap();
    for part in [
        "HttpOnly",
        "Secure",
        "SameSite=Lax",
        "Path=/",
        "Max-Age=2592000",
    ] {
        assert!(cookie.contains(part), "{cookie}");
    }
    assert!(!cookie.contains("Domain"), "{cookie}");
    // The nonce cookie stays for the browser's other ceremonies.
    assert!(r.set_cookie(CEREMONY).is_none());
    assert!(b.cookies.contains_key(CEREMONY));

    // The user handle offered at start is the stored account id.
    let handle: Base64UrlSafeData = serde_json::from_value(opts["user"]["id"].clone()).unwrap();
    let id = Uuid::from_slice(&Vec::<u8>::from(handle)).unwrap();
    let (row_id, kind): (Uuid, String) = users::table
        .filter(users::username.eq(&name))
        .select((users::id, users::kind))
        .first(&mut conn(&state))
        .unwrap();
    assert_eq!((row_id, kind.as_str()), (id, "player"));
    assert_eq!(passkey_ids(&state, id), vec![Vec::<u8>::from(cred.raw_id)]);
    // Only the digest of the cookie's token is stored.
    assert_eq!(session_hashes(&state, id), vec![b.session_hash()]);

    let me = b.get(&app, ME).await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.body, json!({ "username": name }));
    assert_eq!(me.headers[header::CACHE_CONTROL], "no-store");

    let signed_in = b.clone();
    let out = b.post(&app, LOGOUT, json!({})).await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    let cleared = out.set_cookie(SESSION).unwrap();
    for part in ["Max-Age=0", "Secure", "HttpOnly", "Path=/"] {
        assert!(cleared.contains(part), "{cleared}");
    }
    assert!(!b.cookies.contains_key(SESSION));
    assert!(session_hashes(&state, id).is_empty());
    let me = b.get(&app, ME).await;
    assert_eq!(me.status, StatusCode::UNAUTHORIZED);
    assert_eq!(me.error(), NOT_SIGNED_IN);
    // The old cookie is dead too.
    let me = signed_in.clone().get(&app, ME).await;
    assert_eq!(me.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_roundtrip_rotates_the_session() {
    let Some((state, app)) = db_app() else { return };
    let name = fresh("grace");
    let mut b = Browser::new();
    let mut key = soft();
    assert_eq!(
        register(&app, &mut b, &mut key, &name).await.status,
        StatusCode::OK
    );
    let id = user_id(&state, &name);
    let cred_id = passkey_ids(&state, id).remove(0);
    assert_eq!(last_used(&state, &cred_id), None);
    let before = b.clone();

    let s = start(&app, &mut b, LOGIN_START, &name.to_uppercase()).await;
    assert_eq!(s.status, StatusCode::OK, "{:?}", s.body);
    let opts = &s.body["options"]["publicKey"];
    assert_eq!(opts["userVerification"], "required");
    assert_eq!(opts["rpId"], "packit.test");
    assert_eq!(opts["allowCredentials"][0]["id"], b64(&cred_id));
    let cred = key.get(&origin(), request_options(&s)).unwrap();
    let r = b.post(&app, LOGIN_FINISH, finish_body(&s, &cred)).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
    assert_eq!(r.body, json!({ "username": name }));
    assert_eq!(r.headers[header::CACHE_CONTROL], "no-store");

    // A new token, and the one the browser presented is gone.
    assert_ne!(b.cookies[SESSION], before.cookies[SESSION]);
    assert_eq!(session_hashes(&state, id), vec![b.session_hash()]);
    assert_eq!(
        before.clone().get(&app, ME).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(b.get(&app, ME).await.body, json!({ "username": name }));
    assert!(last_used(&state, &cred_id).is_some());

    // Another browser gets its own session; the first stays signed in.
    let mut other = Browser::new();
    assert_eq!(
        login(&app, &mut other, &mut key, &name).await.status,
        StatusCode::OK
    );
    assert_eq!(session_hashes(&state, id).len(), 2);
    assert_eq!(b.get(&app, ME).await.status, StatusCode::OK);
}

#[tokio::test]
async fn names_are_validated_reserved_and_credited_users_locked_out() {
    let Some((state, app)) = db_app() else { return };
    let credited = fresh("cred");
    diesel::insert_into(users::table)
        .values((
            users::id.eq(Uuid::new_v4()),
            users::username.eq(&credited),
            users::kind.eq("credited"),
        ))
        .execute(&mut conn(&state))
        .unwrap();
    let mut b = Browser::new();
    for name in [
        "admin",
        " Root ",
        "API",
        "packit",
        "SYSTEM",
        &credited,
        &credited.to_uppercase(),
    ] {
        let r = start(&app, &mut b, REGISTER_START, name).await;
        assert_eq!(r.status, StatusCode::CONFLICT, "{name}");
        assert_eq!(r.error(), username::UNAVAILABLE);
    }
    for name in ["x", "bad name", "ad\u{e9}", "_lead", ""] {
        let r = start(&app, &mut b, REGISTER_START, name).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{name}");
        assert_eq!(r.error(), username::INVALID);
    }
    // Unknown, credited and malformed names all get the same answer.
    for name in [credited.as_str(), &fresh("nobody"), "x", "ad\u{e9}"] {
        let r = start(&app, &mut Browser::new(), LOGIN_START, name).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED, "{name}");
        assert_eq!(r.body, json!({ "error": SIGN_IN_FAILED }));
    }

    // A player turned into a credited profile can't use its session or sign in.
    let name = fresh("demo");
    let mut key = soft();
    assert_eq!(
        register(&app, &mut b, &mut key, &name).await.status,
        StatusCode::OK
    );
    diesel::update(users::table.filter(users::username.eq(&name)))
        .set(users::kind.eq("credited"))
        .execute(&mut conn(&state))
        .unwrap();
    assert_eq!(b.get(&app, ME).await.status, StatusCode::UNAUTHORIZED);
    let r = start(&app, &mut Browser::new(), LOGIN_START, &name).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    let r = start(&app, &mut Browser::new(), REGISTER_START, &name).await;
    assert_eq!(r.status, StatusCode::CONFLICT);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn duplicate_usernames_conflict_even_when_concurrent() {
    let Some((state, app)) = db_app() else { return };
    let name = fresh("dup");
    assert_eq!(
        register(&app, &mut Browser::new(), &mut soft(), &name)
            .await
            .status,
        StatusCode::OK
    );
    let r = start(
        &app,
        &mut Browser::new(),
        REGISTER_START,
        &name.to_uppercase(),
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    assert_eq!(r.error(), username::UNAVAILABLE);

    // Both pass the pre-check; the unique index settles it at finish.
    let name = fresh("race");
    let (mut b1, mut b2) = (Browser::new(), Browser::new());
    let s1 = start(&app, &mut b1, REGISTER_START, &name).await;
    let s2 = start(&app, &mut b2, REGISTER_START, &name).await;
    assert_eq!((s1.status, s2.status), (StatusCode::OK, StatusCode::OK));
    let c1 = soft().create(&origin(), creation(&s1)).unwrap();
    let c2 = soft().create(&origin(), creation(&s2)).unwrap();
    let results = race(
        &app,
        vec![
            b1.request(Method::POST, REGISTER_FINISH, Some(&finish_body(&s1, &c1))),
            b2.request(Method::POST, REGISTER_FINISH, Some(&finish_body(&s2, &c2))),
        ],
    )
    .await;
    assert_eq!(results[0].status, StatusCode::OK, "{:?}", results[0].body);
    assert_eq!(results[1].status, StatusCode::CONFLICT);
    assert_eq!(results[1].error(), username::UNAVAILABLE);
    assert!(results[1].set_cookie(SESSION).is_none());
    assert_eq!(user_count(&state, &name), 1);
    assert_eq!(passkey_ids(&state, user_id(&state, &name)).len(), 1);
}

#[tokio::test]
async fn a_registered_credential_cannot_create_another_account() {
    let Some((state, app)) = db_app() else { return };
    let mut key = TestKey::new();
    let first = fresh("owner");
    assert_eq!(
        register(&app, &mut Browser::new(), &mut key, &first)
            .await
            .status,
        StatusCode::OK
    );
    // The same credential ID, freshly attested, for a new name.
    let second = fresh("copy");
    let mut b = Browser::new();
    let r = register(&app, &mut b, &mut key.clone(), &second).await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    assert_eq!(r.error(), PASSKEY_TAKEN);
    assert!(!b.cookies.contains_key(SESSION));
    // The user row rolled back with the passkey, so the name is still free.
    assert_eq!(user_count(&state, &second), 0);
    let r = start(&app, &mut Browser::new(), REGISTER_START, &second).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(
        passkey_ids(&state, user_id(&state, &first)),
        vec![key.cred_id]
    );
}

#[tokio::test]
async fn a_used_ceremony_cannot_be_replayed() {
    let Some((state, app)) = db_app() else { return };
    let name = fresh("replay");
    let mut b = Browser::new();
    let mut key = soft();

    let s = start(&app, &mut b, REGISTER_START, &name).await;
    let body = finish_body(&s, &key.create(&origin(), creation(&s)).unwrap());
    let before = b.clone();
    assert_eq!(
        b.post(&app, REGISTER_FINISH, body.clone()).await.status,
        StatusCode::OK
    );
    // Even with the nonce cookie it was bound to.
    let r = before
        .clone()
        .post(&app, REGISTER_FINISH, body.clone())
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(r.error(), CEREMONY_INVALID);
    assert!(r.set_cookie(SESSION).is_none());
    assert_eq!(
        b.post(&app, REGISTER_FINISH, body).await.status,
        StatusCode::BAD_REQUEST
    );

    let s = start(&app, &mut b, LOGIN_START, &name).await;
    let body = finish_body(&s, &key.get(&origin(), request_options(&s)).unwrap());
    let before = b.clone();
    assert_eq!(
        b.post(&app, LOGIN_FINISH, body.clone()).await.status,
        StatusCode::OK
    );
    let r = before.clone().post(&app, LOGIN_FINISH, body).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(r.error(), CEREMONY_INVALID);
    assert!(r.set_cookie(SESSION).is_none());
    assert_eq!(session_hashes(&state, user_id(&state, &name)).len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn parallel_finishes_of_one_ceremony_succeed_once() {
    let Some((state, app)) = db_app() else { return };
    let name = fresh("para");
    let mut b = Browser::new();
    let mut key = soft();

    let s = start(&app, &mut b, REGISTER_START, &name).await;
    let body = finish_body(&s, &key.create(&origin(), creation(&s)).unwrap());
    let reqs = (0..4)
        .map(|_| b.request(Method::POST, REGISTER_FINISH, Some(&body)))
        .collect();
    let results = race(&app, reqs).await;
    let statuses: Vec<_> = results.iter().map(|r| r.status).collect();
    assert_eq!(
        statuses,
        [
            StatusCode::OK,
            StatusCode::BAD_REQUEST,
            StatusCode::BAD_REQUEST,
            StatusCode::BAD_REQUEST
        ]
    );
    assert_eq!(user_count(&state, &name), 1);
    b.store(&results[0].headers);

    let s = start(&app, &mut b, LOGIN_START, &name).await;
    let body = finish_body(&s, &key.get(&origin(), request_options(&s)).unwrap());
    let reqs = (0..4)
        .map(|_| b.request(Method::POST, LOGIN_FINISH, Some(&body)))
        .collect();
    let results = race(&app, reqs).await;
    let statuses: Vec<_> = results.iter().map(|r| r.status).collect();
    assert_eq!(
        statuses,
        [
            StatusCode::OK,
            StatusCode::BAD_REQUEST,
            StatusCode::BAD_REQUEST,
            StatusCode::BAD_REQUEST
        ]
    );
    // One new session, replacing the one the browser presented.
    assert_eq!(session_hashes(&state, user_id(&state, &name)).len(), 1);
}

#[tokio::test]
async fn expired_ceremonies_are_rejected() {
    let Some((state, app)) = db_app() else { return };
    let name = fresh("late");
    let mut b = Browser::new();
    let mut key = soft();

    let s = start(&app, &mut b, REGISTER_START, &name).await;
    let body = finish_body(&s, &key.create(&origin(), creation(&s)).unwrap());
    state.auth.ceremonies.expire_all();
    let r = b.post(&app, REGISTER_FINISH, body).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(r.error(), CEREMONY_INVALID);
    assert_eq!(user_count(&state, &name), 0);

    assert_eq!(
        register(&app, &mut b, &mut key, &name).await.status,
        StatusCode::OK
    );
    let s = start(&app, &mut b, LOGIN_START, &name).await;
    let body = finish_body(&s, &key.get(&origin(), request_options(&s)).unwrap());
    state.auth.ceremonies.expire_all();
    let r = b.post(&app, LOGIN_FINISH, body).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(r.error(), CEREMONY_INVALID);
}

#[tokio::test]
async fn auth_posts_need_the_exact_origin_and_get_no_cors() {
    let app = build_app(state_for(unconnected_pool()));
    let posts = [
        REGISTER_START,
        REGISTER_FINISH,
        LOGIN_START,
        LOGIN_FINISH,
        ADD_START,
        ADD_FINISH,
        LOGOUT,
    ];
    let bad_origins = [
        None,
        Some("null"),
        Some("https://evil.test"),
        Some("http://packit.test"),
        Some("https://packit.test:8443"),
        Some("https://sub.packit.test"),
        Some("https://packit.test/"),
        Some("HTTPS://PACKIT.TEST"),
    ];
    for path in posts {
        for origin in bad_origins {
            let mut b = Browser::new();
            b.origin = origin;
            let r = b.post(&app, path, json!({ "username": "ada" })).await;
            assert_eq!(r.status, StatusCode::FORBIDDEN, "{path} {origin:?}");
            assert_eq!(r.headers[header::CACHE_CONTROL], "no-store");
            assert!(r.headers.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
        }
    }
    // A second, foreign Origin header doesn't sneak past.
    let b = Browser::new();
    let mut req = b.request(
        Method::POST,
        REGISTER_START,
        Some(&json!({ "username": "ada" })),
    );
    req.headers_mut()
        .append(header::ORIGIN, "https://evil.test".parse().unwrap());
    assert_eq!(call(&app, req).await.status, StatusCode::FORBIDDEN);

    // The right Origin passes; this name is invalid, so it stops before the
    // database.
    let r = start(&app, &mut Browser::new(), REGISTER_START, "x").await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(r.error(), username::INVALID);
    // GET needs no Origin.
    let mut b = Browser::new();
    b.origin = None;
    let r = b.get(&app, ME).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    assert_eq!(r.headers[header::CACHE_CONTROL], "no-store");

    // No CORS preflight approval for auth, while the public API keeps its
    // permissive CORS.
    let preflight = Request::builder()
        .method(Method::OPTIONS)
        .uri(LOGIN_START)
        .header(header::ORIGIN, "https://evil.test")
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
        .body(Body::empty())
        .unwrap();
    let r = call(&app, preflight).await;
    assert!(!r.status.is_success());
    assert!(r.headers.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
    let records = Request::get("/api/records")
        .header(header::ORIGIN, "https://evil.test")
        .body(Body::empty())
        .unwrap();
    let r = call(&app, records).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.headers[header::ACCESS_CONTROL_ALLOW_ORIGIN], "*");
}

#[tokio::test]
async fn wrong_origin_rp_or_challenge_fails() {
    let Some((state, app)) = db_app() else { return };
    let name = fresh("rp");
    let mut b = Browser::new();
    for case in 0..3 {
        let s = start(&app, &mut b, REGISTER_START, &name).await;
        let mut ccr = creation(&s);
        let mut at = origin();
        match case {
            // Signed for another site entirely.
            0 => {
                ccr.public_key.rp.id = "evil.test".into();
                at = evil();
            }
            // Our origin, but scoped to the parent domain.
            1 => ccr.public_key.rp.id = "test".into(),
            _ => ccr.public_key.challenge = random_bytes::<32>().to_vec().into(),
        }
        let cred = soft().create(&at, ccr).unwrap();
        let r = b.post(&app, REGISTER_FINISH, finish_body(&s, &cred)).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "case {case}");
        assert_eq!(r.error(), VERIFY_FAILED);
    }
    assert_eq!(user_count(&state, &name), 0);

    let mut key = soft();
    assert_eq!(
        register(&app, &mut b, &mut key, &name).await.status,
        StatusCode::OK
    );
    for case in 0..3 {
        let mut b = Browser::new();
        let s = start(&app, &mut b, LOGIN_START, &name).await;
        let mut rcr = request_options(&s);
        let mut at = origin();
        match case {
            0 => {
                rcr.public_key.rp_id = "evil.test".into();
                at = evil();
            }
            1 => rcr.public_key.rp_id = "test".into(),
            _ => rcr.public_key.challenge = random_bytes::<32>().to_vec().into(),
        }
        let cred = key.get(&at, rcr).unwrap();
        let r = b.post(&app, LOGIN_FINISH, finish_body(&s, &cred)).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED, "case {case}");
        assert_eq!(r.error(), SIGN_IN_FAILED);
        assert!(!b.cookies.contains_key(SESSION));
    }
}

/// SoftPasskey can't be told to skip user verification when it is required,
/// so these tests play a client that downgrades the request to "preferred".
/// The authenticator then signs without the UV flag, and the server must
/// still refuse.
#[tokio::test]
async fn missing_user_verification_fails() {
    let Some((state, app)) = db_app() else { return };
    let name = fresh("nouv");
    let mut b = Browser::new();
    let s = start(&app, &mut b, REGISTER_START, &name).await;
    let mut ccr = creation(&s);
    ccr.public_key
        .authenticator_selection
        .as_mut()
        .unwrap()
        .user_verification = UserVerificationPolicy::Preferred;
    let cred = soft().create(&origin(), ccr).unwrap();
    let r = b.post(&app, REGISTER_FINISH, finish_body(&s, &cred)).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(r.error(), VERIFY_FAILED);
    assert_eq!(user_count(&state, &name), 0);

    let mut key = soft();
    assert_eq!(
        register(&app, &mut b, &mut key, &name).await.status,
        StatusCode::OK
    );
    let mut b = Browser::new();
    let s = start(&app, &mut b, LOGIN_START, &name).await;
    let mut rcr = request_options(&s);
    rcr.public_key.user_verification = UserVerificationPolicy::Preferred;
    let cred = key.get(&origin(), rcr).unwrap();
    let r = b.post(&app, LOGIN_FINISH, finish_body(&s, &cred)).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    assert!(!b.cookies.contains_key(SESSION));
}

#[tokio::test]
async fn counters_and_backup_state_follow_the_library() {
    let Some((state, app)) = db_app() else { return };

    // Synced passkeys report 0 every time, and 0 after 0 is accepted.
    let mut synced = TestKey {
        backup_eligible: true,
        backed_up: true,
        ..TestKey::new()
    };
    let name = fresh("synced");
    assert_eq!(
        register(&app, &mut Browser::new(), &mut synced, &name)
            .await
            .status,
        StatusCode::OK
    );
    for _ in 0..2 {
        let r = login(&app, &mut Browser::new(), &mut synced, &name).await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
    }
    assert_eq!(stored_credential(&state, &synced.cred_id).counter, 0);

    // A counting authenticator must go strictly up.
    let mut hw = TestKey::new();
    let name = fresh("hw");
    assert_eq!(
        register(&app, &mut Browser::new(), &mut hw, &name)
            .await
            .status,
        StatusCode::OK
    );
    for (counter, status, stored) in [
        (5, StatusCode::OK, 5),
        (3, StatusCode::UNAUTHORIZED, 5),
        (5, StatusCode::UNAUTHORIZED, 5),
        (0, StatusCode::UNAUTHORIZED, 5),
        (6, StatusCode::OK, 6),
    ] {
        hw.counter = counter;
        let r = login(&app, &mut Browser::new(), &mut hw, &name).await;
        assert_eq!(r.status, status, "counter {counter}");
        assert_eq!(stored_credential(&state, &hw.cred_id).counter, stored);
    }

    // Backup state is updated from the authenticator, but eligibility can't
    // be withdrawn.
    let mut backup = TestKey {
        backup_eligible: true,
        ..TestKey::new()
    };
    let name = fresh("backup");
    assert_eq!(
        register(&app, &mut Browser::new(), &mut backup, &name)
            .await
            .status,
        StatusCode::OK
    );
    assert!(!stored_credential(&state, &backup.cred_id).backup_state);
    backup.backed_up = true;
    let r = login(&app, &mut Browser::new(), &mut backup, &name).await;
    assert_eq!(r.status, StatusCode::OK);
    let stored = stored_credential(&state, &backup.cred_id);
    assert!(stored.backup_state && stored.backup_eligible);
    backup.backup_eligible = false;
    backup.backed_up = false;
    let r = login(&app, &mut Browser::new(), &mut backup, &name).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
}

/// Two clones of one key sign with the same counter against ceremonies that
/// both saw the old stored value. Only one may sign in.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cloned_counter_race_signs_in_once() {
    let Some((state, app)) = db_app() else { return };
    let mut key = TestKey::new();
    let name = fresh("clone");
    assert_eq!(
        register(&app, &mut Browser::new(), &mut key, &name)
            .await
            .status,
        StatusCode::OK
    );
    key.counter = 7;
    let (mut b1, mut b2) = (Browser::new(), Browser::new());
    let s1 = start(&app, &mut b1, LOGIN_START, &name).await;
    let s2 = start(&app, &mut b2, LOGIN_START, &name).await;
    let c1 = key.get(&origin(), request_options(&s1)).unwrap();
    let c2 = key.clone().get(&origin(), request_options(&s2)).unwrap();
    let results = race(
        &app,
        vec![
            b1.request(Method::POST, LOGIN_FINISH, Some(&finish_body(&s1, &c1))),
            b2.request(Method::POST, LOGIN_FINISH, Some(&finish_body(&s2, &c2))),
        ],
    )
    .await;
    assert_eq!(results[0].status, StatusCode::OK);
    assert_eq!(results[1].status, StatusCode::UNAUTHORIZED);
    assert_eq!(stored_credential(&state, &key.cred_id).counter, 7);
}

#[tokio::test]
async fn expired_sessions_are_rejected_and_cleaned_up() {
    let Some((state, app)) = db_app() else { return };
    let name = fresh("stale");
    let mut b = Browser::new();
    let mut key = soft();
    assert_eq!(
        register(&app, &mut b, &mut key, &name).await.status,
        StatusCode::OK
    );
    let id = user_id(&state, &name);
    let expire = || {
        diesel::update(sessions::table.filter(sessions::user_id.eq(id)))
            .set(sessions::expires_at.eq(Utc::now() - TimeDelta::minutes(1)))
            .execute(&mut conn(&state))
            .unwrap()
    };
    assert_eq!(expire(), 1);
    let r = b.get(&app, ME).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    // Deleted when seen.
    assert!(session_hashes(&state, id).is_empty());
    assert_eq!(
        b.post(&app, ADD_START, json!({})).await.status,
        StatusCode::UNAUTHORIZED
    );

    // Expired sessions are also swept whenever a session is created.
    let mut other = Browser::new();
    assert_eq!(
        login(&app, &mut other, &mut key, &name).await.status,
        StatusCode::OK
    );
    assert_eq!(expire(), 1);
    let mut third = Browser::new();
    assert_eq!(
        login(&app, &mut third, &mut key, &name).await.status,
        StatusCode::OK
    );
    assert_eq!(session_hashes(&state, id), vec![third.session_hash()]);
}

#[tokio::test]
async fn adding_a_passkey_needs_recent_sign_in_and_the_same_account() {
    let Some((state, app)) = db_app() else { return };
    let name = fresh("add");
    let mut b = Browser::new();
    let mut first = soft();
    assert_eq!(
        register(&app, &mut b, &mut first, &name).await.status,
        StatusCode::OK
    );
    let id = user_id(&state, &name);
    let first_id = passkey_ids(&state, id).remove(0);

    let r = Browser::new().post(&app, ADD_START, json!({})).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);

    let s = b.post(&app, ADD_START, json!({})).await;
    assert_eq!(s.status, StatusCode::OK, "{:?}", s.body);
    let opts = &s.body["options"]["publicKey"];
    assert_eq!(opts["user"]["name"], name.as_str());
    assert_eq!(opts["excludeCredentials"][0]["id"], b64(&first_id));
    assert_eq!(
        opts["authenticatorSelection"]["userVerification"],
        "required"
    );
    assert_eq!(opts["authenticatorSelection"]["residentKey"], "preferred");
    let mut second = soft();
    let cred = second.create(&origin(), creation(&s)).unwrap();
    let r = b.post(&app, ADD_FINISH, finish_body(&s, &cred)).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
    assert_eq!(r.body, json!({ "username": name }));
    assert_eq!(passkey_ids(&state, id).len(), 2);
    assert_eq!(
        login(&app, &mut Browser::new(), &mut second, &name)
            .await
            .status,
        StatusCode::OK
    );

    // A sign-in older than five minutes isn't enough.
    diesel::update(sessions::table.filter(sessions::token_hash.eq(b.session_hash())))
        .set(sessions::created_at.eq(Utc::now() - TimeDelta::minutes(6)))
        .execute(&mut conn(&state))
        .unwrap();
    let r = b.post(&app, ADD_START, json!({})).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    assert_eq!(r.error(), REAUTH);
    assert_eq!(
        login(&app, &mut b, &mut first, &name).await.status,
        StatusCode::OK
    );
    assert_eq!(
        b.post(&app, ADD_START, json!({})).await.status,
        StatusCode::OK
    );

    // Another account can't finish this account's ceremony: not from its own
    // browser, which lacks the nonce...
    let other_name = fresh("other");
    let mut other = Browser::new();
    assert_eq!(
        register(&app, &mut other, &mut soft(), &other_name)
            .await
            .status,
        StatusCode::OK
    );
    let other_id = user_id(&state, &other_name);
    let s = b.post(&app, ADD_START, json!({})).await;
    let body = finish_body(&s, &soft().create(&origin(), creation(&s)).unwrap());
    let r = other.post(&app, ADD_FINISH, body.clone()).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(r.error(), CEREMONY_INVALID);
    // ...which used the ceremony up...
    assert_eq!(
        b.post(&app, ADD_FINISH, body).await.status,
        StatusCode::BAD_REQUEST
    );
    // ...and not with a stolen nonce, since the session belongs to someone else.
    let s = b.post(&app, ADD_START, json!({})).await;
    let body = finish_body(&s, &soft().create(&origin(), creation(&s)).unwrap());
    let mut thief = other.clone();
    thief
        .cookies
        .insert(CEREMONY.into(), b.cookies[CEREMONY].clone());
    let r = thief.post(&app, ADD_FINISH, body).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    assert_eq!(r.error(), REAUTH);
    assert_eq!(passkey_ids(&state, id).len(), 2);
    assert_eq!(passkey_ids(&state, other_id).len(), 1);

    // A credential that already belongs to anyone is refused: another
    // account's...
    let mut shared_key = TestKey::new();
    let owner = fresh("keyown");
    let mut owner_browser = Browser::new();
    assert_eq!(
        register(&app, &mut owner_browser, &mut shared_key, &owner)
            .await
            .status,
        StatusCode::OK
    );
    let r = add_passkey(&app, &mut b, &mut shared_key.clone()).await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    assert_eq!(r.error(), PASSKEY_TAKEN);
    // ...or its own. A client that ignores excludeCredentials is caught by
    // the library at finish, which checks the same list.
    let s = owner_browser.post(&app, ADD_START, json!({})).await;
    assert_eq!(
        s.body["options"]["publicKey"]["excludeCredentials"][0]["id"],
        b64(&shared_key.cred_id)
    );
    let cred = shared_key.clone().create(&origin(), creation(&s)).unwrap();
    let r = owner_browser
        .post(&app, ADD_FINISH, finish_body(&s, &cred))
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(r.error(), VERIFY_FAILED);
    assert_eq!(passkey_ids(&state, id).len(), 2);
    assert_eq!(passkey_ids(&state, user_id(&state, &owner)).len(), 1);
}

#[tokio::test]
async fn start_and_finish_are_rate_limited_per_client() {
    let app = build_app(state_for(unconnected_pool()));
    let mut b = Browser::new();
    for _ in 0..IP_BURST {
        let r = start(&app, &mut b, REGISTER_START, "x").await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
    }
    let r = start(&app, &mut b, REGISTER_START, "x").await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(r.headers[header::CACHE_CONTROL], "no-store");
    let retry: u64 = r.headers[header::RETRY_AFTER]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..=2).contains(&retry), "{retry}");
    // Finish endpoints draw from the same bucket.
    let r = b
        .post(
            &app,
            LOGIN_FINISH,
            json!({ "ceremony": "00", "credential": {} }),
        )
        .await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    // Another client is unaffected, and me and logout aren't limited.
    let r = start(&app, &mut Browser::new(), REGISTER_START, "x").await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(b.get(&app, ME).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        b.post(&app, LOGOUT, json!({})).await.status,
        StatusCode::NO_CONTENT
    );
}

#[tokio::test]
async fn forwarded_for_counts_only_from_the_trusted_proxy() {
    let proxy: IpAddr = "10.200.0.1".parse().unwrap();
    let origin = PublicOrigin::parse(TEST_URL, false).unwrap();
    let trust = ProxyTrust {
        ip: Some(proxy),
        token: None,
    };
    let state = AppState::new(true, unconnected_pool(), origin, trust).unwrap();
    let app = build_app(Arc::new(state));
    let forwarded = |b: &Browser, chain: &str| {
        let mut req = b.request(
            Method::POST,
            REGISTER_START,
            Some(&json!({ "username": "x" })),
        );
        req.headers_mut()
            .insert("x-forwarded-for", chain.parse().unwrap());
        req
    };

    let mut via_proxy = Browser::new();
    via_proxy.ip = proxy;
    for _ in 0..IP_BURST {
        let req = forwarded(&via_proxy, "198.51.100.7, 203.0.113.1");
        assert_eq!(call(&app, req).await.status, StatusCode::BAD_REQUEST);
    }
    // Keyed by the last hop the proxy appended, not the client-supplied rest.
    let req = forwarded(&via_proxy, "192.0.2.99, 203.0.113.1");
    assert_eq!(call(&app, req).await.status, StatusCode::TOO_MANY_REQUESTS);
    let req = forwarded(&via_proxy, "203.0.113.2");
    assert_eq!(call(&app, req).await.status, StatusCode::BAD_REQUEST);

    // Anyone else's X-Forwarded-For is ignored, so rotating it doesn't help.
    let direct = Browser::new();
    for i in 0..IP_BURST {
        let req = forwarded(&direct, &format!("203.0.113.{i}"));
        assert_eq!(call(&app, req).await.status, StatusCode::BAD_REQUEST);
    }
    let req = forwarded(&direct, "203.0.113.200");
    assert_eq!(call(&app, req).await.status, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn with_a_proxy_token_forwarded_for_needs_it() {
    let token = "f".repeat(64);
    let origin = PublicOrigin::parse(TEST_URL, false).unwrap();
    let trust = ProxyTrust {
        ip: None,
        token: Some(ProxyToken::parse(&token).unwrap()),
    };
    let app = build_app(Arc::new(
        AppState::new(true, unconnected_pool(), origin, trust).unwrap(),
    ));
    let peer = Browser::new();
    let send = |sent: Option<&str>, chain: &str| {
        let mut req = peer.request(
            Method::POST,
            REGISTER_START,
            Some(&json!({ "username": "x" })),
        );
        if let Some(t) = sent {
            req.headers_mut().insert(TOKEN_HEADER, t.parse().unwrap());
        }
        req.headers_mut()
            .insert("x-forwarded-for", chain.parse().unwrap());
        req
    };

    // With the token, each forwarded client has its own bucket, keyed on the
    // rightmost entry.
    for _ in 0..IP_BURST {
        let r = call(&app, send(Some(&token), "1.2.3.4, 203.0.113.1")).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
    }
    let r = call(&app, send(Some(&token), "203.0.113.1")).await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    let r = call(&app, send(Some(&token), "203.0.113.2")).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);

    // Forged or missing tokens are keyed on the socket peer, whatever
    // X-Forwarded-For says...
    let forged = "e".repeat(64);
    for i in 0..IP_BURST {
        let sent = (i % 2 == 0).then_some(forged.as_str());
        let r = call(&app, send(sent, &format!("203.0.113.{}", 10 + i))).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
    }
    let r = call(&app, send(None, "203.0.113.99")).await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    // ...and so is a trusted request whose last entry is garbage.
    let r = call(&app, send(Some(&token), "203.0.113.3, junk")).await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
}

fn ipv6_browser(site: u16, subnet: u16) -> Browser {
    let mut b = Browser::new();
    b.ip = Ipv6Addr::new(0x2001, 0xdb8, site, subnet, 0, 0, 0, 1).into();
    b
}

/// The login-start username limit is per client, so exhausting it locks out
/// only the client doing it.
#[tokio::test]
async fn login_start_is_rate_limited_per_username_and_client() {
    let Some((_state, app)) = db_app() else {
        return;
    };
    let name = fresh("limit");
    let mut attacker = Browser::new();
    for _ in 0..USERNAME_BURST {
        let r = start(&app, &mut attacker, LOGIN_START, &name).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    }
    for variant in [name.clone(), format!(" {} ", name.to_uppercase())] {
        let r = start(&app, &mut attacker, LOGIN_START, &variant).await;
        assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS, "{variant}");
        assert_eq!(r.error(), TOO_MANY);
        assert!(r.headers.contains_key(header::RETRY_AFTER));
    }
    // Another name from that client is fine, and so is this name from
    // anyone else.
    let r = start(&app, &mut attacker, LOGIN_START, &fresh("limit")).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    let r = start(&app, &mut Browser::new(), LOGIN_START, &name).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);

    // A real account stays usable while another client hammers it.
    let victim = fresh("victim");
    let mut key = soft();
    assert_eq!(
        register(&app, &mut Browser::new(), &mut key, &victim)
            .await
            .status,
        StatusCode::OK
    );
    let mut attacker = Browser::new();
    let mut refused = false;
    for _ in 0..=USERNAME_BURST {
        let r = start(&app, &mut attacker, LOGIN_START, &victim).await;
        refused |= r.status == StatusCode::TOO_MANY_REQUESTS;
    }
    assert!(refused);
    let r = login(&app, &mut Browser::new(), &mut key, &victim).await;
    assert_eq!(r.status, StatusCode::OK);
}

#[tokio::test]
async fn each_client_gets_a_few_ceremonies_in_progress() {
    let Some((_state, app)) = db_app() else {
        return;
    };
    let name = fresh("busy");
    let mut key = soft();
    assert_eq!(
        register(&app, &mut Browser::new(), &mut key, &name)
            .await
            .status,
        StatusCode::OK
    );
    let mut greedy = Browser::new();
    for _ in 0..CEREMONIES_PER_CLIENT {
        let r = start(&app, &mut greedy, LOGIN_START, &name).await;
        assert_eq!(r.status, StatusCode::OK);
    }
    let r = start(&app, &mut greedy, LOGIN_START, &name).await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(r.error(), TOO_MANY);
    let retry: u64 = r.headers[header::RETRY_AFTER]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..=300).contains(&retry), "{retry}");
    // Registration draws on the same allowance.
    let r = start(&app, &mut greedy, REGISTER_START, &fresh("more")).await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    // Everyone else still signs in.
    let r = login(&app, &mut Browser::new(), &mut key, &name).await;
    assert_eq!(r.status, StatusCode::OK);
}

#[tokio::test]
async fn an_ipv6_site_shares_a_ceremony_cap() {
    let Some((_state, app)) = db_app() else {
        return;
    };
    let name = fresh("site");
    let mut key = soft();
    assert_eq!(
        register(&app, &mut Browser::new(), &mut key, &name)
            .await
            .status,
        StatusCode::OK
    );
    // Fill the /48 from several /64s, each within its own cap.
    for subnet in 0..(CEREMONIES_PER_SITE / CEREMONIES_PER_CLIENT) as u16 {
        let mut b = ipv6_browser(1, subnet);
        for _ in 0..CEREMONIES_PER_CLIENT {
            let r = start(&app, &mut b, LOGIN_START, &name).await;
            assert_eq!(r.status, StatusCode::OK, "subnet {subnet}");
        }
    }
    let r = start(&app, &mut ipv6_browser(1, 999), LOGIN_START, &name).await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    let r = start(&app, &mut ipv6_browser(2, 0), LOGIN_START, &name).await;
    assert_eq!(r.status, StatusCode::OK);
    let r = login(&app, &mut Browser::new(), &mut key, &name).await;
    assert_eq!(r.status, StatusCode::OK);
}

#[tokio::test]
async fn an_ipv6_site_shares_a_rate_limit_across_its_64s() {
    let app = build_app(state_for(unconnected_pool()));
    // One request from each fresh /64, so only the /48 bucket can run out.
    let mut sent: u16 = 0;
    let limited = loop {
        let r = start(&app, &mut ipv6_browser(1, sent), REGISTER_START, "x").await;
        sent += 1;
        if r.status == StatusCode::TOO_MANY_REQUESTS {
            break r;
        }
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
        assert!(u32::from(sent) <= SITE_BURST + 50, "never limited");
    };
    assert!(u32::from(sent) > SITE_BURST, "{sent}");
    assert!(limited.headers.contains_key(header::RETRY_AFTER));
    let r = start(&app, &mut ipv6_browser(2, 0), REGISTER_START, "x").await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    // A /48 has no say over IPv4 clients.
    let r = start(&app, &mut Browser::new(), REGISTER_START, "x").await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn untrusted_forwarded_for_is_reported_once() {
    let state = state_for(unconnected_pool());
    let app = build_app(state.clone());
    let b = Browser::new();
    let send = |forwarded: Option<&str>| {
        let mut req = b.request(
            Method::POST,
            REGISTER_START,
            Some(&json!({ "username": "x" })),
        );
        if let Some(v) = forwarded {
            req.headers_mut()
                .insert("x-forwarded-for", v.parse().unwrap());
        }
        req
    };
    call(&app, send(None)).await;
    assert!(!state.auth.warned_forwarded.load(Ordering::Relaxed));
    for _ in 0..3 {
        let r = call(&app, send(Some("203.0.113.7"))).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
        assert!(state.auth.warned_forwarded.load(Ordering::Relaxed));
    }

    // With a trusted proxy configured the header is expected: no warning.
    let origin = PublicOrigin::parse(TEST_URL, false).unwrap();
    let trust = ProxyTrust {
        ip: Some(b.ip),
        token: None,
    };
    let proxied = Arc::new(AppState::new(true, unconnected_pool(), origin, trust).unwrap());
    let app = build_app(proxied.clone());
    call(&app, send(Some("203.0.113.7"))).await;
    assert!(!proxied.auth.warned_forwarded.load(Ordering::Relaxed));
}

#[tokio::test]
async fn adding_a_passkey_rechecks_recent_sign_in_at_finish() {
    let Some((state, app)) = db_app() else {
        return;
    };
    let name = fresh("slow");
    let mut b = Browser::new();
    assert_eq!(
        register(&app, &mut b, &mut soft(), &name).await.status,
        StatusCode::OK
    );
    let s = b.post(&app, ADD_START, json!({})).await;
    assert_eq!(s.status, StatusCode::OK);
    let body = finish_body(&s, &soft().create(&origin(), creation(&s)).unwrap());
    // The sign-in ages past the window between start and finish.
    diesel::update(sessions::table.filter(sessions::token_hash.eq(b.session_hash())))
        .set(sessions::created_at.eq(Utc::now() - TimeDelta::minutes(6)))
        .execute(&mut conn(&state))
        .unwrap();
    let r = b.post(&app, ADD_FINISH, body).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    assert_eq!(r.error(), REAUTH);
    assert_eq!(passkey_ids(&state, user_id(&state, &name)).len(), 1);
}

#[tokio::test]
async fn the_nonce_cookie_survives_other_tabs_and_bad_finishes() {
    let Some((state, app)) = db_app() else {
        return;
    };
    let (first, second) = (fresh("tab"), fresh("tab"));
    let mut b = Browser::new();
    let s1 = start(&app, &mut b, REGISTER_START, &first).await;
    let nonce = b.cookies[CEREMONY].clone();
    // A second tab reuses the nonce, refreshing its lifetime.
    let s2 = start(&app, &mut b, REGISTER_START, &second).await;
    assert!(s2.set_cookie(CEREMONY).unwrap().contains(&nonce));
    assert_eq!(b.cookies[CEREMONY], nonce);
    let c1 = soft().create(&origin(), creation(&s1)).unwrap();
    let c2 = soft().create(&origin(), creation(&s2)).unwrap();

    // Finishes for unknown or malformed ceremonies leave the cookie alone.
    for ceremony in ["00".repeat(16), "zz".into()] {
        let body = json!({ "ceremony": ceremony, "credential": &c1 });
        let r = b.post(&app, REGISTER_FINISH, body).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
        assert!(r.set_cookie(CEREMONY).is_none());
        assert_eq!(b.cookies[CEREMONY], nonce);
    }

    // Both tabs finish, in either order.
    let r = b.post(&app, REGISTER_FINISH, finish_body(&s2, &c2)).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
    let r = b.post(&app, REGISTER_FINISH, finish_body(&s1, &c1)).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
    assert_eq!(user_count(&state, &first), 1);
    assert_eq!(user_count(&state, &second), 1);
}

#[tokio::test]
async fn a_ceremony_only_finishes_the_operation_it_started() {
    let Some((state, app)) = db_app() else {
        return;
    };
    let name = fresh("op");
    let mut b = Browser::new();
    let mut key = soft();
    assert_eq!(
        register(&app, &mut b, &mut key, &name).await.status,
        StatusCode::OK
    );
    let login_started = start(&app, &mut b, LOGIN_START, &name).await;
    let assertion = key.get(&origin(), request_options(&login_started)).unwrap();
    let other = fresh("op");
    let reg_started = start(&app, &mut b, REGISTER_START, &other).await;
    let attestation = soft().create(&origin(), creation(&reg_started)).unwrap();

    // Each id sent to the wrong finish fails, and is used up by it.
    let r = b
        .post(&app, LOGIN_FINISH, finish_body(&reg_started, &assertion))
        .await;
    assert_eq!(r.error(), CEREMONY_INVALID);
    let r = b
        .post(&app, ADD_FINISH, finish_body(&login_started, &attestation))
        .await;
    assert_eq!(r.error(), CEREMONY_INVALID);
    let r = b
        .post(
            &app,
            REGISTER_FINISH,
            finish_body(&reg_started, &attestation),
        )
        .await;
    assert_eq!(r.error(), CEREMONY_INVALID);
    let r = b
        .post(&app, LOGIN_FINISH, finish_body(&login_started, &assertion))
        .await;
    assert_eq!(r.error(), CEREMONY_INVALID);
    assert_eq!(user_count(&state, &other), 0);
}

#[tokio::test]
async fn logout_clears_the_cookie_even_if_the_database_fails() {
    let pool = Pool::builder()
        .connection_timeout(Duration::from_millis(200))
        .build_unchecked(ConnectionManager::<PgConnection>::new(
            "postgres://invalid/db",
        ));
    let app = build_app(state_for(pool));
    let mut b = Browser::new();
    b.cookies.insert(SESSION.into(), "ab".repeat(32));
    let r = b.post(&app, LOGOUT, json!({})).await;
    assert_eq!(r.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(r.set_cookie(SESSION).unwrap().contains("Max-Age=0"));
    assert!(!b.cookies.contains_key(SESSION));
}

#[tokio::test]
async fn ownership_columns_are_indexed() {
    let Some((state, _app)) = db_app() else {
        return;
    };
    let found: i64 = diesel::select(diesel::dsl::sql::<diesel::sql_types::BigInt>(
        "(SELECT count(*) FROM pg_indexes WHERE indexname IN \
         ('scores_user_id_idx', 'solution_shares_created_by_idx'))",
    ))
    .get_result(&mut conn(&state))
    .unwrap();
    assert_eq!(found, 2);
}

/// Two squares in a box no other run uses, so a share of it never dedupes
/// onto another test's row.
fn unique_squares() -> shared::Arrangement {
    let side = 2.0 + (Uuid::new_v4().as_u128() % 1_000_000) as f64 * 1e-7;
    let sq = |cx| shared::Placement {
        cx,
        cy: 0.5,
        theta: 0.0,
    };
    shared::Arrangement {
        n: 2,
        side,
        squares: vec![sq(0.5), sq(1.5)],
    }
}

fn share_body(arrangement: &shared::Arrangement) -> Value {
    json!({ "n": 2, "code": shared::share::encode(arrangement, &[]) })
}

fn share_owner(state: &AppState, url: &Value) -> Option<Uuid> {
    let token = url.as_str().unwrap().rsplit('/').next().unwrap().to_owned();
    solution_shares::table
        .find(token)
        .select(solution_shares::created_by)
        .first(&mut conn(state))
        .unwrap()
}

/// Submitting and sharing need the site's exact Origin, which is checked
/// first, and then a session. Neither answers other origins with CORS.
#[tokio::test]
async fn signed_in_writes_need_the_exact_origin_and_a_session() {
    let Some((state, app)) = db_app() else {
        return;
    };
    let name = fresh("writer");
    let mut b = Browser::new();
    assert_eq!(
        register(&app, &mut b, &mut soft(), &name).await.status,
        StatusCode::OK
    );
    let arrangement = unique_squares();
    let score = json!({ "arrangement": arrangement });
    let share = share_body(&arrangement);
    for (uri, body, why) in [
        ("/api/scores", &score, SIGN_IN_TO_SUBMIT),
        ("/api/shares", &share, SIGN_IN_TO_SHARE),
    ] {
        let r = Browser::new().post(&app, uri, body.clone()).await;
        assert_eq!((r.status, r.error()), (StatusCode::UNAUTHORIZED, why));
        for origin in [None, Some("https://evil.test"), Some("http://packit.test")] {
            let mut elsewhere = b.clone();
            elsewhere.origin = origin;
            let r = elsewhere.post(&app, uri, body.clone()).await;
            assert_eq!(r.status, StatusCode::FORBIDDEN, "{uri} from {origin:?}");
            assert!(r.headers.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
        }
        let mut twice = b.request(Method::POST, uri, Some(body));
        twice
            .headers_mut()
            .append(header::ORIGIN, TEST_URL.parse().unwrap());
        assert_eq!(call(&app, twice).await.status, StatusCode::FORBIDDEN);
    }

    let owner = user_id(&state, &name);
    let r = b.post(&app, "/api/scores", score).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
    assert!(r.headers.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
    let id: Uuid = serde_json::from_value(r.body["id"].clone()).unwrap();
    let credited: Option<Uuid> = scores::table
        .find(id)
        .select(scores::user_id)
        .first(&mut conn(&state))
        .unwrap();
    assert_eq!(credited, Some(owner));
    let r = b.post(&app, "/api/shares", share).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
    assert!(r.headers.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
    assert_eq!(share_owner(&state, &r.body["url"]), Some(owner));

    // Reads keep their permissive CORS.
    let mut reader = Browser::new();
    reader.origin = Some("https://evil.test");
    let r = reader.get(&app, "/api/scores?n=2").await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.headers[header::ACCESS_CONTROL_ALLOW_ORIGIN], "*");
}

/// The leaderboard name is the account's username; a name in the body is
/// ignored.
#[tokio::test]
async fn scores_are_named_by_the_account() {
    let Some((state, app)) = db_app() else {
        return;
    };
    let name = fresh("ada");
    let mut b = Browser::new();
    assert_eq!(
        register(&app, &mut b, &mut soft(), &name).await.status,
        StatusCode::OK
    );
    let body = json!({ "player": "mallory", "arrangement": unique_squares() });
    let r = b.post(&app, "/api/scores", body).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
    assert_eq!(r.body["player"], name.as_str());
    let id = r.body["id"].as_str().unwrap().to_owned();
    let detail = Browser::new().get(&app, &format!("/api/scores/{id}")).await;
    assert_eq!(detail.body["entry"]["player"], name.as_str());
    let stored: String = scores::table
        .find(Uuid::parse_str(&id).unwrap())
        .select(scores::player)
        .first(&mut conn(&state))
        .unwrap();
    assert_eq!(stored, name);
}

/// Anonymous scores from before accounts keep their names and stay
/// unowned, even when an account later takes the same name.
#[tokio::test]
async fn legacy_scores_stay_unclaimed() {
    let Some((state, app)) = db_app() else {
        return;
    };
    let name = fresh("legacy");
    let legacy = Uuid::new_v4();
    diesel::insert_into(scores::table)
        .values((
            scores::id.eq(legacy),
            scores::player.eq(&name),
            scores::n.eq(2),
            scores::side.eq(2.5),
            scores::arrangement.eq(serde_json::to_value(unique_squares()).unwrap()),
        ))
        .execute(&mut conn(&state))
        .unwrap();
    let mut b = Browser::new();
    assert_eq!(
        register(&app, &mut b, &mut soft(), &name).await.status,
        StatusCode::OK
    );
    let r = b
        .post(
            &app,
            "/api/scores",
            json!({ "arrangement": unique_squares() }),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
    let owners: Vec<(Uuid, Option<Uuid>)> = scores::table
        .filter(scores::player.eq(&name))
        .select((scores::id, scores::user_id))
        .load(&mut conn(&state))
        .unwrap();
    assert_eq!(owners.len(), 2);
    for (id, owner) in owners {
        let expected = (id != legacy).then(|| user_id(&state, &name));
        assert_eq!(owner, expected, "{id}");
    }
    let detail = Browser::new()
        .get(&app, &format!("/api/scores/{legacy}"))
        .await;
    assert_eq!(detail.body["entry"]["player"], name.as_str());
}

/// Sharing a snapshot that already has a link returns that link and leaves
/// its creator alone, whether it has none (a legacy row) or another
/// account. Every account gets the same response.
#[tokio::test]
async fn a_reshared_snapshot_keeps_its_original_creator() {
    let Some((state, app)) = db_app() else {
        return;
    };
    let (mut first, mut second) = (Browser::new(), Browser::new());
    let first_name = fresh("first");
    for (b, name) in [(&mut first, &first_name), (&mut second, &fresh("second"))] {
        assert_eq!(
            register(&app, b, &mut soft(), name).await.status,
            StatusCode::OK
        );
    }

    let legacy = unique_squares();
    let code = shared::share::encode(&legacy, &[]);
    let token = Uuid::new_v4().simple().to_string()[..24].to_owned();
    diesel::insert_into(solution_shares::table)
        .values((
            solution_shares::token.eq(&token),
            solution_shares::payload_hash.eq(Sha256::digest(code.as_bytes()).to_vec()),
            solution_shares::n.eq(2),
            solution_shares::code.eq(&code),
        ))
        .execute(&mut conn(&state))
        .unwrap();
    let r = second.post(&app, "/api/shares", share_body(&legacy)).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
    assert_eq!(r.body["url"], format!("{TEST_URL}/s/{token}"));
    assert_eq!(share_owner(&state, &r.body["url"]), None);

    let snapshot = share_body(&unique_squares());
    let original = first.post(&app, "/api/shares", snapshot.clone()).await;
    assert_eq!(original.status, StatusCode::OK);
    let again = second.post(&app, "/api/shares", snapshot).await;
    assert_eq!(
        (again.status, &again.body),
        (original.status, &original.body)
    );
    assert_eq!(
        share_owner(&state, &again.body["url"]),
        Some(user_id(&state, &first_name))
    );
}

/// A credited profile can't sign in, and a session row forged for one is
/// refused and removed.
#[tokio::test]
async fn credited_profiles_cannot_submit_or_share() {
    let Some((state, app)) = db_app() else {
        return;
    };
    let friedman = user_id(&state, "friedman");
    let arrangement = unique_squares();
    for (uri, body, why) in [
        (
            "/api/scores",
            json!({ "arrangement": arrangement }),
            SIGN_IN_TO_SUBMIT,
        ),
        ("/api/shares", share_body(&arrangement), SIGN_IN_TO_SHARE),
    ] {
        let token = session::create(&mut conn(&state), friedman, None).unwrap();
        let mut b = Browser::new();
        b.cookies.insert(SESSION.into(), hex::encode(&token));
        let r = b.post(&app, uri, body).await;
        assert_eq!((r.status, r.error()), (StatusCode::UNAUTHORIZED, why));
        assert!(!session_hashes(&state, friedman).contains(&session::hash_token(&token)));
    }
}

/// Reads and existing links stay public: they work with no Origin or
/// another one, and with no, malformed or unknown session cookies, and never
/// set a cookie. Writes from the same browsers are refused.
#[tokio::test]
async fn reads_and_existing_links_stay_anonymous() {
    let Some((_state, app)) = db_app() else {
        return;
    };
    let mut owner = Browser::new();
    assert_eq!(
        register(&app, &mut owner, &mut soft(), &fresh("linker"))
            .await
            .status,
        StatusCode::OK
    );
    let squares = unique_squares();
    let code = shared::share::encode(&squares, &[]);
    let r = owner.post(&app, "/api/shares", share_body(&squares)).await;
    assert_eq!(r.status, StatusCode::OK);
    let link = r.body["url"]
        .as_str()
        .unwrap()
        .strip_prefix(TEST_URL)
        .unwrap()
        .to_owned();
    let unknown = format!("{SESSION}={}", "00".repeat(32));
    for cookie in [None, Some(format!("{SESSION}=zz")), Some(unknown)] {
        for origin in [None, Some("https://evil.test"), Some(TEST_URL)] {
            let req = |method: Method, uri: &str, body: Option<Value>| {
                let mut req = Request::builder().method(method).uri(uri);
                if let Some(c) = &cookie {
                    req = req.header(header::COOKIE, c);
                }
                if let Some(o) = origin {
                    req = req.header(header::ORIGIN, o);
                }
                match body {
                    Some(b) => req
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(Body::from(serde_json::to_vec(&b).unwrap())),
                    None => req.body(Body::empty()),
                }
                .unwrap()
            };
            for uri in ["/api/records", "/api/scores", "/api/scores?n=2"] {
                let r = call(&app, req(Method::GET, uri, None)).await;
                assert_eq!(r.status, StatusCode::OK, "{uri}");
            }
            let r = call(&app, req(Method::GET, &link, None)).await;
            assert_eq!(r.status, StatusCode::FOUND);
            assert_eq!(
                r.headers[header::LOCATION],
                format!("{TEST_URL}/play/2?s={code}")
            );
            let r = call(&app, req(Method::GET, &format!("/play/2?s={code}"), None)).await;
            assert_eq!(r.status, StatusCode::OK);
            assert!(r.body.as_str().unwrap().contains("og:image"));
            let preview = format!("/api/preview.png?n=2&s={code}");
            let r = call(&app, req(Method::GET, &preview, None)).await;
            assert_eq!(r.status, StatusCode::OK);
            assert_eq!(r.headers[header::CONTENT_TYPE], "image/png");
            assert!(r.headers.get(header::SET_COOKIE).is_none());

            let refused = if origin == Some(TEST_URL) {
                StatusCode::UNAUTHORIZED
            } else {
                StatusCode::FORBIDDEN
            };
            let score = json!({ "arrangement": squares });
            let r = call(&app, req(Method::POST, "/api/scores", Some(score))).await;
            assert_eq!(r.status, refused, "{:?}", r.body);
            let r = call(
                &app,
                req(Method::POST, "/api/shares", Some(share_body(&squares))),
            )
            .await;
            assert_eq!(r.status, refused, "{:?}", r.body);
            assert!(r.headers.get(header::SET_COOKIE).is_none());
        }
    }
}

#[test]
fn a_site_over_its_limit_allocates_no_new_subnets() {
    let origin = PublicOrigin::parse(TEST_URL, false).unwrap();
    let auth = super::Auth::new(origin, ProxyTrust::default()).unwrap();
    let now = std::time::Instant::now();
    let client = |subnet: u16| {
        super::ratelimit::RateKey::of(Ipv6Addr::new(0x2001, 0xdb8, 7, subnet, 0, 0, 0, 1).into())
    };
    for subnet in 0..SITE_BURST as u16 {
        assert!(auth.check_client(client(subnet), now).is_ok());
    }
    assert_eq!(auth.ip_limiter.len(), SITE_BURST as usize);
    for subnet in SITE_BURST as u16..SITE_BURST as u16 + 500 {
        assert!(auth.check_client(client(subnet), now).is_err());
    }
    assert_eq!(auth.ip_limiter.len(), SITE_BURST as usize);
}

#[tokio::test]
async fn seeded_credited_names_are_taken_and_cannot_sign_in() {
    let Some((state, app)) = db_app() else {
        return;
    };
    // Seeded by the migration, with no way to sign in.
    let friedman = user_id(&state, "friedman");
    assert!(passkey_ids(&state, friedman).is_empty());
    let (kind, display): (String, Option<String>) = users::table
        .filter(users::id.eq(friedman))
        .select((users::kind, users::display_name))
        .first(&mut conn(&state))
        .unwrap();
    assert_eq!(
        (kind.as_str(), display.as_deref()),
        ("credited", Some("Erich Friedman"))
    );

    let taken = fresh("taken");
    assert_eq!(
        register(&app, &mut Browser::new(), &mut soft(), &taken)
            .await
            .status,
        StatusCode::OK
    );
    let as_taken = start(&app, &mut Browser::new(), REGISTER_START, &taken).await;
    assert_eq!(as_taken.status, StatusCode::CONFLICT);
    let as_unknown = start(&app, &mut Browser::new(), LOGIN_START, &fresh("nobody")).await;
    assert_eq!(as_unknown.status, StatusCode::UNAUTHORIZED);
    for name in [
        "friedman",
        " Goebel ",
        "HAEMAELAEINEN",
        "winter",
        "themagicanimals",
    ] {
        let r = start(&app, &mut Browser::new(), REGISTER_START, name).await;
        assert_eq!(
            (r.status, &r.body),
            (as_taken.status, &as_taken.body),
            "{name}"
        );
        let r = start(&app, &mut Browser::new(), LOGIN_START, name).await;
        assert_eq!(
            (r.status, &r.body),
            (as_unknown.status, &as_unknown.body),
            "{name}"
        );
    }
}

/// A credited profile added between register start and finish wins; the
/// registration leaves nothing behind.
#[tokio::test]
async fn a_name_credited_mid_registration_fails_cleanly() {
    let Some((state, app)) = db_app() else {
        return;
    };
    let name = fresh("late");
    let mut b = Browser::new();
    let s = start(&app, &mut b, REGISTER_START, &name).await;
    assert_eq!(s.status, StatusCode::OK);
    let handle: Base64UrlSafeData =
        serde_json::from_value(s.body["options"]["publicKey"]["user"]["id"].clone()).unwrap();
    let would_be = Uuid::from_slice(&Vec::<u8>::from(handle)).unwrap();
    let cred = soft().create(&origin(), creation(&s)).unwrap();
    diesel::insert_into(users::table)
        .values((
            users::id.eq(Uuid::new_v4()),
            users::username.eq(&name),
            users::kind.eq("credited"),
            users::display_name.eq("Late Addition"),
        ))
        .execute(&mut conn(&state))
        .unwrap();

    let r = b.post(&app, REGISTER_FINISH, finish_body(&s, &cred)).await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    assert_eq!(r.error(), username::UNAVAILABLE);
    assert!(r.set_cookie(SESSION).is_none());
    assert!(!b.cookies.contains_key(SESSION));
    let rows: i64 = users::table
        .filter(users::id.eq(would_be))
        .count()
        .get_result(&mut conn(&state))
        .unwrap();
    assert_eq!(rows, 0);
    assert!(passkey_ids(&state, would_be).is_empty());
    assert!(session_hashes(&state, would_be).is_empty());
    let raw_id = Vec::<u8>::from(cred.raw_id);
    let stored: i64 = passkeys::table
        .filter(passkeys::credential_id.eq(&raw_id))
        .count()
        .get_result(&mut conn(&state))
        .unwrap();
    assert_eq!(stored, 0);
    let kind: String = users::table
        .filter(users::username.eq(&name))
        .select(users::kind)
        .first(&mut conn(&state))
        .unwrap();
    assert_eq!(kind, "credited");
}

/// The down migration refuses while sign-in data exists. It runs on a scratch
/// copy of the schema inside a rolled-back transaction in packit_test, so
/// real rows and concurrent tests are never touched.
#[test]
fn the_down_migration_never_drops_sign_in_data() {
    use diesel::connection::SimpleConnection;
    use diesel::sql_types::BigInt;

    let Some(pool) = test_db() else {
        eprintln!("TEST_DATABASE_URL not set; skipping");
        return;
    };
    // Each statement runs in a savepoint, so an expected failure leaves the
    // enclosing transaction usable.
    fn run(conn: &mut PgConnection, sql: &str) -> QueryResult<()> {
        conn.transaction(|conn| conn.batch_execute(sql))
    }
    fn count(conn: &mut PgConnection, query: &str) -> i64 {
        diesel::select(diesel::dsl::sql::<BigInt>(&format!("({query})")))
            .get_result(conn)
            .unwrap()
    }
    // Every row and column the refusal must preserve.
    fn snapshot(conn: &mut PgConnection) -> [i64; 6] {
        [
            "SELECT count(*) FROM users",
            "SELECT count(*) FROM users WHERE kind = 'player'",
            "SELECT count(*) FROM passkeys",
            "SELECT count(*) FROM sessions",
            "SELECT count(*) FROM scores WHERE user_id IS NOT NULL",
            "SELECT count(*) FROM solution_shares WHERE created_by IS NOT NULL",
        ]
        .map(|q| count(conn, q))
    }
    let up = include_str!("../../migrations/2026-09-14-000100_passkey_auth/up.sql");
    let down = include_str!("../../migrations/2026-09-14-000100_passkey_auth/down.sql");
    let seeded = up
        .lines()
        .filter(|l| l.trim().starts_with("(gen_random_uuid(), "))
        .count() as i64;
    let credited = "(SELECT id FROM users WHERE username = 'friedman')";
    let cases = [
        (
            "INSERT INTO users (id, username, kind) VALUES (gen_random_uuid(), 'someone', 'player')"
                .to_string(),
            "DELETE FROM users WHERE kind = 'player'",
        ),
        (
            format!(
                "INSERT INTO passkeys (credential_id, user_id, passkey) \
                 VALUES (decode('01', 'hex'), {credited}, '{{}}')"
            ),
            "DELETE FROM passkeys",
        ),
        (
            format!(
                "INSERT INTO sessions (token_hash, user_id, expires_at) \
                 VALUES (decode(repeat('ab', 32), 'hex'), {credited}, now())"
            ),
            "DELETE FROM sessions",
        ),
        (
            format!(
                "INSERT INTO scores (player, n, side, arrangement, user_id) \
                 VALUES ('p', 1, 1, '{{}}', {credited})"
            ),
            "UPDATE scores SET user_id = NULL",
        ),
        (
            format!(
                "INSERT INTO solution_shares (token, payload_hash, n, code, created_by) \
                 VALUES (repeat('a', 24), decode(repeat('cd', 32), 'hex'), 1, repeat('0', 70), \
                 {credited})"
            ),
            "UPDATE solution_shares SET created_by = NULL",
        ),
    ];
    pool.get()
        .unwrap()
        .test_transaction::<_, diesel::result::Error, _>(|conn| {
            let schema = format!("passkey_down_{}", Uuid::new_v4().simple());
            conn.batch_execute(&format!(
                "CREATE SCHEMA {schema}; SET LOCAL search_path TO {schema}, public;"
            ))?;
            conn.batch_execute(include_str!(
                "../../migrations/00000000000000_initial/up.sql"
            ))?;
            conn.batch_execute(include_str!(
                "../../migrations/2026-09-13-000000_solution_shares/up.sql"
            ))?;
            conn.batch_execute(include_str!(
                "../../migrations/2026-09-14-000000_share_glue/up.sql"
            ))?;
            conn.batch_execute(up)?;
            assert!(seeded > 0);
            assert_eq!(snapshot(conn), [seeded, 0, 0, 0, 0, 0]);
            for (insert, cleanup) in &cases {
                run(conn, insert)?;
                let before = snapshot(conn);
                let refused = run(conn, down).unwrap_err().to_string();
                assert!(
                    refused.contains("refusing to drop sign-in data"),
                    "{refused}"
                );
                assert_eq!(snapshot(conn), before, "{insert}");
                run(conn, cleanup)?;
            }
            // Only the seeded credited profiles are left: down goes through.
            assert_eq!(snapshot(conn), [seeded, 0, 0, 0, 0, 0]);
            run(conn, down)?;
            let tables = count(
                conn,
                "SELECT count(*) FROM information_schema.tables WHERE table_schema = \
                 current_schema() AND table_name IN ('users', 'passkeys', 'sessions')",
            );
            let columns = count(
                conn,
                "SELECT count(*) FROM information_schema.columns WHERE table_schema = \
                 current_schema() AND column_name IN ('user_id', 'created_by')",
            );
            assert_eq!((tables, columns), (0, 0));
            // The now-anonymous score and share are kept.
            assert_eq!(count(conn, "SELECT count(*) FROM scores"), 1);
            assert_eq!(count(conn, "SELECT count(*) FROM solution_shares"), 1);

            // The migration before this one reverts next, in reverse order,
            // and its own guard still sees a glue-free table.
            let constraint = |conn: &mut PgConnection, name: &str| {
                count(
                    conn,
                    &format!(
                        "SELECT count(*) FROM pg_constraint c JOIN pg_namespace n \
                         ON n.oid = c.connamespace WHERE n.nspname = current_schema() \
                         AND c.conname = '{name}'"
                    ),
                )
            };
            assert_eq!(constraint(conn, "solution_shares_code_check"), 1);
            run(
                conn,
                include_str!("../../migrations/2026-09-14-000000_share_glue/down.sql"),
            )?;
            assert_eq!(constraint(conn, "solution_shares_code_check"), 0);
            assert_eq!(constraint(conn, "solution_shares_check"), 1);
            assert_eq!(count(conn, "SELECT count(*) FROM solution_shares"), 1);
            Ok(())
        });
}
