//! Integration tests against a real PostgreSQL (`DATABASE_URL`). They skip when it is unset.

use randscan_db::run_migrations;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

pub async fn test_pool() -> Option<PgPool> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .expect("connect to DATABASE_URL");
    run_migrations(&pool).await.expect("migrations");
    Some(pool)
}

#[tokio::test]
async fn migrations_are_versioned_and_idempotent() {
    let Some(pool) = test_pool().await else {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    };
    // Running again must be a no-op.
    run_migrations(&pool).await.expect("second run");

    let versions: Vec<i32> =
        sqlx::query_scalar("SELECT version FROM schema_migrations ORDER BY version")
            .fetch_all(&pool)
            .await
            .unwrap();
    // A long-lived database (this shared one, and the live one) may also record the burned
    // version 8, the reverted chain-11 receivers migration; a fresh database never does.
    let versions: Vec<i32> = versions.into_iter().filter(|v| *v != 8).collect();
    assert_eq!(
        versions,
        vec![1, 2, 3, 4, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15]
    );

    for table in ["users", "sessions", "api_keys"] {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT FROM information_schema.tables WHERE table_name = $1)",
        )
        .bind(table)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(exists, "table {table} missing");
    }
}

use chrono::{Duration, Utc};
use randscan_db as db;

fn unique_email() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("t{nanos}@example.com")
}

#[tokio::test]
async fn users_are_unique_by_email() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let email = unique_email();
    let u = db::create_user(&pool, &email, "hash").await.unwrap();
    assert_eq!(u.email, email);
    assert!(u.last_login_at.is_none());
    let dup = db::create_user(&pool, &email, "hash2").await;
    assert!(matches!(dup, Err(db::DbError::Constraint(_))));
    let found = db::get_user_by_email(&pool, &email).await.unwrap().unwrap();
    assert_eq!(found.id, u.id);
    assert_eq!(found.password_hash, "hash");
    db::touch_last_login(&pool, u.id).await.unwrap();
    assert!(db::get_user(&pool, u.id)
        .await
        .unwrap()
        .unwrap()
        .last_login_at
        .is_some());
}

#[tokio::test]
async fn sessions_expire_and_delete() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let u = db::create_user(&pool, &unique_email(), "hash")
        .await
        .unwrap();
    let live = format!("{:0>64}", format!("{:x}", u.id * 7 + 1));
    let dead = format!("{:0>64}", format!("{:x}", u.id * 7 + 2));
    db::create_session(
        &pool,
        &live,
        u.id,
        Utc::now() + Duration::days(1),
        Some("ua"),
        Some("1.2.3.4"),
    )
    .await
    .unwrap();
    db::create_session(
        &pool,
        &dead,
        u.id,
        Utc::now() - Duration::days(1),
        None,
        None,
    )
    .await
    .unwrap();

    let (s, user) = db::get_session_user(&pool, &live).await.unwrap().unwrap();
    assert_eq!(s.user_id, u.id);
    assert_eq!(user.id, u.id);
    assert!(db::get_session_user(&pool, &dead).await.unwrap().is_none());

    db::delete_expired_sessions(&pool, u.id).await.unwrap();
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE user_id = $1")
        .bind(u.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1);

    let later = Utc::now() + Duration::days(30);
    db::touch_session(&pool, &live, later).await.unwrap();
    let (s, _) = db::get_session_user(&pool, &live).await.unwrap().unwrap();
    assert!((s.expires_at - later).num_seconds().abs() < 2);

    db::delete_session(&pool, &live).await.unwrap();
    assert!(db::get_session_user(&pool, &live).await.unwrap().is_none());
}

#[tokio::test]
async fn api_keys_lifecycle() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let u = db::create_user(&pool, &unique_email(), "hash")
        .await
        .unwrap();
    let hash = format!("{:0>64}", format!("{:x}", u.id * 11 + 3));
    let k = db::create_api_key(&pool, u.id, "bot", "abcdefghijkl", &hash)
        .await
        .unwrap();
    assert_eq!(k.name, "bot");
    assert_eq!(k.request_count, 0);
    assert_eq!(db::count_active_api_keys(&pool, u.id).await.unwrap(), 1);

    let active = db::get_active_api_key(&pool, &hash).await.unwrap().unwrap();
    assert_eq!(active.id, k.id);
    db::record_api_key_use(&pool, k.id).await.unwrap();
    let listed = db::list_api_keys(&pool, u.id).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].request_count, 1);
    assert!(listed[0].last_used_at.is_some());

    // Another user cannot revoke it.
    assert!(!db::revoke_api_key(&pool, u.id + 1_000_000, k.id)
        .await
        .unwrap());
    assert!(db::revoke_api_key(&pool, u.id, k.id).await.unwrap());
    assert!(
        !db::revoke_api_key(&pool, u.id, k.id).await.unwrap(),
        "already revoked"
    );
    assert!(db::get_active_api_key(&pool, &hash)
        .await
        .unwrap()
        .is_none());
    assert_eq!(db::count_active_api_keys(&pool, u.id).await.unwrap(), 0);
    assert!(db::list_api_keys(&pool, u.id).await.unwrap()[0]
        .revoked_at
        .is_some());
}

#[tokio::test]
async fn delete_all_expired_sessions_removes_only_expired() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let u = db::create_user(&pool, &unique_email(), "hash")
        .await
        .unwrap();
    let live = format!("{:0>64}", format!("{:x}", u.id * 13 + 1));
    let dead = format!("{:0>64}", format!("{:x}", u.id * 13 + 2));
    db::create_session(
        &pool,
        &live,
        u.id,
        Utc::now() + Duration::days(1),
        Some("ua"),
        Some("1.2.3.4"),
    )
    .await
    .unwrap();
    db::create_session(
        &pool,
        &dead,
        u.id,
        Utc::now() - Duration::days(1),
        None,
        None,
    )
    .await
    .unwrap();

    let deleted = db::delete_all_expired_sessions(&pool).await.unwrap();
    assert!(
        deleted >= 1,
        "expected at least the expired session removed"
    );

    assert!(db::get_session_user(&pool, &dead).await.unwrap().is_none());
    let (s, _) = db::get_session_user(&pool, &live).await.unwrap().unwrap();
    assert_eq!(s.user_id, u.id);
}

/// Build a maintenance-DB connection URL (same host/credentials, `postgres` database) from
/// `DATABASE_URL`, and one for a scratch database with the given name.
fn maintenance_and_scratch_urls(scratch_db: &str) -> (String, String) {
    let base = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let idx = base.rfind('/').expect("DATABASE_URL has a path");
    let prefix = &base[..idx];
    (
        format!("{prefix}/postgres"),
        format!("{prefix}/{scratch_db}"),
    )
}

/// Migration 015 on a database that already indexes a chain (the live explorer on chain 21)
/// adopts that chain: the recorded chain id and block 0's hash, lowercased. A database with no
/// block 0 gets no row, and the indexer's first pass records the node's chain.
#[tokio::test]
async fn migration_15_adopts_the_indexed_chain() {
    if std::env::var("DATABASE_URL").is_err() {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let scratch_db = format!("randscan_mig15_{nanos}");
    let (maintenance_url, scratch_url) = maintenance_and_scratch_urls(&scratch_db);
    let admin_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&maintenance_url)
        .await
        .expect("connect to maintenance database");
    sqlx::query(&format!("CREATE DATABASE {scratch_db}"))
        .execute(&admin_pool)
        .await
        .expect("create scratch database");
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&scratch_url)
        .await
        .expect("connect to scratch database");

    // A fresh database: no block 0, no row.
    run_migrations(&pool).await.unwrap();
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chain_meta")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);

    // Wind back to a version-14 database holding chain 21, then migrate it.
    sqlx::query("DROP TABLE chain_meta")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM schema_migrations WHERE version = 15")
        .execute(&pool)
        .await
        .unwrap();
    let genesis = "CA76C2766ED6090AC6BA5DAD86B724DD5D2B80C0EDFAED287CD8B129E663CA9A";
    sqlx::query(
        "INSERT INTO blocks (hash, height, view, parent, proposer, timestamp_ms, tx_root, state_root, justify_view, tx_count)
         VALUES ($1, 0, 0, $2, 'p', 1, 'r', 's', 0, 0)",
    )
    .bind(genesis)
    .bind("00".repeat(32))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE indexer_state SET chain_id = 21 WHERE id = 1")
        .execute(&pool)
        .await
        .unwrap();
    run_migrations(&pool).await.unwrap();
    let meta = db::get_chain_meta(&pool).await.unwrap();
    assert_eq!(
        meta,
        Some(db::ChainMeta {
            chain_id: 21,
            genesis_hash: genesis.to_ascii_lowercase(),
        })
    );

    pool.close().await;
    sqlx::query(&format!("DROP DATABASE IF EXISTS {scratch_db}"))
        .execute(&admin_pool)
        .await
        .expect("drop scratch database");
}

#[tokio::test]
async fn concurrent_migrations_do_not_race() {
    if std::env::var("DATABASE_URL").is_err() {
        eprintln!("skipping: DATABASE_URL unset");
        return;
    }

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let scratch_db = format!("randscan_mig_{nanos}");
    let (maintenance_url, scratch_url) = maintenance_and_scratch_urls(&scratch_db);

    let admin_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&maintenance_url)
        .await
        .expect("connect to maintenance database");
    sqlx::query(&format!("CREATE DATABASE {scratch_db}"))
        .execute(&admin_pool)
        .await
        .expect("create scratch database");

    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(&scratch_url)
        .await
        .expect("connect to scratch database");

    let mut handles = Vec::new();
    for _ in 0..8 {
        let pool = pool.clone();
        handles.push(tokio::spawn(async move { run_migrations(&pool).await }));
    }
    for h in handles {
        h.await.expect("task join").expect("run_migrations");
    }

    let versions: Vec<i32> =
        sqlx::query_scalar("SELECT version FROM schema_migrations ORDER BY version")
            .fetch_all(&pool)
            .await
            .unwrap();
    // A fresh database: every migration in the tree once, and never the burned version 8.
    assert_eq!(
        versions,
        vec![1, 2, 3, 4, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15]
    );

    pool.close().await;
    sqlx::query(&format!("DROP DATABASE IF EXISTS {scratch_db}"))
        .execute(&admin_pool)
        .await
        .expect("drop scratch database");
}

#[tokio::test]
async fn password_resets_are_single_use_and_replace_earlier_tokens() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let u = db::create_user(&pool, &unique_email(), "hash")
        .await
        .unwrap();
    let h = |n: i64| format!("{:0>64}", format!("{:x}", u.id * 31 + n));

    db::create_password_reset(&pool, u.id, &h(1), Utc::now() + Duration::hours(1))
        .await
        .unwrap();
    db::create_password_reset(&pool, u.id, &h(2), Utc::now() + Duration::hours(1))
        .await
        .unwrap();
    // Issuing a second token invalidates the first.
    assert_eq!(
        db::consume_password_reset(&pool, &h(1)).await.unwrap(),
        None
    );
    assert_eq!(
        db::consume_password_reset(&pool, &h(2)).await.unwrap(),
        Some(u.id)
    );
    // Single use.
    assert_eq!(
        db::consume_password_reset(&pool, &h(2)).await.unwrap(),
        None
    );
    // Expired tokens are never consumed.
    db::create_password_reset(&pool, u.id, &h(3), Utc::now() - Duration::minutes(1))
        .await
        .unwrap();
    assert_eq!(
        db::consume_password_reset(&pool, &h(3)).await.unwrap(),
        None
    );
    // Unknown token.
    assert_eq!(
        db::consume_password_reset(&pool, &h(99)).await.unwrap(),
        None
    );
    // The reaper removes used and expired rows.
    let n = db::delete_expired_password_resets(&pool).await.unwrap();
    assert!(n >= 2, "removed used + expired rows, got {n}");
}

#[tokio::test]
async fn update_password_and_delete_sessions_except_current() {
    let Some(pool) = test_pool().await else {
        return;
    };
    let u = db::create_user(&pool, &unique_email(), "old")
        .await
        .unwrap();
    db::update_password(&pool, u.id, "new").await.unwrap();
    assert_eq!(
        db::get_user(&pool, u.id)
            .await
            .unwrap()
            .unwrap()
            .password_hash,
        "new"
    );

    let s1 = format!("{:0>64}", format!("{:x}", u.id * 37 + 1));
    let s2 = format!("{:0>64}", format!("{:x}", u.id * 37 + 2));
    for s in [&s1, &s2] {
        db::create_session(&pool, s, u.id, Utc::now() + Duration::days(1), None, None)
            .await
            .unwrap();
    }
    assert_eq!(
        db::delete_user_sessions(&pool, u.id, Some(&s1))
            .await
            .unwrap(),
        1
    );
    assert!(
        db::get_session_user(&pool, &s1).await.unwrap().is_some(),
        "kept"
    );
    assert!(
        db::get_session_user(&pool, &s2).await.unwrap().is_none(),
        "deleted"
    );
    assert_eq!(
        db::delete_user_sessions(&pool, u.id, None).await.unwrap(),
        1
    );
    assert!(db::get_session_user(&pool, &s1).await.unwrap().is_none());
}
