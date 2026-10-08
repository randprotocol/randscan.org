//! Shared harness: a real database (`DATABASE_URL`), a stub indexer, `oneshot` requests.

#![allow(dead_code)]

pub mod mock_node;

use axum::{
    body::Body,
    http::{HeaderMap, Request, StatusCode},
    Router,
};
use http_body_util::BodyExt;
use randscan_api::{create_router, mail::MailSender, ratelimit::RateLimiter, ApiConfig, AppState};
use randscan_db::{run_migrations, DbPool};
use randscan_indexer::{Broadcaster, IndexerConfig, IndexerService};
use randscan_ws::WsManager;
use serde_json::Value;
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

/// Limits off, proxy trusted so tests can pick their own client IP, insecure cookies.
pub fn test_config() -> ApiConfig {
    ApiConfig {
        cookie_secure: false,
        trust_proxy: true,
        anon_rpm: 0,
        key_rpm: 0,
        auth_rpm: 0,
        ..ApiConfig::default()
    }
}

pub async fn test_app(cfg: ApiConfig) -> Option<(Router, PgPool)> {
    test_app_with_mailer(cfg, None).await
}

pub async fn test_app_with_mailer(
    cfg: ApiConfig,
    mailer: Option<Arc<dyn MailSender>>,
) -> Option<(Router, PgPool)> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .expect("connect to DATABASE_URL");
    run_migrations(&pool).await.expect("migrations");
    let db = DbPool::new(pool.clone());
    let indexer = Arc::new(IndexerService::new(
        IndexerConfig::default(),
        db.clone(),
        Broadcaster::new(),
    ));
    let state = AppState {
        db,
        indexer,
        ws_manager: Arc::new(WsManager::new()),
        config: Arc::new(cfg),
        limiter: Arc::new(RateLimiter::new()),
        mailer,
    };
    Some((create_router(state), pool))
}

/// Unique per call, also across tests running in parallel within one binary (the clock alone
/// collides at microsecond resolution and turned into sporadic 409 `email_taken` on signup).
pub fn unique_email() -> String {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("t{nanos}-{}-{n}@example.com", std::process::id())
}

pub fn json_req(
    method: &str,
    uri: &str,
    body: Option<Value>,
    cookie: Option<&str>,
) -> Request<Body> {
    let mut b = Request::builder()
        .method(method)
        .uri(uri)
        .header("x-forwarded-for", "198.51.100.7");
    if let Some(c) = cookie {
        b = b.header("cookie", c);
    }
    match body {
        Some(v) => b
            .header("content-type", "application/json")
            .body(Body::from(v.to_string()))
            .unwrap(),
        None => b.body(Body::empty()).unwrap(),
    }
}

pub async fn call(app: &Router, req: Request<Body>) -> (StatusCode, HeaderMap, Value) {
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, headers, json)
}

/// `name=value` of the session cookie from a Set-Cookie header.
pub fn session_cookie_from(headers: &HeaderMap) -> String {
    headers
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|v| v.starts_with("randscan_session="))
        .and_then(|v| v.split(';').next())
        .expect("session cookie")
        .to_string()
}

/// The API wired to a live indexer that follows the node at `rpc_url` (a mock or a real
/// `rand-node`). `run()` is not started; call `start()` when the test is ready.
pub struct LiveApp {
    pub app: Router,
    pub pool: PgPool,
    pub indexer: Arc<IndexerService>,
    pub broadcaster: Broadcaster,
}

impl LiveApp {
    pub fn start(&self) {
        let indexer = self.indexer.clone();
        tokio::spawn(async move {
            if let Err(e) = indexer.run().await {
                eprintln!("indexer stopped: {e:#}");
            }
        });
    }

    /// Poll `GET path` until `pred(body)` holds; panics with the last body after `timeout`.
    pub async fn wait_for(
        &self,
        path: &str,
        timeout: Duration,
        pred: impl Fn(&Value) -> bool,
    ) -> Value {
        let start = std::time::Instant::now();
        let mut last = Value::Null;
        while start.elapsed() < timeout {
            let (_, _, body) = call(&self.app, json_req("GET", path, None, None)).await;
            if pred(&body) {
                return body;
            }
            last = body;
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("timed out waiting on {path}; last body: {last}");
    }
}

pub async fn live_app(cfg: ApiConfig, rpc_url: &str) -> Option<LiveApp> {
    let url = std::env::var("DATABASE_URL").ok()?;
    // Keep the peer tracker off the network (it would geolocate the host otherwise).
    std::env::set_var("NODE_PUBLIC_IP", "127.0.0.1");
    let pool = PgPoolOptions::new()
        .max_connections(6)
        .connect(&url)
        .await
        .expect("connect to DATABASE_URL");
    run_migrations(&pool).await.expect("migrations");
    let db = DbPool::new(pool.clone());
    let broadcaster = Broadcaster::new();
    let indexer = Arc::new(IndexerService::new(
        IndexerConfig {
            rpc_url: rpc_url.to_string(),
            poll_interval: Duration::from_millis(100),
            batch_size: 50,
            stats_interval: Duration::from_millis(200),
            nodes_interval: Duration::from_secs(3600),
        },
        db.clone(),
        broadcaster.clone(),
    ));
    let state = AppState {
        db,
        indexer: indexer.clone(),
        ws_manager: Arc::new(WsManager::new()),
        config: Arc::new(cfg),
        limiter: Arc::new(RateLimiter::new()),
        mailer: None,
    };
    Some(LiveApp {
        app: create_router(state),
        pool,
        indexer,
        broadcaster,
    })
}
