use crate::{IndexerStateRow, Result};
use sqlx::{PgConnection, PgPool};

pub async fn get_indexer_state(pool: &PgPool) -> Result<IndexerStateRow> {
    Ok(sqlx::query_as::<_, IndexerStateRow>(
        "SELECT next_height, last_indexed_hash, is_syncing, chain_id, next_leaf FROM indexer_state WHERE id = 1",
    )
    .fetch_one(pool)
    .await?)
}

pub async fn set_next_height(
    conn: &mut PgConnection,
    next_height: i64,
    last_hash: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "UPDATE indexer_state SET next_height = $1, last_indexed_hash = $2, updated_at = NOW() WHERE id = 1",
    )
    .bind(next_height)
    .bind(last_hash)
    .execute(conn)
    .await?;
    Ok(())
}

/// The next commitment-tree leaf to fetch.
pub async fn set_next_leaf(conn: &mut PgConnection, next_leaf: i64) -> Result<()> {
    sqlx::query("UPDATE indexer_state SET next_leaf = $1, updated_at = NOW() WHERE id = 1")
        .bind(next_leaf)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn set_syncing(pool: &PgPool, syncing: bool) -> Result<()> {
    sqlx::query("UPDATE indexer_state SET is_syncing = $1, updated_at = NOW() WHERE id = 1")
        .bind(syncing)
        .execute(pool)
        .await?;
    Ok(())
}

/// Record which chain the indexed data belongs to.
pub async fn set_chain_id(conn: &mut PgConnection, chain_id: i64) -> Result<()> {
    sqlx::query("UPDATE indexer_state SET chain_id = $1, updated_at = NOW() WHERE id = 1")
        .bind(chain_id)
        .execute(conn)
        .await?;
    Ok(())
}

/// The chain the indexed data belongs to (`chain_meta`, migration 015).
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ChainMeta {
    pub chain_id: i64,
    pub genesis_hash: String,
}

/// `None` before the indexer first reached a node, and right after a chain reset.
pub async fn get_chain_meta(pool: &PgPool) -> Result<Option<ChainMeta>> {
    Ok(
        sqlx::query_as::<_, ChainMeta>(
            "SELECT chain_id, genesis_hash FROM chain_meta WHERE id = 1",
        )
        .fetch_optional(pool)
        .await?,
    )
}

/// Record (or replace) the chain the indexed data belongs to; the hash is stored lowercase.
pub async fn set_chain_meta(
    conn: &mut PgConnection,
    chain_id: i64,
    genesis_hash: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO chain_meta (id, chain_id, genesis_hash) VALUES (1, $1, LOWER($2))
         ON CONFLICT (id) DO UPDATE SET chain_id = EXCLUDED.chain_id,
             genesis_hash = EXCLUDED.genesis_hash, adopted_at = NOW(), updated_at = NOW()",
    )
    .bind(chain_id)
    .bind(genesis_hash)
    .execute(conn)
    .await?;
    Ok(())
}

/// Tables derived from the chain, in dependency order. Everything else (users, sessions,
/// API keys, password resets, the `node_geo` IP cache) is independent of the chain and kept.
pub const CHAIN_TABLES: &[&str] = &[
    "nullifiers",
    "notes",
    "receipts",
    "programs",
    "transactions",
    "blocks",
    "validators",
];

/// Forget every block, transaction, note, nullifier, program and validator and start over at
/// height 0 (and leaf 0) for `chain_id`, and forget the recorded `chain_meta`. Used when the
/// indexer moves to the pinned chain, or on its first pass over data from another chain. Runs in
/// one transaction.
pub async fn reset_chain_data(conn: &mut PgConnection, chain_id: i64) -> Result<()> {
    let tables = CHAIN_TABLES.join(", ");
    sqlx::query("BEGIN").execute(&mut *conn).await?;
    let result: Result<()> = async {
        sqlx::query(&format!("TRUNCATE {tables} RESTART IDENTITY CASCADE"))
            .execute(&mut *conn)
            .await?;
        // The wiped database belongs to no chain until the caller records the new one; if that
        // never happens the indexer's next first pass takes the node's identity.
        sqlx::query("DELETE FROM chain_meta")
            .execute(&mut *conn)
            .await?;
        sqlx::query(
            "UPDATE network_stats SET chain_id = $1, height = 0, view = 0, total_transactions = 0,
                notes = 0, nullifiers = 0, validator_count = 0, active_validator_count = 0,
                total_stake = 0, total_supply = 0, pool_value = NULL, program_count = 0,
                avg_block_time_ms = 0, current_leader = NULL, tree_root = NULL, hc_bundle = NULL,
                epoch = NULL, epoch_blocks = NULL, updated_at = NOW()
             WHERE id = 1",
        )
        .bind(chain_id)
        .execute(&mut *conn)
        .await?;
        sqlx::query(
            "UPDATE indexer_state SET next_height = 0, next_leaf = 0, last_indexed_hash = NULL,
                is_syncing = FALSE, chain_id = $1, updated_at = NOW() WHERE id = 1",
        )
        .bind(chain_id)
        .execute(&mut *conn)
        .await?;
        Ok(())
    }
    .await;
    match result {
        Ok(()) => {
            sqlx::query("COMMIT").execute(&mut *conn).await?;
            Ok(())
        }
        Err(e) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            Err(e)
        }
    }
}
