mod auth;
mod config;
mod db;
mod handlers;
mod models;
mod schema;
#[cfg(test)]
mod test_support;

use crate::auth::proxy::ProxyTrust;
use crate::config::{Config, PublicOrigin};
use crate::db::DbPool;
use axum::http::StatusCode;
use axum::{
    routing::{get, post},
    Router,
};
use clap::Parser;
use memory_serve::{load_assets, CacheControl, MemoryServe};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use ws_bridge::WsEndpoint;

#[derive(Parser, Debug, Clone)]
#[command(name = "packit")]
#[command(about = "packit game server")]
struct Args {
    /// Enable development mode (relaxed config requirements)
    #[arg(long)]
    dev_mode: bool,
}

pub struct AppState {
    pub dev_mode: bool,
    pub db_pool: DbPool,
    /// Origin for absolute link-preview URLs, from `PUBLIC_URL`.
    pub public_url: String,
    pub auth: auth::Auth,
}

impl AppState {
    pub fn new(
        dev_mode: bool,
        db_pool: DbPool,
        public: PublicOrigin,
        proxy: ProxyTrust,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            dev_mode,
            db_pool,
            public_url: public.origin.clone(),
            auth: auth::Auth::new(public, proxy)?,
        })
    }
}

/// Build the full application router from shared state.
///
/// Kept as a pure function of `AppState` so tests can drive the entire app
/// in-process via `tower::ServiceExt::oneshot` — no bound port, no network.
pub fn build_app(state: Arc<AppState>) -> Router {
    // Permissive CORS covers the public API only. `/api/auth` gets none, so
    // other sites can't read its responses.
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Pre-compressed (brotli/gzip), content-negotiated frontend assets with
    // ETag/304 and an SPA fallback to index.html. Assets are embedded at build
    // time in release builds and read from disk in debug builds.
    let frontend = MemoryServe::new(load_assets!("../frontend/dist"))
        .index_file(Some("/index.html"))
        .fallback(Some("/index.html"))
        .fallback_status(StatusCode::OK)
        .html_cache_control(CacheControl::NoCache)
        .cache_control(CacheControl::Long)
        .into_router();

    let public = Router::new()
        .route(
            "/api/shares",
            post(handlers::shares::create).layer(axum::extract::DefaultBodyLimit::max(16 * 1024)),
        )
        .route("/s/:token", get(handlers::shares::resolve))
        .route("/api/health", get(handlers::health::health))
        .route("/api/records", get(handlers::records::records))
        .route(
            "/api/scores",
            get(handlers::scores::list).post(handlers::scores::submit),
        )
        .route("/api/scores/:id", get(handlers::scores::detail))
        .route("/api/preview.png", get(handlers::preview::preview_png))
        .route("/play/:n", get(handlers::preview::play))
        .with_state(state.clone())
        .route(shared::AppSocket::PATH, handlers::websocket::handler())
        .merge(frontend)
        .layer(cors);

    handlers::auth::router(state.clone())
        .merge(public)
        // Outermost, so no layer or handler below ever sees the proxy token.
        // Request logging, if added, belongs inside this.
        .layer(axum::middleware::from_fn_with_state(
            state,
            auth::proxy::edge,
        ))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Log panics with their source location and a backtrace via tracing, so
    // crashes are captured in structured logs rather than only on stderr.
    // Set RUST_BACKTRACE=1 to populate the backtrace.
    std::panic::set_hook(Box::new(|panic_info| {
        let backtrace = std::backtrace::Backtrace::capture();
        match panic_info.location() {
            Some(loc) => tracing::error!(
                "PANIC at {}:{}:{}: {}",
                loc.file(),
                loc.line(),
                loc.column(),
                panic_info
            ),
            None => tracing::error!("PANIC: {}", panic_info),
        }
        tracing::error!("Backtrace:\n{backtrace}");
    }));

    if args.dev_mode {
        tracing::warn!("DEV MODE ENABLED");
    }

    // Load .env file if present
    dotenvy::dotenv().ok();

    let config = Config::from_env(args.dev_mode)?;

    // Create database pool and run migrations
    let pool = db::create_pool()?;

    tracing::info!("Running database migrations...");
    match db::run_migrations(&pool) {
        Ok(applied) => {
            if applied.is_empty() {
                tracing::info!("Database is up to date");
            } else {
                for m in &applied {
                    tracing::info!("Applied migration: {}", m);
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to run migrations: {}", e);
            return Err(e);
        }
    }

    let app_state = Arc::new(AppState::new(
        args.dev_mode,
        pool,
        config.public.clone(),
        config.proxy.clone(),
    )?);

    let app = build_app(app_state);

    // Bind and serve. The peer address feeds the auth rate limits.
    let listener = tokio::net::TcpListener::bind(config.bind_addr()).await?;
    tracing::info!("Listening on {}", listener.local_addr()?);

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("Received Ctrl+C, shutting down..."),
        _ = terminate => tracing::info!("Received SIGTERM, shutting down..."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{state_for, test_db, unconnected_pool, TEST_URL};
    use axum::body::Body;
    use axum::http::{header, Request, StatusCode};
    use tower::ServiceExt;

    /// State with a pool that is never actually connected. The health and asset
    /// routes don't touch the database, so this exercises the whole router
    /// without a running Postgres.
    fn test_state() -> Arc<AppState> {
        state_for(unconnected_pool())
    }

    #[tokio::test]
    async fn health_returns_ok_json() {
        let resp = build_app(test_state())
            .oneshot(Request::get("/api/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(ct.contains("json"), "expected JSON, got {ct}");

        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let parsed: shared::HealthResponse = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(parsed.status, "ok");
    }

    #[tokio::test]
    async fn index_served_as_html() {
        let resp = build_app(test_state())
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(ct.contains("html"), "expected HTML, got {ct}");
    }

    #[tokio::test]
    async fn unknown_path_falls_back_to_index() {
        // SPA fallback: any unmatched path serves index.html with 200.
        let resp = build_app(test_state())
            .oneshot(
                Request::get("/some/client/route")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    async fn call<T: serde::de::DeserializeOwned>(
        app: &Router,
        req: Request<Body>,
    ) -> (StatusCode, T) {
        let resp = app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    fn post_json(uri: &str, body: &impl serde::Serialize) -> Request<Body> {
        Request::post(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(body).unwrap()))
            .unwrap()
    }

    fn two_squares(player: &str, side: f64) -> shared::SubmitScore {
        let sq = |cx| shared::Placement {
            cx,
            cy: 0.5,
            theta: 0.0,
        };
        shared::SubmitScore {
            player: player.to_string(),
            arrangement: shared::Arrangement {
                n: 2,
                side,
                squares: vec![sq(0.5), sq(1.5)],
            },
        }
    }

    async fn fetch(req: Request<Body>) -> (StatusCode, header::HeaderMap, Vec<u8>) {
        let resp = build_app(test_state()).oneshot(req).await.unwrap();
        let (parts, body) = resp.into_parts();
        let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
        (parts.status, parts.headers, bytes.to_vec())
    }

    async fn fetch_uri(uri: &str) -> (StatusCode, header::HeaderMap, Vec<u8>) {
        fetch(Request::get(uri).body(Body::empty()).unwrap()).await
    }

    /// The built page with `tags` inserted before `</head>` and nothing else
    /// changed, so hashed scripts and SRI stay intact.
    fn assert_page_with_tags(html: &str) -> &str {
        let page = include_str!("../../frontend/dist/index.html");
        let head_end = page.find("</head>").unwrap();
        assert!(html.starts_with(&page[..head_end]), "{html}");
        assert!(html.ends_with(&page[head_end..]), "{html}");
        &html[head_end..html.len() - (page.len() - head_end)]
    }

    #[tokio::test]
    async fn shared_play_page_describes_the_packing() {
        let code = shared::share::encode(&two_squares("", 2.0).arrangement);
        let req = Request::get(format!("/play/2?s={code}"))
            .header(header::HOST, "evil.example")
            .header("x-forwarded-host", "evil.example")
            .body(Body::empty())
            .unwrap();
        let (status, headers, body) = fetch(req).await;
        assert_eq!(status, StatusCode::OK);
        assert!(headers[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .contains("html"));
        let html = String::from_utf8(body).unwrap();
        let tags = assert_page_with_tags(&html);
        for expected in [
            format!(r#"<meta property="og:url" content="{TEST_URL}/play/2?s={code}">"#),
            format!(
                r#"<meta property="og:image" content="{TEST_URL}/api/preview.png?n=2&amp;s={code}">"#
            ),
            format!(
                r#"<meta name="twitter:image" content="{TEST_URL}/api/preview.png?n=2&amp;s={code}">"#
            ),
            r#"<meta name="twitter:card" content="summary_large_image">"#.to_string(),
            r#"<meta property="og:title" content="2 squares in a 2.0000 box">"#.to_string(),
        ] {
            assert!(tags.contains(&expected), "missing {expected} in {tags}");
        }
        assert!(tags.contains("A valid packing of 2 unit squares"), "{tags}");
        assert!(!tags.contains("evil.example"));
    }

    #[tokio::test]
    async fn play_page_without_a_valid_code_is_generic() {
        for (uri, url) in [
            ("/play/2", format!("{TEST_URL}/play/2")),
            ("/play/2?s=zz", format!("{TEST_URL}/play/2")),
            ("/play/2?s=%3Cscript%3E", format!("{TEST_URL}/play/2")),
            ("/play/abc", format!("{TEST_URL}/")),
            ("/play/0", format!("{TEST_URL}/")),
        ] {
            let (status, _, body) = fetch_uri(uri).await;
            assert_eq!(status, StatusCode::OK, "{uri}");
            let html = String::from_utf8(body).unwrap();
            let tags = assert_page_with_tags(&html);
            assert!(
                tags.contains(&format!(r#"<meta property="og:url" content="{url}">"#)),
                "{uri}: {tags}"
            );
            assert!(!tags.contains("og:image"), "{uri}: {tags}");
            assert!(!tags.contains("<script"), "{uri}: {tags}");
            assert!(tags.contains(r#"content="summary""#), "{uri}: {tags}");
        }
    }

    #[tokio::test]
    async fn preview_png_draws_each_packing() {
        let a = two_squares("", 2.0).arrangement;
        let mut b = a.clone();
        b.side = 2.5;
        b.squares[1].theta = 0.3;
        let mut pngs = Vec::new();
        for arrangement in [&a, &b] {
            let code = shared::share::encode(arrangement);
            let (status, headers, png) = fetch_uri(&format!("/api/preview.png?n=2&s={code}")).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(headers[header::CONTENT_TYPE], "image/png");
            assert_eq!(
                headers[header::CACHE_CONTROL],
                "public, max-age=31536000, immutable"
            );
            assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
            // IHDR width and height.
            assert_eq!(png[16..20], 1200u32.to_be_bytes());
            assert_eq!(png[20..24], 630u32.to_be_bytes());
            pngs.push(png);
        }
        assert_ne!(
            pngs[0], pngs[1],
            "distinct packings must preview differently"
        );
    }

    #[tokio::test]
    async fn preview_png_rejects_invalid_codes() {
        let code = shared::share::encode(&two_squares("", 2.0).arrangement);
        for uri in [
            "/api/preview.png".to_string(),
            "/api/preview.png?n=2".to_string(),
            "/api/preview.png?n=2&s=zz".to_string(),
            format!("/api/preview.png?n=3&s={code}"),
            format!("/api/preview.png?n=-2&s={code}"),
        ] {
            let (status, headers, _) = fetch_uri(&uri).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
            assert_eq!(headers[header::CACHE_CONTROL], "no-store", "{uri}");
        }
    }

    #[tokio::test]
    async fn records_served_from_refs() {
        let (status, records): (_, Vec<shared::KnownRecord>) = call(
            &build_app(test_state()),
            Request::get("/api/records").body(Body::empty()).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(records[0].n, 1);
    }

    #[tokio::test]
    async fn overlapping_submission_rejected_before_db() {
        let mut body = two_squares("ada", 2.0);
        body.arrangement.squares[1].cx = 1.0;
        let (status, err): (_, shared::ApiError) =
            call(&build_app(test_state()), post_json("/api/scores", &body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(err.error.contains("overlap"), "{}", err.error);
    }

    /// Submit / list / detail against a real Postgres. Runs only when
    /// TEST_DATABASE_URL is set; CI provides one.
    #[tokio::test]
    async fn scores_roundtrip_against_postgres() {
        let Some(db_pool) = test_db() else {
            eprintln!("TEST_DATABASE_URL not set; skipping");
            return;
        };
        let app = build_app(state_for(db_pool));

        let (status, loose): (_, shared::ScoreEntry) =
            call(&app, post_json("/api/scores", &two_squares("loose", 3.0))).await;
        assert_eq!(status, StatusCode::OK);
        let (_, tight): (_, shared::ScoreEntry) =
            call(&app, post_json("/api/scores", &two_squares("tight", 2.0))).await;

        let (status, board): (_, Vec<shared::ScoreEntry>) = call(
            &app,
            Request::get("/api/scores?n=2&limit=200")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(board.iter().any(|e| e.id == tight.id));
        assert!(board.windows(2).all(|w| w[0].side <= w[1].side));

        let (_, leaders): (_, Vec<shared::ScoreEntry>) = call(
            &app,
            Request::get("/api/scores").body(Body::empty()).unwrap(),
        )
        .await;
        assert!(leaders.iter().any(|e| e.n == 2 && e.side <= 2.0));

        let (status, detail): (_, shared::ScoreDetail) = call(
            &app,
            Request::get(format!("/api/scores/{}", loose.id))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(detail.entry.id, loose.id);
        assert_eq!(detail.entry.side, 3.0);
        // Rank is computed at read time, so the tighter packing now outranks it.
        assert!(detail.entry.rank > tight.rank);
        assert_eq!(detail.arrangement, two_squares("loose", 3.0).arrangement);

        let (status, _): (_, shared::ApiError) = call(
            &app,
            Request::get(format!("/api/scores/{}", uuid::Uuid::new_v4()))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    /// Scores with identical side and timestamp still get distinct ranks, and
    /// `detail` agrees with the order `list` returns them in.
    #[tokio::test]
    async fn equal_side_and_timestamp_ranks_are_unique() {
        use crate::schema::scores;
        use diesel::prelude::*;

        let Some(db_pool) = test_db() else {
            eprintln!("TEST_DATABASE_URL not set; skipping");
            return;
        };

        // A side no other run will reuse, so only our two rows tie.
        let side = 50.0 + (uuid::Uuid::new_v4().as_u128() % 1_000_000) as f64 * 1e-6;
        let at = chrono::DateTime::from_timestamp(1_700_000_000, 0)
            .unwrap()
            .naive_utc();
        let ids = [uuid::Uuid::new_v4(), uuid::Uuid::new_v4()];
        let arrangement = serde_json::to_value(shared::Arrangement {
            n: 97,
            side,
            squares: vec![],
        })
        .unwrap();
        let mut conn = db_pool.get().unwrap();
        for id in ids {
            diesel::insert_into(scores::table)
                .values((
                    scores::id.eq(id),
                    scores::player.eq("tie"),
                    scores::n.eq(97),
                    scores::side.eq(side),
                    scores::arrangement.eq(&arrangement),
                    scores::submitted_at.eq(at),
                ))
                .execute(&mut conn)
                .unwrap();
        }
        drop(conn);

        let app = build_app(state_for(db_pool));
        let mut ranks = Vec::new();
        for id in ids {
            let (status, detail): (_, shared::ScoreDetail) = call(
                &app,
                Request::get(format!("/api/scores/{id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
            ranks.push(detail.entry.rank);
        }
        assert_eq!(ranks[0].abs_diff(ranks[1]), 1);

        let (_, board): (_, Vec<shared::ScoreEntry>) = call(
            &app,
            Request::get("/api/scores?n=97&limit=200")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        for (id, rank) in ids.iter().zip(&ranks) {
            if let Some(e) = board.iter().find(|e| e.id == *id) {
                assert_eq!(e.rank, *rank);
            }
        }
    }
    #[tokio::test]
    async fn short_links_validate_before_using_the_database() {
        let mut arr = two_squares("", 2.0).arrangement;
        arr.squares[0].cx = 1001.0;
        for body in [
            shared::CreateShare {
                n: 0,
                code: String::new(),
            },
            shared::CreateShare {
                n: 101,
                code: String::new(),
            },
            shared::CreateShare {
                n: 2,
                code: "zz".into(),
            },
            shared::CreateShare {
                n: 3,
                code: shared::share::encode(&two_squares("", 2.0).arrangement),
            },
            shared::CreateShare {
                n: 2,
                code: shared::share::encode(&arr),
            },
        ] {
            let (status, _): (_, shared::ApiError) =
                call(&build_app(test_state()), post_json("/api/shares", &body)).await;
            assert_eq!(status, StatusCode::BAD_REQUEST);
        }
        let oversized = shared::CreateShare {
            n: 2,
            code: "0".repeat(20_000),
        };
        let response = build_app(test_state())
            .oneshot(post_json("/api/shares", &oversized))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let (status, _, _) = fetch_uri("/s/not-a-token").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn short_links_roundtrip_deduplicate_and_keep_solution_previews() {
        let Some(db_pool) = test_db() else {
            eprintln!("TEST_DATABASE_URL not set; skipping");
            return;
        };
        let app = build_app(state_for(db_pool));
        // Both valid and unfinished snapshots are shareable. The full f64
        // value survives storage and redirect without passing through f32.
        let mut arr = two_squares("", 2.0 + 2e-10).arrangement;
        arr.squares[1].cx = 1.2;
        let code = shared::share::encode(&arr);
        let body = shared::CreateShare {
            n: arr.n,
            code: code.clone(),
        };
        let upper = shared::CreateShare {
            n: arr.n,
            code: code.to_uppercase(),
        };
        let ((status, a), (_, b)): ((_, shared::ShortShare), (_, shared::ShortShare)) = tokio::join!(
            call(&app, post_json("/api/shares", &body)),
            call(&app, post_json("/api/shares", &upper)),
        );
        assert_eq!(status, StatusCode::OK);
        assert_eq!(a, b, "concurrent canonical duplicates reuse the same URL");
        assert_eq!(a.url.len(), TEST_URL.len() + 3 + 24);
        let response = app
            .clone()
            .oneshot(
                Request::get(a.url.strip_prefix(TEST_URL).unwrap())
                    .header(header::HOST, "evil.example")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FOUND);
        let location = response.headers()[header::LOCATION].to_str().unwrap();
        assert_eq!(location, format!("{TEST_URL}/play/2?s={code}"));
        assert_eq!(
            shared::share::decode(location.split("?s=").nth(1).unwrap(), 2).unwrap(),
            arr
        );
        let page = app
            .clone()
            .oneshot(
                Request::get(location.strip_prefix(TEST_URL).unwrap())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(page.status(), StatusCode::OK);
        let html = String::from_utf8(
            axum::body::to_bytes(page.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        let tags = assert_page_with_tags(&html);
        assert!(tags.contains(r#"name="twitter:card" content="summary_large_image""#));
        assert!(tags.contains(&format!("{TEST_URL}/api/preview.png?n=2&amp;s={code}")));
        let (status, headers, png) = fetch_uri(&format!("/api/preview.png?n=2&s={code}")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers[header::CONTENT_TYPE], "image/png");
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        arr.squares[0].theta = 0.1;
        let (_, different): (_, shared::ShortShare) = call(
            &app,
            post_json(
                "/api/shares",
                &shared::CreateShare {
                    n: 2,
                    code: shared::share::encode(&arr),
                },
            ),
        )
        .await;
        assert_ne!(a, different);
        let missing = format!("/s/{}", &uuid::Uuid::new_v4().simple().to_string()[..24]);
        let response = app
            .oneshot(Request::get(missing).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
