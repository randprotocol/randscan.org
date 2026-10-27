use crate::{state::AppState, ApiResult};
use axum::{extract::State, Json};
use randscan_core::{classify, ChainResponse, ChainState, ChainView};
use randscan_db as db;

/// Wall-clock milliseconds, the clock block timestamps are measured against.
pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// The indexed data as a [`ChainView`], plus whether the indexer is halted and whether its last
/// request to the node succeeded. The stored chain is `chain_meta` (falling back to the chain id
/// `indexer_state` recorded, with no genesis, before the first pass); the head is the latest
/// indexed block. While halted, the node's height belongs to another chain, so `node_height`
/// and `lag` are left out rather than compared across chains.
pub(crate) async fn chain_view(state: &AppState) -> db::Result<(ChainView, bool, bool)> {
    let pool = state.db.inner();
    let meta = db::get_chain_meta(pool).await?;
    let (chain_id, genesis_hash) = match meta {
        Some(m) => (m.chain_id, Some(m.genesis_hash)),
        None => (
            db::get_indexer_state(pool).await?.chain_id.unwrap_or(0),
            None,
        ),
    };
    let head = db::latest_block_head(pool).await?;
    let s = state.indexer.get_state().await;
    let head_height = head.map(|(h, _)| h).unwrap_or(0);
    let (node_height, lag) = if s.halted || s.node_chain_id.is_none() {
        (None, None)
    } else {
        let indexed = head.map(|(h, _)| h).unwrap_or(-1);
        (Some(s.node_height), Some((s.node_height - indexed).max(0)))
    };
    let view = ChainView {
        chain_id,
        genesis_hash,
        head_height,
        head_timestamp_ms: head.map(|(_, ts)| ts),
        node_height,
        lag,
        node_chain_id: s.node_chain_id,
        node_genesis_hash: s.node_genesis_hash,
    };
    Ok((view, s.halted, s.is_connected))
}

/// GET /api/v1/chain — the explorer's [`ChainView`] and its state against the configured pin
/// (site-sync design §2–§3). The view classifies as the spec's table says. A node the indexer
/// cannot reach turns a `live` or `behind` view into `unreachable`: the data was current a moment
/// ago but nothing confirms it now. Once the head ages past `STALL_MS` the state is `stalled`,
/// which says more about what the reader is looking at.
pub async fn get_chain(State(state): State<AppState>) -> ApiResult<Json<ChainResponse>> {
    let (view, halted, connected) = chain_view(&state).await?;
    let expected = state.indexer.expected();
    let mut c = classify(Some(&view), expected, now_ms());
    if !connected && matches!(c.state, ChainState::Live | ChainState::Behind) {
        c.state = ChainState::Unreachable;
        c.detail = format!(
            "The explorer cannot reach its node; last indexed block #{}.",
            view.head_height
        );
    }
    Ok(Json(ChainResponse {
        view,
        state: c.state,
        detail: c.detail,
        expected_chain_id: expected.chain_id,
        expected_genesis_hash: expected.genesis_hash.clone(),
        halted,
        node_connected: connected,
    }))
}
