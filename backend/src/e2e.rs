//! Passkey flows end to end in real Chrome: the built app served by this
//! backend against the test database, driven over WebDriver, with Chrome's
//! virtual authenticator as the passkey. Every step is a standard WebDriver
//! command (navigation, element lookup, clicks, typing, and the WebAuthn
//! extension's virtual-authenticator endpoints); nothing runs page script.
//! Results are checked in the database, never by reading page state.
//!
//! Plain `cargo test` skips this unless `CHROMEDRIVER` is set. For a run
//! that must not skip, set `PACKIT_E2E=1`, which turns a missing driver or
//! database into a failure. Build the frontend first, since the server
//! serves `frontend/dist`:
//!
//! ```sh
//! (cd frontend && trunk build)
//! PACKIT_E2E=1 CHROMEDRIVER=/path/to/chromedriver \
//! TEST_DATABASE_URL=postgresql://packit:dev_password@localhost:5433/packit_test \
//!   cargo test -p backend --locked e2e -- --test-threads=1
//! ```
//!
//! `CHROME_BINARY` picks the browser when chromedriver can't find it.

use crate::auth::proxy::ProxyTrust;
use crate::config::PublicOrigin;
use crate::db::DbPool;
use crate::schema::{passkeys, scores, sessions, solution_shares, users};
use crate::test_support::test_db;
use crate::{build_app, AppState};
use axum::http::Method;
use chrono::{DateTime, Utc};
use diesel::prelude::*;
use fantoccini::elements::Element;
use fantoccini::wd::WebDriverCompatibleCommand;
use fantoccini::{Client, ClientBuilder, Locator};
use serde_json::{json, Value};
use shared::glue::{Feature, Glue};
use shared::{share, Arrangement, Placement};
use std::net::SocketAddr;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};
use url::Url;
use uuid::Uuid;

/// How long any one step may take. Settling on the CPU is the slowest.
const STEP: Duration = Duration::from_secs(45);

/// chromedriver, stopped when the test ends.
struct Driver(Child);

impl Drop for Driver {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// The WebDriver WebAuthn extension's virtual-authenticator commands.
#[derive(Debug)]
enum Authenticator {
    /// A platform passkey: CTAP2 over the internal transport, with resident
    /// keys and user verification that always succeeds.
    Add,
    Remove(String),
}

impl WebDriverCompatibleCommand for Authenticator {
    fn endpoint(&self, base: &Url, session: Option<&str>) -> Result<Url, url::ParseError> {
        let session = session.expect("a WebDriver session");
        let path = match self {
            Self::Add => format!("session/{session}/webauthn/authenticator"),
            Self::Remove(id) => format!("session/{session}/webauthn/authenticator/{id}"),
        };
        base.join(&path)
    }

    fn method_and_body(&self, _: &Url) -> (Method, Option<String>) {
        match self {
            Self::Add => (
                Method::POST,
                Some(
                    json!({
                        "protocol": "ctap2",
                        "transport": "internal",
                        "hasResidentKey": true,
                        "hasUserVerification": true,
                        "isUserConsenting": true,
                        "isUserVerified": true,
                    })
                    .to_string(),
                ),
            ),
            Self::Remove(_) => (Method::DELETE, None),
        }
    }
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

async fn start_browser(chromedriver: &str) -> (Driver, Client) {
    let port = free_port();
    let driver = Driver(
        Command::new(chromedriver)
            .arg(format!("--port={port}"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap_or_else(|e| panic!("can't start {chromedriver}: {e}")),
    );
    let mut options = json!({
        "args": ["--headless=new", "--no-sandbox", "--window-size=1280,1000"],
    });
    if let Ok(binary) = std::env::var("CHROME_BINARY") {
        options["binary"] = binary.into();
    }
    let mut capabilities = serde_json::Map::new();
    capabilities.insert("goog:chromeOptions".into(), options);
    let started = Instant::now();
    loop {
        match ClientBuilder::native()
            .capabilities(capabilities.clone())
            .connect(&format!("http://127.0.0.1:{port}"))
            .await
        {
            Ok(client) => return (driver, client),
            Err(e) if started.elapsed() > Duration::from_secs(20) => {
                panic!("chromedriver didn't start a session: {e}")
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(200)).await,
        }
    }
}

/// Serve the app on a free port, as `http://localhost:port`: a passkey
/// origin browsers accept without TLS.
async fn serve(pool: DbPool) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://localhost:{}", listener.local_addr().unwrap().port());
    let public = PublicOrigin::parse(&origin, true).unwrap();
    let state = Arc::new(AppState::new(true, pool, public, ProxyTrust::default()).unwrap());
    let app = build_app(state).into_make_service_with_connect_info::<SocketAddr>();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    origin
}

struct Browser {
    client: Client,
    origin: String,
    pool: DbPool,
}

impl Browser {
    async fn goto(&self, path: &str) {
        self.client
            .goto(&format!("{}{path}", self.origin))
            .await
            .unwrap();
    }

    async fn find(&self, css: &str) -> Element {
        self.client
            .wait()
            .at_most(STEP)
            .for_element(Locator::Css(css))
            .await
            .unwrap_or_else(|e| panic!("{css}: {e}"))
    }

    async fn click(&self, css: &str) {
        self.find(css).await.click().await.unwrap();
    }

    /// Wait until the text of `css` passes `check`, and return it.
    async fn text(&self, css: &str, check: impl Fn(&str) -> bool) -> String {
        let started = Instant::now();
        loop {
            if let Ok(e) = self.client.find(Locator::Css(css)).await {
                if let Ok(text) = e.text().await {
                    if check(&text) {
                        return text;
                    }
                }
            }
            if started.elapsed() > STEP {
                panic!("{css} never matched; {}", self.page_state().await);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// The page's status messages, to explain a failed step.
    async fn page_state(&self) -> String {
        let mut state = Vec::new();
        for css in [
            ".pg-status",
            ".pg-share-status",
            ".pg-share-error",
            ".pg-sign-in",
            ".account-error",
            ".account-notice",
        ] {
            if let Ok(e) = self.client.find(Locator::Css(css)).await {
                state.push(format!("{css}: {:?}", e.text().await.unwrap_or_default()));
            }
        }
        state.join(", ")
    }

    /// The play screen's button labelled `label`.
    async fn button(&self, label: &str) -> Element {
        let started = Instant::now();
        loop {
            for b in self
                .client
                .find_all(Locator::Css(".pg-submit button, .pg-advanced button"))
                .await
                .unwrap()
            {
                if b.text().await.unwrap_or_default() == label {
                    return b;
                }
            }
            assert!(started.elapsed() < STEP, "no {label} button");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    async fn press(&self, label: &str) {
        self.button(label).await.click().await.unwrap();
    }

    /// Reset the scene from the Advanced panel: an edit after a request.
    async fn reset_scene(&self) {
        let advanced = self.find(".pg-advanced").await;
        if advanced.attr("open").await.unwrap().is_none() {
            self.click(".pg-advanced summary").await;
        }
        self.press("Reset").await;
        self.text(".pg-status", |t| t.starts_with("Fresh grid"))
            .await;
    }

    async fn add_authenticator(&self) -> String {
        let reply = self.client.issue_cmd(Authenticator::Add).await.unwrap();
        reply["authenticatorId"]
            .as_str()
            .or_else(|| reply.as_str())
            .unwrap_or_else(|| panic!("no authenticator id in {reply}"))
            .to_owned()
    }

    async fn remove_authenticator(&self, id: String) {
        self.client
            .issue_cmd(Authenticator::Remove(id))
            .await
            .unwrap();
    }

    /// Type `username` into the open sign-in dialog and press `button`.
    async fn sign_in_with(&self, username: &str, button: &str) {
        let input = self.find("#account-username").await;
        input.clear().await.unwrap();
        input.send_keys(username).await.unwrap();
        self.click(button).await;
        self.text(".account-name", |t| t == username).await;
    }

    async fn sign_out(&self) {
        self.click(".account-sign-out").await;
        self.find(".account-open").await;
    }

    fn user_id(&self, username: &str) -> Uuid {
        users::table
            .filter(users::username.eq(username))
            .select(users::id)
            .first(&mut self.pool.get().unwrap())
            .unwrap()
    }

    /// This account's short links, oldest first.
    fn shares(&self, owner: Uuid) -> Vec<String> {
        solution_shares::table
            .filter(solution_shares::created_by.eq(owner))
            .order(solution_shares::created_at.asc())
            .select(solution_shares::code)
            .load(&mut self.pool.get().unwrap())
            .unwrap()
    }

    async fn wait_shares(&self, owner: Uuid, count: usize) -> Vec<String> {
        let started = Instant::now();
        loop {
            let shares = self.shares(owner);
            if shares.len() >= count {
                assert_eq!(shares.len(), count, "one link per request");
                return shares;
            }
            if started.elapsed() > STEP {
                panic!("only {} links; {}", shares.len(), self.page_state().await);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

/// A box side this run alone uses, so no share dedupes onto an earlier
/// run's link, which keeps its creator. The offset is a multiple of 2^-18
/// below 0.25, exact in f32, so a loaded scene reads back unchanged.
fn unique_side(base: f64) -> f64 {
    base + (Uuid::new_v4().as_u128() % 65_536) as f64 / 262_144.0
}

/// Two separated squares glued to each other and to a wall.
fn glued_scene() -> (Arrangement, Vec<Glue>) {
    let sq = |cx| Placement {
        cx,
        cy: 0.75,
        theta: 0.125,
    };
    let arrangement = Arrangement {
        n: 2,
        side: unique_side(2.5),
        squares: vec![sq(0.75), sq(1.875)],
    };
    let glues = vec![
        Glue {
            a: Feature::Edge { square: 0, edge: 0 },
            b: Feature::Edge { square: 1, edge: 2 },
        },
        Glue {
            a: Feature::Wall(3),
            b: Feature::Midpoint { square: 1, edge: 1 },
        },
    ];
    (arrangement, glues)
}

#[tokio::test(flavor = "multi_thread")]
async fn passkeys_gate_sharing_and_submitting_in_a_real_browser() {
    let required = std::env::var("PACKIT_E2E").is_ok_and(|v| v == "1");
    let Ok(chromedriver) = std::env::var("CHROMEDRIVER") else {
        assert!(!required, "PACKIT_E2E=1 needs CHROMEDRIVER");
        eprintln!("CHROMEDRIVER not set; skipping");
        return;
    };
    let Some(pool) = test_db() else {
        assert!(!required, "PACKIT_E2E=1 needs TEST_DATABASE_URL");
        eprintln!("TEST_DATABASE_URL not set; skipping");
        return;
    };
    let origin = serve(pool.clone()).await;
    let (_driver, client) = start_browser(&chromedriver).await;
    let b = Browser {
        client,
        origin,
        pool,
    };
    let first_key = b.add_authenticator().await;
    let username = format!("e2e-{}", &Uuid::new_v4().simple().to_string()[..12]);

    // A signed-out Share freezes the snapshot and opens sign-in; creating
    // an account then shares exactly that snapshot, not the reset scene.
    let (scene, glues) = glued_scene();
    b.goto(&format!("/play/2?s={}", share::encode(&scene, &glues)))
        .await;
    b.text(".pg-status", |t| t.starts_with("Shared packing loaded"))
        .await;
    b.find(".account-open").await;
    b.press("Share").await;
    b.text(".account-reason", |t| t.starts_with("Sign in to share"))
        .await;
    b.reset_scene().await;
    b.sign_in_with(&username, ".account-create").await;
    let owner = b.user_id(&username);
    let shared = b.wait_shares(owner, 1).await;
    assert_eq!(
        share::decode(&shared[0], 2).unwrap(),
        share::Snapshot {
            arrangement: scene.clone(),
            glues: glues.clone()
        }
    );
    b.text(".pg-share-status", |t| t.contains("link")).await;

    // Certify a packing and share it while signed in: that link holds the
    // certified arrangement exactly. Signed out, Submit freezes it; after
    // signing in again it is submitted as it was.
    let loose = Arrangement {
        n: 2,
        side: unique_side(2.5),
        squares: vec![
            Placement {
                cx: 0.6,
                cy: 0.6,
                theta: 0.0,
            },
            Placement {
                cx: 1.9,
                cy: 0.6,
                theta: 0.0,
            },
        ],
    };
    b.goto(&format!("/play/2?s={}", share::encode(&loose, &[])))
        .await;
    b.text(".account-name", |t| t == username).await;
    b.press("Settle").await;
    b.text(".pg-status", |t| t.starts_with("Ready")).await;
    b.press("Share").await;
    let certified = share::decode(&b.wait_shares(owner, 2).await[1], 2)
        .unwrap()
        .arrangement;
    b.sign_out().await;
    b.press("Submit packing").await;
    b.text(".account-reason", |t| t.starts_with("Sign in to submit"))
        .await;
    b.reset_scene().await;
    b.sign_in_with(&username, ".account-sign-in").await;
    b.text(".pg-status", |t| t.starts_with("Saved!")).await;
    let submitted: Vec<(String, Value)> = scores::table
        .filter(scores::user_id.eq(owner))
        .select((scores::player, scores::arrangement))
        .load(&mut b.pool.get().unwrap())
        .unwrap();
    assert_eq!(submitted.len(), 1);
    assert_eq!(submitted[0].0, username);
    assert_eq!(
        serde_json::from_value::<Arrangement>(submitted[0].1.clone()).unwrap(),
        certified
    );

    // Dismissing sign-in drops the frozen Share, even if signing in follows.
    b.sign_out().await;
    b.press("Share").await;
    b.click(".account-cancel").await;
    b.text(".pg-sign-in", |t| {
        t == "Not signed in, so nothing was shared."
    })
    .await;
    b.click(".account-open").await;
    b.sign_in_with(&username, ".account-sign-in").await;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(
        b.shares(owner).len(),
        2,
        "the cancelled share never went out"
    );

    // An expired session: the page still thinks it's signed in, the server
    // answers 401, and signing in again sends the same snapshot.
    let other = Arrangement {
        side: unique_side(2.75),
        ..scene.clone()
    };
    b.goto(&format!("/play/2?s={}", share::encode(&other, &glues)))
        .await;
    b.text(".account-name", |t| t == username).await;
    diesel::delete(sessions::table.filter(sessions::user_id.eq(owner)))
        .execute(&mut b.pool.get().unwrap())
        .unwrap();
    b.press("Share").await;
    b.text(".pg-sign-in", |t| t.starts_with("You're signed out"))
        .await;
    b.sign_in_with(&username, ".account-sign-in").await;
    let shared = b.wait_shares(owner, 3).await;
    assert_eq!(
        share::decode(&shared[2], 2).unwrap(),
        share::Snapshot {
            arrangement: other,
            glues
        }
    );

    // Signing out ends the session on the server, not just in the page.
    b.sign_out().await;
    let live: i64 = sessions::table
        .filter(sessions::user_id.eq(owner))
        .count()
        .get_result(&mut b.pool.get().unwrap())
        .unwrap();
    assert_eq!(live, 0);
    b.goto("/").await;
    b.find(".account-open").await;

    // After a fresh sign-in, swap the first authenticator for a second one
    // (Chrome allows one internal authenticator at a time) and add a passkey
    // on it. With the first gone, the new passkey signs in on its own.
    b.click(".account-open").await;
    b.sign_in_with(&username, ".account-sign-in").await;
    b.remove_authenticator(first_key).await;
    let second_key = b.add_authenticator().await;
    b.click(".account-add").await;
    b.text(".account-notice", |t| t.starts_with("Passkey added"))
        .await;
    let keys = || -> Vec<(Vec<u8>, Option<DateTime<Utc>>)> {
        passkeys::table
            .filter(passkeys::user_id.eq(owner))
            .order(passkeys::created_at.asc())
            .select((passkeys::credential_id, passkeys::last_used_at))
            .load(&mut b.pool.get().unwrap())
            .unwrap()
    };
    let added = keys();
    assert_eq!(added.len(), 2);
    assert_eq!(added[1].1, None, "the new passkey hasn't signed in yet");
    b.sign_out().await;
    b.click(".account-open").await;
    b.sign_in_with(&username, ".account-sign-in").await;
    assert!(keys()[1].1.is_some(), "the second passkey signed in");
    b.remove_authenticator(second_key).await;
    b.client.close().await.unwrap();
}
