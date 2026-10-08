//! Envelopes for the browser-side opener. Everything served here is public chain data (the
//! node serves it to anyone); the explorer holds no key and decrypts nothing. A viewing key
//! or a per-transaction key pasted into the site is used by WebAssembly in the browser only.

use crate::{error::AppError, state::AppState, ApiResult};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use randscan_core::{
    classify_query, NoteEnvelope, NoteEnvelopePage, QueryKind, TransactionEnvelopes, TxKind,
};
use randscan_db as db;
use serde::Deserialize;

fn to_wire(r: db::NoteEnvelopeRow) -> NoteEnvelope {
    let public = r.public_note();
    NoteEnvelope {
        public,
        leaf_index: r.leaf_index,
        cm: r.cm,
        height: r.height,
        tx_hash: r.tx_hash,
        envelope: r.envelope,
    }
}

/// GET /api/v1/transactions/:hash/envelopes — the notes a transaction created, with their
/// envelopes, plus a call's (or an invoke's) sealed input transcript and `h_in`.
pub async fn transaction_envelopes(
    State(state): State<AppState>,
    Path(hash): Path<String>,
) -> ApiResult<Json<TransactionEnvelopes>> {
    let hash = match classify_query(&hash) {
        QueryKind::Hash(h) => h,
        _ => {
            return Err(AppError::BadRequest(
                "transaction hash must be 64 hex characters".into(),
            ))
        }
    };
    let pool = state.db.inner();
    let tx = db::get_transaction_summary(pool, &hash)
        .await?
        .ok_or_else(|| AppError::NotFound("transaction".into()))?;
    let kind = TxKind::parse_lossy(&tx.kind);
    let notes = db::get_note_envelopes_for_tx(pool, &hash)
        .await?
        .into_iter()
        .map(to_wire)
        .collect();
    // A call's transcript, and an invoke's (its proof is a call's, over the transition).
    let (h_in, call_envelope) = if kind.has_receipt() {
        let h_in = db::get_receipt(pool, &hash).await?.map(|r| r.h_in);
        // The transcript lives on the node only (it can be large); fetched on demand.
        let env = match state.indexer.rpc().call_envelope(&hash).await {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("call envelope fetch for {} failed: {}", hash, e);
                None
            }
        };
        (h_in, env)
    } else {
        (None, None)
    };
    Ok(Json(TransactionEnvelopes {
        hash,
        kind,
        notes,
        h_in,
        call_envelope,
    }))
}

#[derive(Debug, Deserialize)]
pub struct EnvelopePageQuery {
    #[serde(default)]
    pub from_leaf: i64,
    #[serde(default = "default_page")]
    pub limit: i64,
}

fn default_page() -> i64 {
    500
}

/// GET /api/v1/envelopes?from_leaf&limit — leaves with envelopes, oldest first, at most 1000
/// per page; a wallet-style history scan walks this from leaf 0 and tries its key on each.
pub async fn list_envelopes(
    State(state): State<AppState>,
    Query(q): Query<EnvelopePageQuery>,
) -> ApiResult<Json<NoteEnvelopePage>> {
    let pool = state.db.inner();
    let from_leaf = q.from_leaf.max(0);
    let limit = q.limit.clamp(1, 1000);
    let rows = db::list_note_envelopes(pool, from_leaf, limit).await?;
    let total_leaves = db::count_notes(pool).await?;
    let next_leaf = if rows.len() as i64 == limit {
        rows.last().map(|r| r.leaf_index + 1)
    } else {
        None
    };
    Ok(Json(NoteEnvelopePage {
        from_leaf,
        next_leaf,
        total_leaves,
        notes: rows.into_iter().map(to_wire).collect(),
    }))
}
