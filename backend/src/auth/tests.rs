//! End-to-end passkey flows through the real router, with software
//! authenticators. Database tests skip when TEST_DATABASE_URL is unset.

use super::{hex, random_bytes, session, username, IP_BURST, USERNAME_BURST};
use crate::build_app;
use crate::config::PublicOrigin;
use crate::handlers::auth::{
    CEREMONY_INVALID, NOT_SIGNED_IN, PASSKEY_TAKEN, REAUTH, SIGN_IN_FAILED, VERIFY_FAILED,
};
use crate::schema::{passkeys, sessions, users};
use crate::test_support::{state_for, test_db, unconnected_pool, TEST_URL};
use crate::AppState;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, HeaderMap, Method, Request, StatusCode};
use axum::Router;
use chrono::{DateTime, TimeDelta, Utc};
use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, PooledConnection};
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
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
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
    assert!(r.set_cookie(CEREMONY).unwrap().contains("Max-Age=0"));
    assert!(!b.cookies.contains_key(CEREMONY));

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
    let state = AppState::new(true, unconnected_pool(), origin, Some(proxy)).unwrap();
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
async fn login_start_is_rate_limited_per_username() {
    let Some((_state, app)) = db_app() else {
        return;
    };
    let name = fresh("limit");
    for _ in 0..USERNAME_BURST {
        let r = start(&app, &mut Browser::new(), LOGIN_START, &name).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    }
    for variant in [name.clone(), format!(" {} ", name.to_uppercase())] {
        let r = start(&app, &mut Browser::new(), LOGIN_START, &variant).await;
        assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS, "{variant}");
        assert!(r.headers.contains_key(header::RETRY_AFTER));
    }
    let r = start(&app, &mut Browser::new(), LOGIN_START, &fresh("limit")).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
}

/// Accounts gate nothing else: the public API works with no Origin, from
/// another origin, and with no, malformed or unknown session cookies.
#[tokio::test]
async fn existing_endpoints_still_work_anonymously() {
    let Some((_state, app)) = db_app() else {
        return;
    };
    let squares = shared::Arrangement {
        n: 2,
        side: 2.0,
        squares: vec![
            shared::Placement {
                cx: 0.5,
                cy: 0.5,
                theta: 0.0,
            },
            shared::Placement {
                cx: 1.5,
                cy: 0.5,
                theta: 0.0,
            },
        ],
    };
    let code = shared::share::encode(&squares);
    let unknown = format!("{SESSION}={}", "00".repeat(32));
    for cookie in [None, Some(format!("{SESSION}=zz")), Some(unknown)] {
        for origin in [None, Some("https://evil.test")] {
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
            let r = call(&app, req(Method::GET, "/api/records", None)).await;
            assert_eq!(r.status, StatusCode::OK);
            let score = json!({ "player": "anon", "arrangement": squares });
            let r = call(&app, req(Method::POST, "/api/scores", Some(score))).await;
            assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
            let share = json!({ "n": 2, "code": code });
            let r = call(&app, req(Method::POST, "/api/shares", Some(share))).await;
            assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
            let path = r.body["url"]
                .as_str()
                .unwrap()
                .strip_prefix(TEST_URL)
                .unwrap()
                .to_owned();
            let r = call(&app, req(Method::GET, &path, None)).await;
            assert_eq!(r.status, StatusCode::FOUND);
            let r = call(&app, req(Method::GET, &format!("/play/2?s={code}"), None)).await;
            assert_eq!(r.status, StatusCode::OK);
            assert!(r.body.as_str().unwrap().contains("og:image"));
            assert!(r.headers.get(header::SET_COOKIE).is_none());
        }
    }
}
