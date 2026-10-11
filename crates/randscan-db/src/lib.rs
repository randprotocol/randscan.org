//! RandScan database layer (PostgreSQL via SQLx).

pub mod error;
pub mod models;
pub mod pool;
pub mod queries;

pub use error::*;
pub use models::*;
pub use pool::*;
pub use queries::*;

use sqlx::postgres::PgPoolOptions;
pub use sqlx::PgPool;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub connect_timeout: Duration,
    pub idle_timeout: Duration,
}

impl DatabaseConfig {
    pub fn from_env() -> Self {
        let num = |k: &str, d: u64| {
            std::env::var(k)
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(d)
        };
        Self {
            url: std::env::var("DATABASE_URL").unwrap_or_else(|_| {
                "postgres://randscan:randscan@localhost:5432/randscan".to_string()
            }),
            max_connections: num("DB_MAX_CONNECTIONS", 10) as u32,
            min_connections: num("DB_MIN_CONNECTIONS", 1) as u32,
            connect_timeout: Duration::from_secs(num("DB_CONNECT_TIMEOUT", 10)),
            idle_timeout: Duration::from_secs(num("DB_IDLE_TIMEOUT", 300)),
        }
    }
}

pub async fn create_pool(config: &DatabaseConfig) -> Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(config.connect_timeout)
        .idle_timeout(config.idle_timeout)
        .connect(&config.url)
        .await
        .map_err(|e| DbError::Connection(e.to_string()))
}

/// Embedded migrations in order. Add new files here; never edit an applied one.
const MIGRATIONS: &[(i32, &str)] = &[
    (
        1,
        include_str!("../../../migrations/001_initial_schema.sql"),
    ),
    (2, include_str!("../../../migrations/002_accounts.sql")),
    (
        3,
        include_str!("../../../migrations/003_password_resets.sql"),
    ),
    (
        4,
        include_str!("../../../migrations/004_bridge_and_chain_id.sql"),
    ),
    (
        5,
        include_str!("../../../migrations/005_shielded_chain.sql"),
    ),
    (
        6,
        include_str!("../../../migrations/006_note_envelopes.sql"),
    ),
    (7, include_str!("../../../migrations/007_rand_symbol.sql")),
    // Version 8 is burned: the chain-11 receivers migration ran as 8 on the live database
    // (2026-09-16) and was reverted from the tree with the short-address feature. A database that
    // recorded 8 would skip anything reusing that number, so this one is 9.
    (
        9,
        include_str!("../../../migrations/009_hidden_asset_bundle_and_rpl_tokens.sql"),
    ),
    (
        10,
        include_str!("../../../migrations/010_public_input_and_chain_limits.sql"),
    ),
    (
        11,
        include_str!("../../../migrations/011_split_authorisation_and_gas.sql"),
    ),
    (
        12,
        include_str!("../../../migrations/012_program_state_invoke.sql"),
    ),
    (
        13,
        include_str!("../../../migrations/013_bridge_fees_and_audit_v6.sql"),
    ),
    (
        14,
        include_str!("../../../migrations/014_perps_multisig_aggregate.sql"),
    ),
];

/// Advisory lock key used to serialize migration runs across concurrent callers. The
/// constant spells "RANDSCAN" in ASCII bytes, reinterpreted as a signed 64-bit integer.
const MIGRATION_LOCK_KEY: i64 = 0x52414E445343414E;

/// Apply every migration that `schema_migrations` does not record yet, each in its own transaction.
///
/// Databases created before the runner existed have `blocks` but no `schema_migrations`; they are
/// recorded as version 1 first.
///
/// Concurrent callers (e.g. multiple test binaries running in parallel against a fresh
/// database) would otherwise race on `CREATE TABLE IF NOT EXISTS schema_migrations` and
/// on applying the same migration twice. To prevent that, the whole runner executes on a
/// single pooled connection while holding a Postgres advisory lock (`pg_advisory_lock`),
/// which is released on every exit path, including errors.
pub async fn run_migrations(pool: &PgPool) -> Result<()> {
    let mut conn = pool.acquire().await?;

    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(MIGRATION_LOCK_KEY)
        .execute(&mut *conn)
        .await?;

    let result = run_migrations_locked(&mut conn).await;

    if let Err(e) = sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(MIGRATION_LOCK_KEY)
        .execute(&mut *conn)
        .await
    {
        tracing::warn!("failed to release migration advisory lock: {}", e);
    }

    result
}

/// The body of `run_migrations`, executed on a single connection while the advisory lock
/// from `run_migrations` is held.
///
/// Each migration's statements and its `schema_migrations` insert are wrapped in a plain
/// `BEGIN`/`COMMIT`/`ROLLBACK` on that same connection, rather than `sqlx::Transaction`
/// (whose `Executor` impl over a reborrowed `&mut PgConnection` hits a known rustc/sqlx
/// higher-ranked-trait-bound inference limitation once this function is spawned onto a
/// task, surfacing as "implementation of `Executor` is not general enough").
async fn run_migrations_locked(conn: &mut sqlx::PgConnection) -> Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )",
    )
    .execute(&mut *conn)
    .await?;

    let blocks_exist: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT FROM information_schema.tables WHERE table_name = 'blocks')",
    )
    .fetch_one(&mut *conn)
    .await?;

    if blocks_exist {
        let ok: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT FROM information_schema.columns WHERE table_name = 'blocks' AND column_name = 'justify_view')",
        )
        .fetch_one(&mut *conn)
        .await?;
        if !ok {
            return Err(DbError::Migration(
                "database has the legacy schema; drop and recreate the database".into(),
            ));
        }
        sqlx::query("INSERT INTO schema_migrations (version) VALUES (1) ON CONFLICT DO NOTHING")
            .execute(&mut *conn)
            .await?;
    }

    let applied: Vec<i32> = sqlx::query_scalar("SELECT version FROM schema_migrations")
        .fetch_all(&mut *conn)
        .await?;

    for (version, sql) in MIGRATIONS {
        if applied.contains(version) {
            continue;
        }
        tracing::info!("applying migration {}", version);

        sqlx::query("BEGIN").execute(&mut *conn).await?;

        // Executed via the fully-qualified `Executor` form (rather than `sqlx::raw_sql(sql)
        // .execute(&mut *conn)`) to pin the `Database` type eagerly: with the method-call
        // form, rustc's trait solver fails to prove `Executor` is implemented generally
        // enough once this function's future is spawned onto a new task (observed as
        // "implementation of `Executor` is not general enough"), because `RawSql`'s
        // `Execute` impl is generic over every `Database`, not just `Postgres`.
        if let Err(e) =
            <&mut sqlx::PgConnection as sqlx::Executor>::execute(&mut *conn, sqlx::raw_sql(sql))
                .await
        {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            return Err(DbError::Migration(format!("migration {}: {}", version, e)));
        }

        if let Err(e) = sqlx::query("INSERT INTO schema_migrations (version) VALUES ($1)")
            .bind(version)
            .execute(&mut *conn)
            .await
        {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            return Err(e.into());
        }

        sqlx::query("COMMIT").execute(&mut *conn).await?;
    }
    Ok(())
}
