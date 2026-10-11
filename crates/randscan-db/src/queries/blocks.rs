use crate::{BlockRow, Result};
use sqlx::{PgConnection, PgPool};

const COLS: &str = "hash, height, view, parent, proposer, timestamp_ms, tx_root, state_root, justify_view, tx_count";

pub struct NewBlock<'a> {
    pub hash: &'a str,
    pub height: i64,
    pub view: i64,
    pub parent: &'a str,
    pub proposer: &'a str,
    pub timestamp_ms: i64,
    pub tx_root: &'a str,
    pub state_root: &'a str,
    pub justify_view: i64,
    pub tx_count: i32,
}

pub async fn insert_block(conn: &mut PgConnection, b: &NewBlock<'_>) -> Result<()> {
    sqlx::query(
        "INSERT INTO blocks (hash, height, view, parent, proposer, timestamp_ms, tx_root, state_root, justify_view, tx_count)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(b.hash)
    .bind(b.height)
    .bind(b.view)
    .bind(b.parent)
    .bind(b.proposer)
    .bind(b.timestamp_ms)
    .bind(b.tx_root)
    .bind(b.state_root)
    .bind(b.justify_view)
    .bind(b.tx_count)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn get_block_by_height(pool: &PgPool, height: i64) -> Result<Option<BlockRow>> {
    let sql = format!("SELECT {COLS} FROM blocks WHERE height = $1");
    Ok(sqlx::query_as::<_, BlockRow>(&sql)
        .bind(height)
        .fetch_optional(pool)
        .await?)
}

pub async fn get_block_by_hash(pool: &PgPool, hash: &str) -> Result<Option<BlockRow>> {
    let sql = format!("SELECT {COLS} FROM blocks WHERE hash = $1");
    Ok(sqlx::query_as::<_, BlockRow>(&sql)
        .bind(hash)
        .fetch_optional(pool)
        .await?)
}

pub async fn get_block_hash_at(pool: &PgPool, height: i64) -> Result<Option<String>> {
    Ok(
        sqlx::query_scalar("SELECT hash FROM blocks WHERE height = $1")
            .bind(height)
            .fetch_optional(pool)
            .await?,
    )
}

pub async fn get_latest_blocks(pool: &PgPool, limit: i64) -> Result<Vec<BlockRow>> {
    let sql = format!("SELECT {COLS} FROM blocks ORDER BY height DESC LIMIT $1");
    Ok(sqlx::query_as::<_, BlockRow>(&sql)
        .bind(limit)
        .fetch_all(pool)
        .await?)
}

pub async fn list_blocks(
    pool: &PgPool,
    offset: i64,
    limit: i64,
    proposer: Option<&str>,
) -> Result<Vec<BlockRow>> {
    let sql = format!(
        "SELECT {COLS} FROM blocks WHERE ($1::text IS NULL OR proposer = $1) ORDER BY height DESC LIMIT $2 OFFSET $3"
    );
    Ok(sqlx::query_as::<_, BlockRow>(&sql)
        .bind(proposer)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?)
}

pub async fn count_blocks(pool: &PgPool, proposer: Option<&str>) -> Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT COUNT(*) FROM blocks WHERE ($1::text IS NULL OR proposer = $1)")
            .bind(proposer)
            .fetch_one(pool)
            .await?,
    )
}

pub async fn max_block_height(pool: &PgPool) -> Result<Option<i64>> {
    Ok(sqlx::query_scalar("SELECT MAX(height) FROM blocks")
        .fetch_one(pool)
        .await?)
}

/// Average block interval over the last `n` blocks, in milliseconds.
pub async fn avg_block_time_ms(pool: &PgPool, n: i64) -> Result<f64> {
    // MIN and MAX are NULL over an empty table, so they decode as `Option`.
    let row: Option<(Option<i64>, Option<i64>, i64)> = sqlx::query_as(
        "SELECT MIN(timestamp_ms), MAX(timestamp_ms), COUNT(*) FROM (SELECT timestamp_ms FROM blocks ORDER BY height DESC LIMIT $1) t",
    )
    .bind(n)
    .fetch_optional(pool)
    .await?;
    Ok(match row {
        Some((Some(min), Some(max), count)) if count > 1 => (max - min) as f64 / (count - 1) as f64,
        _ => 0.0,
    })
}

/// Delete every block at or above `height` (transactions, receipts, programs cascade).
pub async fn delete_blocks_from(conn: &mut PgConnection, height: i64) -> Result<u64> {
    let r = sqlx::query("DELETE FROM blocks WHERE height >= $1")
        .bind(height)
        .execute(conn)
        .await?;
    Ok(r.rows_affected())
}
