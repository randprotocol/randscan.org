//! `AppError` as an HTTP response, and the fixed-window limiter's bookkeeping. No database.

use axum::{body::to_bytes, http::StatusCode, response::IntoResponse};
use randscan_api::ratelimit::{Decision, RateLimiter};
use randscan_api::AppError;
use serde_json::Value;

async fn render(e: AppError) -> (StatusCode, Option<String>, Value) {
    let res = e.into_response();
    let status = res.status();
    let retry = res
        .headers()
        .get("retry-after")
        .map(|v| v.to_str().unwrap().to_string());
    let bytes = to_bytes(res.into_body(), 1 << 20).await.unwrap();
    (status, retry, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn every_error_has_its_status_and_body() {
    let (s, retry, b) = render(AppError::NotFound("block".into())).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    assert!(retry.is_none());
    assert_eq!(b["error"], "not_found");

    let (s, _, b) = render(AppError::BadRequest("bad height".into())).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(b["message"].as_str().unwrap().contains("bad height"));

    let (s, _, b) = render(AppError::conflict("email_taken", "taken")).await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert_eq!(b["error"], "email_taken");
    let (s, _, b) = render(AppError::unauthorized("unauthorized", "sign in")).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert_eq!(b["message"], "sign in");
    let (s, _, b) = render(AppError::bad("weak_password", "too short")).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(b["error"], "weak_password");

    let (s, retry, b) = render(AppError::TooManyRequests { retry_after: 17 }).await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(retry.as_deref(), Some("17"));
    assert_eq!(b["error"], "rate_limited");
    assert!(b["message"].as_str().unwrap().contains("17 seconds"));
}

#[tokio::test]
async fn internal_failures_do_not_leak_their_cause() {
    let (s, _, b) = render(AppError::Internal(
        "connection string postgres://u:secret@h".into(),
    ))
    .await;
    assert_eq!(s, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(b["message"], "internal error");
    assert!(!b.to_string().contains("secret"));

    let db = randscan_db::DbError::Connection("password=hunter2".into());
    let (s, _, b) = render(AppError::from(db)).await;
    assert_eq!(s, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(b["message"], "database error");
    assert!(!b.to_string().contains("hunter2"));
}

#[test]
fn the_window_counts_down_then_limits_then_resets() {
    let l = RateLimiter::new();
    assert!(l.is_empty());
    let t = 1_000_000 - 1_000_000 % 60; // the start of a window
    assert_eq!(
        l.check("ip:a", 2, t + 5),
        Decision::Allowed { remaining: 1 }
    );
    assert_eq!(
        l.check("ip:a", 2, t + 6),
        Decision::Allowed { remaining: 0 }
    );
    assert_eq!(
        l.check("ip:a", 2, t + 20),
        Decision::Limited { retry_after: 40 }
    );
    // Another id has its own count; a limit of 0 means unlimited and is never recorded.
    assert_eq!(
        l.check("ip:b", 2, t + 20),
        Decision::Allowed { remaining: 1 }
    );
    assert_eq!(
        l.check("ip:c", 0, t + 20),
        Decision::Allowed {
            remaining: u32::MAX
        }
    );
    assert_eq!(l.len(), 2);
    // The next window starts afresh.
    assert_eq!(
        l.check("ip:a", 2, t + 60),
        Decision::Allowed { remaining: 1 }
    );
}

#[test]
fn sweeping_drops_only_the_windows_that_have_ended() {
    let l = RateLimiter::new();
    let t = 600_000;
    l.check("old", 5, t + 1);
    l.check("new", 5, t + 61);
    assert_eq!(l.len(), 2);
    l.sweep(t + 61); // "old"'s window ended at t+60; "new"'s runs to t+120
    assert_eq!(l.len(), 1);
    assert_eq!(
        l.check("new", 5, t + 62),
        Decision::Allowed { remaining: 3 }
    );
    l.sweep(t + 120);
    assert!(l.is_empty());
}
