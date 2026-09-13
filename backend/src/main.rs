mod config;
mod db;
mod handlers;
mod models;
mod schema;

use crate::config::Config;
use crate::db::DbPool;
use axum::http::StatusCode;
use axum::{
    routing::{get, post},
    Router,
};
use clap::Parser;
use memory_serve::{load_assets, CacheControl, MemoryServe};
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

#[derive(Clone)]
pub struct AppState {
    pub dev_mode: bool,
    pub db_pool: DbPool,
    /// Origin for absolute link-preview URLs; see `Config::public_url`.
    pub public_url: String,
}

/// Build the full application router from shared state.
///
/// Kept as a pure function of `AppState` so tests can drive the entire app
/// in-process via `tower::ServiceExt::oneshot` — no bound port, no network.
pub fn build_app(state: Arc<AppState>) -> Router {
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

    Router::new()
        .route(
            "/api/shares",
            post(handlers::shares::create).layer(axum::extract::DefaultBodyLimit::max(
                shared::share::MAX_LEN + 1024,
            )),
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
        .with_state(state)
        .route(shared::AppSocket::PATH, handlers::websocket::handler())
        .merge(frontend)
        .layer(cors)
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

    let config = Config::from_env();

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

    let app_state = Arc::new(AppState {
        dev_mode: args.dev_mode,
        db_pool: pool,
        public_url: config.public_url.clone(),
    });

    let app = build_app(app_state);

    // Bind and serve
    let listener = tokio::net::TcpListener::bind(config.bind_addr()).await?;
    tracing::info!("Listening on {}", listener.local_addr()?);

    axum::serve(listener, app)
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
    use axum::body::Body;
    use axum::http::{header, Request, StatusCode};
    use diesel::pg::PgConnection;
    use diesel::r2d2::{ConnectionManager, Pool};
    use tower::ServiceExt;

    /// State with a pool that is never actually connected. The health and asset
    /// routes don't touch the database, so `build_unchecked` lets us exercise
    /// the whole router without a running Postgres.
    fn test_state() -> Arc<AppState> {
        let manager = ConnectionManager::<PgConnection>::new("postgres://invalid/db");
        let db_pool = Pool::builder().build_unchecked(manager);
        Arc::new(AppState {
            dev_mode: true,
            db_pool,
            public_url: TEST_URL.to_string(),
        })
    }

    const TEST_URL: &str = "https://packit.test";

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

    /// Pool for `TEST_DATABASE_URL`, migrated exactly once per test process so
    /// parallel DB tests don't race to create the schema.
    fn test_db() -> Option<DbPool> {
        static POOL: std::sync::OnceLock<Option<DbPool>> = std::sync::OnceLock::new();
        POOL.get_or_init(|| {
            let url = std::env::var("TEST_DATABASE_URL").ok()?;
            let pool = Pool::builder()
                .max_size(4)
                .build(ConnectionManager::<PgConnection>::new(url))
                .unwrap();
            db::run_migrations(&pool).unwrap();
            Some(pool)
        })
        .clone()
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
        let code = shared::share::encode(&two_squares("", 2.0).arrangement, &[]);
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
            let code = shared::share::encode(arrangement, &[]);
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
        let code = shared::share::encode(&two_squares("", 2.0).arrangement, &[]);
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
        let app = build_app(Arc::new(AppState {
            dev_mode: true,
            db_pool,
            public_url: TEST_URL.to_string(),
        }));

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

        let app = build_app(Arc::new(AppState {
            dev_mode: true,
            db_pool,
            public_url: TEST_URL.to_string(),
        }));
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
                code: shared::share::encode(&two_squares("", 2.0).arrangement, &[]),
            },
            shared::CreateShare {
                n: 2,
                code: shared::share::encode(&arr, &[]),
            },
        ] {
            let (status, _): (_, shared::ApiError) =
                call(&build_app(test_state()), post_json("/api/shares", &body)).await;
            assert_eq!(status, StatusCode::BAD_REQUEST);
        }
        let oversized = shared::CreateShare {
            n: 2,
            code: "0".repeat(shared::share::MAX_LEN + 2048),
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
        let app = build_app(Arc::new(AppState {
            dev_mode: true,
            db_pool,
            public_url: TEST_URL.into(),
        }));
        // Both valid and unfinished snapshots are shareable. The full f64
        // value survives storage and redirect without passing through f32.
        let mut arr = two_squares("", 2.0 + 2e-10).arrangement;
        arr.squares[1].cx = 1.2;
        let code = shared::share::encode(&arr, &[]);
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
            shared::share::decode(location.split("?s=").nth(1).unwrap(), 2)
                .unwrap()
                .arrangement,
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
                    code: shared::share::encode(&arr, &[]),
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

    #[tokio::test]
    async fn short_links_keep_glue_through_the_redirect() {
        use shared::glue::{Feature, Glue};
        let Some(db_pool) = test_db() else {
            eprintln!("TEST_DATABASE_URL not set; skipping");
            return;
        };
        let app = build_app(Arc::new(AppState {
            dev_mode: true,
            db_pool,
            public_url: TEST_URL.into(),
        }));
        let arr = two_squares("", 2.0).arrangement;
        let glues = vec![
            Glue {
                a: Feature::Edge { square: 0, edge: 0 },
                b: Feature::Edge { square: 1, edge: 2 },
            },
            Glue {
                a: Feature::Corner {
                    square: 1,
                    corner: 3,
                },
                b: Feature::Wall(2),
            },
            Glue {
                a: Feature::Wall(1),
                b: Feature::Midpoint { square: 0, edge: 3 },
            },
        ];
        let code = shared::share::encode(&arr, &glues);
        let share = |code: String| {
            let app = app.clone();
            async move {
                let (status, link): (_, shared::ShortShare) = call(
                    &app,
                    post_json("/api/shares", &shared::CreateShare { n: 2, code }),
                )
                .await;
                assert_eq!(status, StatusCode::OK);
                link
            }
        };
        let glued = share(code.clone()).await;
        assert_eq!(glued, share(code.to_uppercase()).await);
        assert_ne!(
            glued,
            share(shared::share::encode(&arr, &[])).await,
            "glue is part of the snapshot"
        );
        let response = app
            .clone()
            .oneshot(
                Request::get(glued.url.strip_prefix(TEST_URL).unwrap())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FOUND);
        let location = response.headers()[header::LOCATION].to_str().unwrap();
        assert_eq!(location, format!("{TEST_URL}/play/2?s={code}"));
        let snapshot = shared::share::decode(location.split("?s=").nth(1).unwrap(), 2).unwrap();
        assert_eq!(snapshot.arrangement, arr);
        assert_eq!(snapshot.glues, glues);
        let (status, _, png) = fetch_uri(&format!("/api/preview.png?n=2&s={code}")).await;
        assert_eq!(status, StatusCode::OK);
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    }

    #[test]
    fn share_glue_down_migration_refuses_while_glued_codes_exist() {
        use crate::schema::solution_shares as shares;
        use diesel::connection::SimpleConnection;
        use diesel::prelude::*;
        use shared::glue::{Feature, Glue};
        let Some(pool) = test_db() else {
            eprintln!("TEST_DATABASE_URL not set; skipping");
            return;
        };
        let arr = two_squares("", 2.0).arrangement;
        let glue_free = shared::share::encode(&arr, &[]);
        let glued = shared::share::encode(
            &arr,
            &[Glue {
                a: Feature::Edge { square: 0, edge: 0 },
                b: Feature::Wall(0),
            }],
        );
        // Each statement runs in a savepoint, so an expected failure leaves
        // the enclosing transaction usable.
        let run = |conn: &mut PgConnection, sql: &str| {
            conn.transaction::<_, diesel::result::Error, _>(|conn| conn.batch_execute(sql))
        };
        let insert = |conn: &mut PgConnection, code: &str| {
            conn.transaction::<_, diesel::result::Error, _>(|conn| {
                let id = uuid::Uuid::new_v4();
                diesel::insert_into(shares::table)
                    .values((
                        shares::token.eq(&id.simple().to_string()[..24]),
                        shares::payload_hash.eq(id.as_bytes().repeat(2)),
                        shares::n.eq(2),
                        shares::code.eq(code),
                    ))
                    .execute(conn)
                    .map(|_| ())
            })
        };
        let count = |conn: &mut PgConnection| shares::table.count().get_result::<i64>(conn);
        let down = include_str!("../migrations/2026-09-14-000000_share_glue/down.sql");
        pool.get()
            .unwrap()
            .test_transaction::<_, diesel::result::Error, _>(|conn| {
                // A scratch copy of the table, rolled back with the rest, so
                // real rows and concurrent tests are never touched.
                let schema = format!("share_glue_down_{}", uuid::Uuid::new_v4().simple());
                conn.batch_execute(&format!(
                    "CREATE SCHEMA {schema}; SET LOCAL search_path TO {schema};"
                ))?;
                conn.batch_execute(include_str!(
                    "../migrations/2026-09-13-000000_solution_shares/up.sql"
                ))?;
                conn.batch_execute(include_str!(
                    "../migrations/2026-09-14-000000_share_glue/up.sql"
                ))?;
                insert(conn, &glue_free)?;
                insert(conn, &glued)?;
                let refused = run(conn, down).unwrap_err().to_string();
                assert!(refused.contains("glued share codes exist"), "{refused}");
                assert_eq!(count(conn)?, 2, "a refused rollback keeps every row");
                insert(conn, &glued).expect("the glue constraint is still in place");
                diesel::delete(shares::table.filter(shares::code.ne(&glue_free))).execute(conn)?;
                run(conn, down)?;
                assert_eq!(count(conn)?, 1);
                assert!(
                    insert(conn, &glued).is_err(),
                    "the glue-free constraint is back"
                );
                insert(conn, &glue_free)?;
                Ok(())
            });
    }

    /// One HTTP/1.1 exchange over a real socket, so hyper's own limits apply.
    /// Returns the status, the response head, and the body.
    async fn raw_http(addr: std::net::SocketAddr, request: String) -> (u16, String, Vec<u8>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        let end = response
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .expect("a complete response head");
        let head = String::from_utf8(response[..end].to_vec()).unwrap();
        (
            head[9..12].parse().unwrap(),
            head,
            response[end + 4..].to_vec(),
        )
    }

    #[tokio::test]
    async fn the_largest_share_code_is_stored_and_redirected_but_too_long_to_open() {
        use crate::schema::solution_shares as shares;
        use diesel::prelude::*;
        use shared::glue::{Feature, Glue, MAX_GLUES};
        let Some(db_pool) = test_db() else {
            eprintln!("TEST_DATABASE_URL not set; skipping");
            return;
        };
        let app = build_app(Arc::new(AppState {
            dev_mode: true,
            db_pool: db_pool.clone(),
            public_url: TEST_URL.into(),
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let arr = shared::Arrangement {
            n: 100,
            side: 10.0,
            squares: (0..100)
                .map(|i| shared::Placement {
                    cx: (i % 10) as f64 + 0.5,
                    cy: (i / 10) as f64 + 0.5,
                    theta: 0.0,
                })
                .collect(),
        };
        // Distinct corner-to-midpoint pairs, always between different squares.
        let glues: Vec<Glue> = (0..MAX_GLUES)
            .map(|i| {
                let (square, k) = (i % 100, i / 100);
                Glue {
                    a: Feature::Corner {
                        square,
                        corner: (k % 4) as u8,
                    },
                    b: Feature::Midpoint {
                        square: (square + 1 + k / 4) % 100,
                        edge: 0,
                    },
                }
            })
            .collect();
        let code = shared::share::encode(&arr, &glues);
        assert_eq!(code.len(), shared::share::MAX_LEN);
        let body = serde_json::to_string(&shared::CreateShare {
            n: 100,
            code: code.clone(),
        })
        .unwrap();
        let (status, _, reply) = raw_http(
            addr,
            format!(
                "POST /api/shares HTTP/1.1\r\nHost: packit.test\r\n\
                 Content-Type: application/json\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n{body}",
                body.len()
            ),
        )
        .await;
        assert_eq!(status, 200, "{}", String::from_utf8_lossy(&reply));
        let link: shared::ShortShare = serde_json::from_slice(&reply).unwrap();
        let token = link
            .url
            .strip_prefix(&format!("{TEST_URL}/s/"))
            .unwrap()
            .to_string();
        let stored: String = shares::table
            .find(&token)
            .select(shares::code)
            .first(&mut db_pool.get().unwrap())
            .unwrap();
        assert_eq!(stored, code);

        let get = |path: String| {
            format!("GET {path} HTTP/1.1\r\nHost: packit.test\r\nConnection: close\r\n\r\n")
        };
        let (status, head, _) = raw_http(addr, get(format!("/s/{token}"))).await;
        assert_eq!(status, 302, "{head}");
        let location = head
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("location").then(|| value.trim())
            })
            .expect("a Location header");
        assert_eq!(location, format!("{TEST_URL}/play/100?s={code}"));
        assert_eq!(
            shared::share::decode(location.split("?s=").nth(1).unwrap(), 100).unwrap(),
            shared::share::Snapshot {
                arrangement: arr.clone(),
                glues: glues.clone(),
            }
        );
        // hyper rejects request targets over 65,534 bytes (`MAX_URI_LEN`, not
        // configurable), so the redirect target of a code this long can't be
        // opened here. At n = 100 the longest that opens has 3,793 glues.
        let (status, head, _) =
            raw_http(addr, get(location.strip_prefix(TEST_URL).unwrap().into())).await;
        assert_eq!(status, 414, "{head}");
        let page = |glues: &[Glue]| {
            get(format!(
                "/play/100?s={}",
                shared::share::encode(&arr, glues)
            ))
        };
        let (status, head, _) = raw_http(addr, page(&glues[..3794])).await;
        assert_eq!(status, 414, "{head}");
        let (status, head, html) = raw_http(addr, page(&glues[..3793])).await;
        assert_eq!(status, 200, "{head}");
        assert!(String::from_utf8(html)
            .unwrap()
            .contains("/api/preview.png?n=100&amp;s="));
    }
}
